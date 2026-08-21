use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

pub struct JavaScriptAnalyzer;

pub(crate) fn find_with_language(
    source: &str,
    ranges: &[LineRange],
    language: Language,
) -> Result<Vec<SourceSymbol>> {
    let mut parser = Parser::new();
    parser.set_language(&language)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("JavaScript-family parse failed"))?;
    let mut symbols = Vec::new();
    walk(tree.root_node(), source.as_bytes(), &mut symbols);
    Ok(smallest_symbols(symbols, ranges))
}

fn enclosing_classes(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(parent.kind(), "class_declaration" | "class") {
            if let Some(name) = parent
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
            {
                names.push(name.to_string());
            }
        }
        current = parent.parent();
    }
    names.reverse();
    names
}

fn enclosing_export(node: Node) -> Node {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.kind() == "export_statement" {
            return candidate;
        }
        current = candidate.parent();
    }
    node
}

fn walk(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
    let declaration = match node.kind() {
        "function_declaration" => node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .map(|name| (name.to_string(), SymbolKind::Function, node)),
        "method_definition" => node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .map(|name| (name.to_string(), SymbolKind::Method, node)),
        "arrow_function" => node.parent().and_then(|parent| {
            if parent.kind() != "variable_declarator" {
                return None;
            }
            let name = parent
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())?;
            Some((name.to_string(), SymbolKind::Function, parent))
        }),
        _ => None,
    };
    if let Some((name, kind, range_node)) = declaration {
        let classes = enclosing_classes(node, source);
        let qualified_name = if classes.is_empty() {
            name
        } else {
            format!("{}.{}", classes.join("."), name)
        };
        let range_node = enclosing_export(range_node);
        let start = range_node.start_position().row + 1;
        let end = range_node.end_position().row + 1;
        out.push(SourceSymbol {
            name: qualified_name.clone(),
            qualified_name: Some(qualified_name.clone()),
            kind,
            start_line: start,
            end_line: end,
            source: source_for_lines(std::str::from_utf8(source).unwrap_or_default(), start, end),
        });
    }
    for child in node.children(&mut node.walk()) {
        walk(child, source, out);
    }
}

impl LanguageAnalyzer for JavaScriptAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("js" | "jsx")))
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceSymbol>> {
        find_with_language(source, ranges, tree_sitter_javascript::LANGUAGE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_functions_methods_and_named_arrow_functions() {
        let source = "async function load() {\n    return 1;\n}\nconst save = async (id) => {\n    return id;\n};\nclass Service {\n    authenticate() {\n        return true;\n    }\n}\n";
        let symbols = JavaScriptAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 5, end: 5 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "save");
        let method = JavaScriptAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 8, end: 8 }])
            .unwrap();
        assert_eq!(method[0].name, "Service.authenticate");
    }
}
