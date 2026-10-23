use super::{smallest_units, source_for_lines, LanguageAnalyzer, SourceUnit, SourceUnitKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct KotlinAnalyzer;

impl KotlinAnalyzer {
    fn extract_identifier(node: Node, source: &[u8]) -> Option<String> {
        if node.kind() == "identifier" || node.kind() == "simple_identifier" {
            return node.utf8_text(source).ok().map(|s| s.to_string());
        }
        for child in node.children(&mut node.walk()) {
            if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                return child.utf8_text(source).ok().map(|s| s.to_string());
            }
        }
        None
    }

    fn extract_function_name(node: Node, source: &[u8]) -> String {
        if let Some(name_node) = node.child_by_field_name("name") {
            if let Ok(text) = name_node.utf8_text(source) {
                return text.to_string();
            }
        }
        let mut last_id = None;
        for child in node.children(&mut node.walk()) {
            if child.kind() == "parameter_list"
                || child.kind() == "value_parameters"
                || child.kind() == "("
            {
                break;
            }
            if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                if let Ok(text) = child.utf8_text(source) {
                    last_id = Some(text.to_string());
                }
            }
        }
        last_id.unwrap_or_else(|| "function".to_string())
    }

    fn property_kind(node: Node, source: &[u8]) -> SourceUnitKind {
        let is_const = node.children(&mut node.walk()).any(|child| {
            child.kind() == "const"
                || (child.kind() == "modifiers"
                    && child
                        .utf8_text(source)
                        .ok()
                        .is_some_and(|text| text.split_whitespace().any(|token| token == "const")))
        });
        if is_const {
            SourceUnitKind::Constant
        } else {
            SourceUnitKind::Property
        }
    }

    fn extract_property_name(node: Node, source: &[u8]) -> String {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "variable_declaration" {
                if let Some(id) = Self::extract_identifier(child, source) {
                    return id;
                }
            }
            if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                if let Ok(text) = child.utf8_text(source) {
                    return text.to_string();
                }
            }
        }
        "property".to_string()
    }
}

impl LanguageAnalyzer for KotlinAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext, "kt" | "kts"))
            .unwrap_or(false)
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())?;
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let root = tree.root_node();
        let bytes = source.as_bytes();

        let mut units = Vec::new();

        let mut process_node = |node: Node| {
            if !node.is_named() {
                return;
            }
            let kind_str = node.kind();
            if kind_str == "import"
                || kind_str == "package_header"
                || kind_str == "file_annotation"
                || kind_str == "shebang"
            {
                return;
            }

            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;

            match kind_str {
                "function_declaration" => {
                    let name = Self::extract_function_name(node, bytes);
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: SourceUnitKind::Function,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                "class_declaration" => {
                    let name = Self::extract_identifier(node, bytes)
                        .unwrap_or_else(|| "Class".to_string());
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: SourceUnitKind::Class,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                "object_declaration" => {
                    let name = Self::extract_identifier(node, bytes)
                        .unwrap_or_else(|| "Object".to_string());
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: SourceUnitKind::Class,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                "property_declaration" => {
                    let name = Self::extract_property_name(node, bytes);
                    units.push(SourceUnit {
                        name,
                        qualified_name: None,
                        kind: Self::property_kind(node, bytes),
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
                _ => {
                    for inner in node.children(&mut node.walk()) {
                        match inner.kind() {
                            "function_declaration" => {
                                let name = Self::extract_function_name(inner, bytes);
                                units.push(SourceUnit {
                                    name,
                                    qualified_name: None,
                                    kind: SourceUnitKind::Function,
                                    start_line: inner.start_position().row + 1,
                                    end_line: inner.end_position().row + 1,
                                    source: source_for_lines(
                                        source,
                                        inner.start_position().row + 1,
                                        inner.end_position().row + 1,
                                    ),
                                });
                                return;
                            }
                            "class_declaration" => {
                                let name = Self::extract_identifier(inner, bytes)
                                    .unwrap_or_else(|| "Class".to_string());
                                units.push(SourceUnit {
                                    name,
                                    qualified_name: None,
                                    kind: SourceUnitKind::Class,
                                    start_line: inner.start_position().row + 1,
                                    end_line: inner.end_position().row + 1,
                                    source: source_for_lines(
                                        source,
                                        inner.start_position().row + 1,
                                        inner.end_position().row + 1,
                                    ),
                                });
                                return;
                            }
                            "property_declaration" => {
                                let name = Self::extract_property_name(inner, bytes);
                                units.push(SourceUnit {
                                    name,
                                    qualified_name: None,
                                    kind: Self::property_kind(inner, bytes),
                                    start_line: inner.start_position().row + 1,
                                    end_line: inner.end_position().row + 1,
                                    source: source_for_lines(
                                        source,
                                        inner.start_position().row + 1,
                                        inner.end_position().row + 1,
                                    ),
                                });
                                return;
                            }
                            _ => {}
                        }
                    }

                    let first_line = source_for_lines(source, start_line, start_line);
                    let name = first_line.trim().to_string();
                    let name = if name.is_empty() {
                        "statement".to_string()
                    } else {
                        name
                    };

                    units.push(SourceUnit {
                        name,
                        qualified_name: Some(super::infra::statement_identity(node, bytes)),
                        kind: SourceUnitKind::TopLevelBlock,
                        start_line,
                        end_line,
                        source: source_for_lines(source, start_line, end_line),
                    });
                }
            }
        };

        for child in root.children(&mut root.walk()) {
            process_node(child);
        }

        Ok(smallest_units(units, ranges))
    }
}
