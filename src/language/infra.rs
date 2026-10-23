use super::{smallest_units, source_for_lines, LanguageAnalyzer, SourceUnit, SourceUnitKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

// =========================================================================
// HCL Analyzer (.tf, .tfvars, .hcl)
// =========================================================================

pub struct HclAnalyzer;

impl HclAnalyzer {
    fn extract_block_name(node: Node, source: &[u8]) -> String {
        let mut parts = Vec::new();
        for child in node.children(&mut node.walk()) {
            if child.kind() == "identifier" || child.kind() == "string_lit" {
                if let Ok(text) = child.utf8_text(source) {
                    parts.push(text.trim());
                }
            } else if child.kind() == "body" || child.kind() == "block_start" || child.kind() == "{"
            {
                break;
            }
        }
        if parts.is_empty() {
            "block".to_string()
        } else {
            parts.join(" ")
        }
    }
}

impl LanguageAnalyzer for HclAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.ends_with(".tf.json") {
                return false;
            }
        }
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "tf" | "tfvars" | "hcl"))
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_hcl::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut units = Vec::new();
        // Root is config_file; its direct child is body or object
        let body_node = root
            .children(&mut root.walk())
            .find(|c| c.kind() == "body")
            .unwrap_or(root);

        for child in body_node.children(&mut body_node.walk()) {
            match child.kind() {
                "block" => {
                    let name = Self::extract_block_name(child, bytes);
                    let start_line = child.start_position().row + 1;
                    let end_line = child.end_position().row + 1;
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: SourceUnitKind::TopLevelBlock,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                "attribute" => {
                    let name = child
                        .child_by_field_name("key")
                        .or_else(|| {
                            child
                                .children(&mut child.walk())
                                .find(|c| c.kind() == "identifier")
                        })
                        .and_then(|c| c.utf8_text(bytes).ok())
                        .unwrap_or("attribute")
                        .to_string();
                    let start_line = child.start_position().row + 1;
                    let end_line = child.end_position().row + 1;
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: SourceUnitKind::Property,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                _ => {}
            }
        }

        Ok(smallest_units(units, ranges))
    }
}

// =========================================================================
// YAML Analyzer (.yaml, .yml)
// =========================================================================

pub struct YamlAnalyzer;

impl YamlAnalyzer {
    fn extract_key_name(pair: Node, source: &[u8]) -> String {
        // block_mapping_pair has field "key" or first child
        if let Some(key_node) = pair.child_by_field_name("key") {
            if let Ok(text) = key_node.utf8_text(source) {
                return text.trim().to_string();
            }
        }
        for child in pair.children(&mut pair.walk()) {
            if child.is_named() {
                if let Ok(text) = child.utf8_text(source) {
                    return text.trim().to_string();
                }
            }
        }
        "entry".to_string()
    }

    fn find_block_mapping(node: Node) -> Option<Node> {
        if node.kind() == "block_mapping" {
            return Some(node);
        }
        for child in node.children(&mut node.walk()) {
            if child.kind() == "block_mapping" {
                return Some(child);
            }
            if child.kind() == "block_node" {
                if let Some(m) = Self::find_block_mapping(child) {
                    return Some(m);
                }
            }
        }
        None
    }
}

impl LanguageAnalyzer for YamlAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "yaml" | "yml"))
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_yaml::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut units = Vec::new();

        // Root is "stream", containing "document" nodes
        for doc in root.children(&mut root.walk()) {
            if doc.kind() != "document" {
                continue;
            }

            if let Some(mapping) = Self::find_block_mapping(doc) {
                for pair in mapping.children(&mut mapping.walk()) {
                    if pair.kind() == "block_mapping_pair" {
                        let name = Self::extract_key_name(pair, bytes);
                        let start_line = pair.start_position().row + 1;
                        let end_line = pair.end_position().row + 1;
                        units.push(SourceUnit {
                            name,
                            qualified_name: None,
                            kind: SourceUnitKind::TopLevelBlock,
                            start_line,
                            end_line,
                            source: source_for_lines(source, start_line, end_line),
                        });
                    }
                }
            } else {
                // If document does not have a top-level block mapping, treat whole document as one unit
                let start_line = doc.start_position().row + 1;
                let end_line = doc.end_position().row + 1;
                let name = "document".to_string();
                units.push(SourceUnit {
                    name,
                    qualified_name: None,
                    kind: SourceUnitKind::TopLevelBlock,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            }
        }

        Ok(smallest_units(units, ranges))
    }
}

// =========================================================================
// JSON & JSONC Analyzers
// =========================================================================

pub struct JsonAnalyzer;
pub struct JsoncAnalyzer;

/// String/comment-aware, length-preserving replacement of ONLY trailing separator commas with ASCII space.
/// Byte length, offsets, and line/col layout are strictly preserved.
pub fn sanitize_jsonc_trailing_commas(source: &str) -> String {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut out = bytes.to_vec();

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' => {
                // Skip string literal
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                // Line comment
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                // Block comment
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < len {
                    i += 2;
                }
            }
            b',' => {
                // Check if this comma is a trailing separator comma before } or ]
                let comma_pos = i;
                let mut peek = i + 1;
                let mut is_trailing = false;

                while peek < len {
                    let b = bytes[peek];
                    if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                        peek += 1;
                    } else if b == b'/' && peek + 1 < len && bytes[peek + 1] == b'/' {
                        // Skip line comment
                        peek += 2;
                        while peek < len && bytes[peek] != b'\n' {
                            peek += 1;
                        }
                    } else if b == b'/' && peek + 1 < len && bytes[peek + 1] == b'*' {
                        // Skip block comment
                        peek += 2;
                        while peek < len && !(bytes[peek] == b'*' && bytes[peek + 1] == b'/') {
                            peek += 1;
                        }
                        if peek + 1 < len {
                            peek += 2;
                        }
                    } else if b == b'}' || b == b']' {
                        is_trailing = true;
                        break;
                    } else {
                        break;
                    }
                }

                if is_trailing {
                    out[comma_pos] = b' ';
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Safety: only commas (ASCII 0x2C) were replaced by spaces (ASCII 0x20), maintaining valid UTF-8
    String::from_utf8(out).unwrap_or_else(|_| source.to_string())
}

/// Validate JSON/JSONC without changing the original source's byte or line positions.
/// The returned parser text replaces trailing JSONC commas only; comments remain syntax.
pub fn prepare_json_source(source: &str, jsonc: bool) -> Option<String> {
    let parsed = if jsonc {
        sanitize_jsonc_trailing_commas(source)
    } else {
        source.to_owned()
    };
    let mut validation = parsed.as_bytes().to_vec();
    let mut i = 0;
    while jsonc && i < validation.len() {
        if validation[i] == b'"' {
            i += 1;
            while i < validation.len() {
                match validation[i] {
                    b'\\' => i = (i + 2).min(validation.len()),
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
        } else if validation[i..].starts_with(b"//") {
            while i < validation.len() && !matches!(validation[i], b'\n' | b'\r') {
                validation[i] = b' ';
                i += 1;
            }
        } else if validation[i..].starts_with(b"/*") {
            validation[i..i + 2].fill(b' ');
            i += 2;
            while i < validation.len() && !validation[i..].starts_with(b"*/") {
                if !matches!(validation[i], b'\n' | b'\r') {
                    validation[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= validation.len() {
                return None;
            }
            validation[i..i + 2].fill(b' ');
            i += 2;
        } else {
            i += 1;
        }
    }
    serde_json::from_slice::<serde_json::Value>(&validation).ok()?;
    Some(parsed)
}

/// Identity follows the statement's syntax, not mutable argument values.
pub(crate) fn statement_identity(node: Node, source: &[u8]) -> String {
    node.child_by_field_name("name")
        .filter(|name| {
            matches!(
                name.kind(),
                "command_name" | "identifier" | "word" | "variable_name"
            )
        })
        .and_then(|name| name.utf8_text(source).ok())
        .map(str::to_owned)
        .unwrap_or_else(|| node.kind().replace('_', " "))
}

fn extract_json_units(source: &str, ranges: &[LineRange], jsonc: bool) -> Result<Vec<SourceUnit>> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_json::LANGUAGE.into())?;

    let Some(parse_text) = prepare_json_source(source, jsonc) else {
        return Ok(vec![]);
    };
    let tree = match parser.parse(&parse_text, None) {
        Some(t) => t,
        None => return Ok(vec![]),
    };

    let root = tree.root_node();
    // If parse has error, return empty so coarse fallback handles it
    if root.has_error() {
        return Ok(vec![]);
    }

    let bytes = source.as_bytes();
    let mut units = Vec::new();

    // In tree-sitter-json: root is "document"
    for child in root.children(&mut root.walk()) {
        match child.kind() {
            "object" => {
                let mut members = Vec::new();
                for pair in child.children(&mut child.walk()) {
                    if pair.kind() == "pair" {
                        let name = pair
                            .child_by_field_name("key")
                            .and_then(|k| k.utf8_text(bytes).ok())
                            .unwrap_or("property")
                            .to_string();
                        let start_line = pair.start_position().row + 1;
                        let end_line = pair.end_position().row + 1;
                        members.push(SourceUnit {
                            name,
                            qualified_name: None,
                            kind: SourceUnitKind::Property,
                            start_line,
                            end_line,
                            source: source_for_lines(source, start_line, end_line),
                        });
                    }
                }
                if members.is_empty()
                    || members
                        .windows(2)
                        .any(|pair| pair[0].end_line >= pair[1].start_line)
                {
                    let start_line = child.start_position().row + 1;
                    let end_line = child.end_position().row + 1;
                    units.push(SourceUnit {
                        name: "object".into(),
                        qualified_name: None,
                        kind: SourceUnitKind::TopLevelBlock,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                } else {
                    units.extend(members);
                }
            }
            "array" => {
                let start_line = child.start_position().row + 1;
                let end_line = child.end_position().row + 1;
                units.push(SourceUnit {
                    name: "array".to_string(),
                    qualified_name: None,
                    kind: SourceUnitKind::TopLevelBlock,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            }
            _ => {}
        }
    }

    Ok(smallest_units(units, ranges))
}

impl LanguageAnalyzer for JsonAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.ends_with(".tf.json") {
                return true;
            }
        }
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "json")
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        // Strict JSON: no sanitization of trailing commas/comments
        extract_json_units(source, ranges, false)
    }
}

impl LanguageAnalyzer for JsoncAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "jsonc")
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        extract_json_units(source, ranges, true)
    }
}

// =========================================================================
// Dockerfile / Containerfile Analyzer
// =========================================================================

pub struct DockerfileAnalyzer;

impl DockerfileAnalyzer {
    pub fn is_dockerfile_path(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            let lower = name.to_ascii_lowercase();
            if lower == "dockerfile" || lower == "containerfile" {
                return true;
            }
            let parts: Vec<&str> = lower.split('.').collect();
            if parts.len() == 2 {
                if parts[0] == "dockerfile"
                    || parts[1] == "dockerfile"
                    || parts[0] == "containerfile"
                    || parts[1] == "containerfile"
                {
                    return true;
                }
            }
            false
        } else {
            false
        }
    }
}

impl LanguageAnalyzer for DockerfileAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        Self::is_dockerfile_path(path)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_containerfile::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();

        let mut units = Vec::new();

        // In tree-sitter-containerfile: root is "source_file", containing "*_instruction"
        for child in root.children(&mut root.walk()) {
            if child.kind().ends_with("_instruction") || child.kind() == "instruction" {
                let start_line = child.start_position().row + 1;
                let end_line = child.end_position().row + 1;

                // Build a concise instruction name from the first line
                let full_line = source_for_lines(source, start_line, start_line);
                let name = full_line.trim().to_string();
                let name = if name.is_empty() {
                    child.kind().to_string()
                } else {
                    name
                };

                units.push(SourceUnit {
                    name,
                    qualified_name: Some(
                        child
                            .kind()
                            .trim_end_matches("_instruction")
                            .to_ascii_uppercase(),
                    ),
                    kind: SourceUnitKind::TopLevelBlock,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            }
        }

        Ok(smallest_units(units, ranges))
    }
}

// =========================================================================
// Bash Analyzer (.sh, .bash)
// =========================================================================

pub struct BashAnalyzer;

impl LanguageAnalyzer for BashAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "sh" | "bash"))
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_bash::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut units = Vec::new();

        for child in root.children(&mut root.walk()) {
            if !child.is_named() {
                continue;
            }

            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            if child.kind() == "function_definition" {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .unwrap_or("function")
                    .to_string();

                units.push(SourceUnit {
                    name,
                    qualified_name: None,
                    kind: SourceUnitKind::Function,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            } else {
                let first_line = source_for_lines(source, start_line, start_line);
                let name = first_line.trim().to_string();
                let name = if name.is_empty() {
                    "statement".to_string()
                } else {
                    name
                };

                units.push(SourceUnit {
                    name,
                    qualified_name: Some(statement_identity(child, bytes)),
                    kind: SourceUnitKind::TopLevelBlock,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            }
        }

        Ok(smallest_units(units, ranges))
    }
}

// =========================================================================
// PowerShell Analyzer (.ps1, .psm1, .psd1)
// =========================================================================

pub struct PowerShellAnalyzer;

impl LanguageAnalyzer for PowerShellAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "ps1" | "psm1" | "psd1"))
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_powershell::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut units = Vec::new();

        // Helper to collect units from statement node
        let mut process_statement = |stmt: Node| {
            if !stmt.is_named() {
                return;
            }
            let start_line = stmt.start_position().row + 1;
            let end_line = stmt.end_position().row + 1;

            if stmt.kind() == "function_statement" {
                let name = stmt
                    .children(&mut stmt.walk())
                    .find(|c| c.kind() == "function_name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .unwrap_or("function")
                    .to_string();

                units.push(SourceUnit {
                    name,
                    qualified_name: None,
                    kind: SourceUnitKind::Function,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            } else {
                let first_line = source_for_lines(source, start_line, start_line);
                let name = first_line.trim().to_string();
                let name = if name.is_empty() {
                    "statement".to_string()
                } else {
                    name
                };

                units.push(SourceUnit {
                    name,
                    qualified_name: Some(statement_identity(stmt, bytes)),
                    kind: SourceUnitKind::TopLevelBlock,
                    start_line,
                    end_line,
                    source: source_for_lines(source, start_line, end_line),
                });
            }
        };

        // Root is program. Statements may be in statement_list or direct children.
        for child in root.children(&mut root.walk()) {
            if child.kind() == "statement_list" {
                for stmt in child.children(&mut child.walk()) {
                    process_statement(stmt);
                }
            } else {
                process_statement(child);
            }
        }

        Ok(smallest_units(units, ranges))
    }
}
