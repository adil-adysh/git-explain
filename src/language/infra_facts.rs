//! Structural and script syntax facts for infrastructure formats and Kotlin.
//!
//! Extracts bounded, raw syntactic facts with exact byte/line provenance,
//! without inferring cloud deployment state, schema validity, variable values,
//! or unseen external behavior.

use super::facts::{SourceFact, SourceSpan};
use super::grammar::grammar_for_language;
use super::infra::{prepare_json_source, sanitize_jsonc_trailing_commas};
use tree_sitter::{Node, Parser};

/// Check if language belongs to the infrastructure/script/Kotlin family.
pub fn is_infra_or_kotlin_language(language: &str) -> bool {
    matches!(
        language.to_lowercase().as_str(),
        "hcl"
            | "terraform"
            | "tf"
            | "yaml"
            | "yml"
            | "json"
            | "tf.json"
            | "jsonc"
            | "dockerfile"
            | "containerfile"
            | "docker"
            | "bash"
            | "sh"
            | "shell"
            | "powershell"
            | "ps1"
            | "psm1"
            | "psd1"
            | "kotlin"
            | "kt"
            | "kts"
    )
}

fn make_span(source: &str, start_byte: usize, end_byte: usize) -> Option<SourceSpan> {
    if start_byte >= end_byte || end_byte > source.len() {
        return None;
    }
    if !source.is_char_boundary(start_byte) || !source.is_char_boundary(end_byte) {
        return None;
    }
    let bytes = source.as_bytes();
    let start_line = bytes[..start_byte].iter().filter(|&&b| b == b'\n').count() + 1;
    let end_line = bytes[..end_byte.saturating_sub(1)]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1;
    Some(SourceSpan {
        start_byte,
        end_byte,
        start_line,
        end_line,
    })
}

fn add_syntax_fact(
    source: &str,
    start_byte: usize,
    end_byte: usize,
    node_kind: &str,
    out: &mut Vec<SourceFact>,
) {
    if let Some(span) = make_span(source, start_byte, end_byte) {
        let syntax = source[span.start_byte..span.end_byte].to_string();
        out.push(SourceFact::Syntax {
            span,
            node_kind: node_kind.to_string(),
            syntax,
        });
    }
}

/// Extract infrastructure / Kotlin syntax facts.
/// Returns None if the language is not part of this family (so legacy extractors run).
/// Returns Some(empty) if input is malformed, and Some(facts) when valid.
pub fn extract_infra_facts(language: &str, source: &str) -> Option<Vec<SourceFact>> {
    if !is_infra_or_kotlin_language(language) {
        return None;
    }

    let lang_norm = language.to_lowercase();
    let facts = match lang_norm.as_str() {
        "json" | "tf.json" | "jsonc" => extract_json_facts(&lang_norm, source),
        "hcl" | "terraform" | "tf" => extract_hcl_facts(source),
        "yaml" | "yml" => extract_yaml_facts(source),
        "dockerfile" | "containerfile" | "docker" => extract_dockerfile_facts(source),
        "bash" | "sh" | "shell" => extract_bash_facts(source),
        "powershell" | "ps1" | "psm1" | "psd1" => extract_powershell_facts(source),
        "kotlin" | "kt" | "kts" => extract_kotlin_facts(source),
        _ => return None,
    };
    Some(facts.unwrap_or_default())
}

fn extract_json_facts(language: &str, source: &str) -> Option<Vec<SourceFact>> {
    let is_jsonc = language == "jsonc";
    let trimmed = source.trim_start();
    let is_root_attempt = trimmed.starts_with('{') || trimmed.starts_with('[');

    // Validate complete documents before considering member fragments. JSONC
    // documents may begin with comments instead of their opening delimiter.
    if let Some(prepared) = prepare_json_source(source, is_jsonc) {
        let mut parser = Parser::new();
        let grammar = grammar_for_language("json")?;
        parser.set_language(&grammar).ok()?;
        let tree = parser.parse(&prepared, None)?;
        let root = tree.root_node();
        if root.has_error() {
            return Some(vec![]);
        }

        let mut facts = Vec::new();
        collect_json_nodes(root, source, &mut facts);
        Some(facts)
    } else if is_root_attempt {
        Some(vec![])
    } else {
        // Snippet / member fragment without outer braces
        let prefix = "{\n";
        let prefix_len = prefix.len();
        let suffix = "\n}";
        let wrapped = format!("{prefix}{source}{suffix}");

        let prepared = if is_jsonc {
            prepare_json_source(&wrapped, true)?
        } else {
            let masked = sanitize_jsonc_trailing_commas(&wrapped);
            prepare_json_source(&masked, false)?
        };

        let mut parser = Parser::new();
        let grammar = grammar_for_language("json")?;
        parser.set_language(&grammar).ok()?;
        let tree = parser.parse(&prepared, None)?;
        let root = tree.root_node();
        if root.has_error() {
            return Some(vec![]);
        }

        let mut facts = Vec::new();
        let mut cursor = root.walk();
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            if node.kind() == "pair" {
                let start = node.start_byte();
                let end = node.end_byte();
                if start >= prefix_len && end <= prefix_len + source.len() {
                    let orig_start = start - prefix_len;
                    let orig_end = end - prefix_len;
                    add_syntax_fact(source, orig_start, orig_end, "pair", &mut facts);
                }
            }
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
        Some(facts)
    }
}

fn collect_json_nodes(node: Node, source: &str, out: &mut Vec<SourceFact>) {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "pair" {
            add_syntax_fact(source, n.start_byte(), n.end_byte(), "pair", out);
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn extract_hcl_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("hcl")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "block" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "block",
                    &mut facts,
                );
            }
            "attribute" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "attribute",
                    &mut facts,
                );
            }
            "function_call" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "function_call",
                    &mut facts,
                );
            }
            _ => {}
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Some(facts)
}

fn extract_yaml_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("yaml")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "block_mapping_pair" | "flow_pair" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "mapping_pair",
                    &mut facts,
                );
            }
            "block_sequence" | "flow_sequence" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "sequence",
                    &mut facts,
                );
            }
            "block_scalar" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "literal",
                    &mut facts,
                );
            }
            _ => {}
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Some(facts)
}

fn extract_dockerfile_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("dockerfile")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if kind.ends_with("_instruction") || kind == "instruction" {
            add_syntax_fact(
                source,
                child.start_byte(),
                child.end_byte(),
                kind,
                &mut facts,
            );
        }
    }
    Some(facts)
}

fn extract_bash_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("bash")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "function_definition" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "function_definition",
                    &mut facts,
                );
            }
            "command" => {
                let bytes = source.as_bytes();
                let is_return = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .map(|name| name == "return")
                    .unwrap_or(false);
                let kind = if is_return { "return" } else { "command" };
                add_syntax_fact(source, node.start_byte(), node.end_byte(), kind, &mut facts);
            }
            "pipeline" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "pipeline",
                    &mut facts,
                );
            }
            "variable_assignment" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "assignment",
                    &mut facts,
                );
            }
            _ => {}
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Some(facts)
}

fn extract_powershell_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("powershell")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "function_statement" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "function_statement",
                    &mut facts,
                );
            }
            "command" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "command",
                    &mut facts,
                );
            }
            "pipeline" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "pipeline",
                    &mut facts,
                );
            }
            "assignment_expression" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "assignment",
                    &mut facts,
                );
            }
            "return_statement" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "return",
                    &mut facts,
                );
            }
            _ => {}
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Some(facts)
}

fn extract_kotlin_facts(source: &str) -> Option<Vec<SourceFact>> {
    let mut parser = Parser::new();
    let grammar = grammar_for_language("kotlin")?;
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return Some(vec![]);
    }

    let mut facts = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            "function_declaration"
            | "class_declaration"
            | "property_declaration"
            | "object_declaration" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "declaration",
                    &mut facts,
                );
            }
            "call_expression" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "call",
                    &mut facts,
                );
            }
            "comparison_expression" | "equality_expression" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "comparison",
                    &mut facts,
                );
            }
            "return_expression" | "jump_expression" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "return",
                    &mut facts,
                );
            }
            "user_type" | "nullable_type" => {
                add_syntax_fact(
                    source,
                    node.start_byte(),
                    node.end_byte(),
                    "type",
                    &mut facts,
                );
            }
            _ => {}
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Some(facts)
}
