use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct CppAnalyzer;

fn declarator_name(node: Node, source: &[u8]) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "qualified_identifier"
    ) {
        return node.utf8_text(source).ok().map(str::to_string);
    }
    if let Some(declarator) = node.child_by_field_name("declarator") {
        if let Some(name) = declarator_name(declarator, source) {
            return Some(name);
        }
    }
    for child in node.children(&mut node.walk()) {
        if let Some(name) = declarator_name(child, source) {
            return Some(name);
        }
    }
    None
}

fn enclosing_scopes(node: Node, source: &[u8]) -> Vec<String> {
    let mut scopes = Vec::new();
    let mut current = node.parent();
    while let Some(parent) = current {
        if matches!(
            parent.kind(),
            "class_specifier" | "struct_specifier" | "namespace_definition"
        ) {
            if let Some(name) = parent
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
            {
                scopes.push(name.to_string());
            }
        }
        current = parent.parent();
    }
    scopes.reverse();
    scopes
}

impl LanguageAnalyzer for CppAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("cpp" | "cc" | "cxx" | "hpp"))
        })
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_cpp::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("C++ parse failed"))?;
        let mut symbols = Vec::new();
        collect(tree.root_node(), source.as_bytes(), &mut symbols);
        Ok(smallest_symbols(symbols, ranges))
    }
}

fn collect(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
    if node.kind() == "function_definition" {
        if let Some(declarator_node) = node.child_by_field_name("declarator") {
            let raw_declarator = declarator_node.utf8_text(source).unwrap_or_default();
            let Some(mut name) = declarator_name(declarator_node, source) else {
                return collect_children(node, source, out);
            };
            let is_destructor = raw_declarator.contains('~');
            if is_destructor && !name.starts_with('~') {
                name = format!("~{name}");
            }
            let scopes = enclosing_scopes(node, source);
            if !scopes.is_empty() && !name.contains("::") {
                name = format!("{}::{}", scopes.join("::"), name);
            }
            let qualified_name = name.contains("::").then(|| name.clone());
            let is_constructor = !is_destructor
                && scopes
                    .last()
                    .is_some_and(|scope| name == *scope || name.ends_with(&format!("::{scope}")));
            let range_node = node
                .parent()
                .filter(|parent| parent.kind() == "template_declaration")
                .unwrap_or(node);
            let start = range_node.start_position().row + 1;
            let end = range_node.end_position().row + 1;
            out.push(SourceSymbol {
                name,
                qualified_name,
                kind: if is_destructor {
                    SymbolKind::Destructor
                } else if is_constructor {
                    SymbolKind::Constructor
                } else if !scopes.is_empty() {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                },
                start_line: start,
                end_line: end,
                source: source_for_lines(
                    std::str::from_utf8(source).unwrap_or_default(),
                    start,
                    end,
                ),
            });
        }
    }
    collect_children(node, source, out);
}

fn collect_children(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
    for child in node.children(&mut node.walk()) {
        collect(child, source, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_namespaced_cpp_method() {
        let source = "namespace auth {\nclass Service {\npublic:\n    bool authenticate() {\n        return true;\n    }\n};\n}\n";
        let symbols = CppAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 5, end: 5 }])
            .unwrap();
        assert_eq!(symbols[0].name, "auth::Service::authenticate");
    }

    #[test]
    fn finds_destructor() {
        let source = "class Service {\npublic:\n    ~Service() {\n        close();\n    }\n};\n";
        let symbols = CppAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 4, end: 4 }])
            .unwrap();
        assert_eq!(symbols[0].name, "Service::~Service");
        assert_eq!(symbols[0].kind, SymbolKind::Destructor);
    }

    #[test]
    fn finds_constructor() {
        let source = "class Service {\npublic:\n    Service() {\n        open();\n    }\n};\n";
        let symbols = CppAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 4, end: 4 }])
            .unwrap();
        assert_eq!(symbols[0].name, "Service::Service");
        assert_eq!(symbols[0].kind, SymbolKind::Constructor);
    }
}
