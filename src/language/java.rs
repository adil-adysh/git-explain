use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct JavaAnalyzer;

impl JavaAnalyzer {
    fn enclosing_types(node: Node, source: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        let mut current = node.parent();
        while let Some(parent) = current {
            if matches!(
                parent.kind(),
                "class_declaration" | "interface_declaration" | "enum_declaration"
            ) {
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

    fn walk(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
        let kind = match node.kind() {
            "method_declaration" => SymbolKind::Method,
            "constructor_declaration" => SymbolKind::Constructor,
            _ => {
                for child in node.children(&mut node.walk()) {
                    Self::walk(child, source, out);
                }
                return;
            }
        };
        let name = node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .unwrap_or("member");
        let types = Self::enclosing_types(node, source);
        let qualified_name = if types.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", types.join("."), name)
        };
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        out.push(SourceSymbol {
            name: qualified_name,
            kind,
            start_line: start,
            end_line: end,
            source: source_for_lines(std::str::from_utf8(source).unwrap_or_default(), start, end),
        });
        for child in node.children(&mut node.walk()) {
            Self::walk(child, source, out);
        }
    }
}

impl LanguageAnalyzer for JavaAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension == "java")
    }

    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_java::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Java parse failed"))?;
        let mut symbols = Vec::new();
        Self::walk(tree.root_node(), source.as_bytes(), &mut symbols);
        Ok(smallest_symbols(symbols, ranges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_method_and_constructor_with_qualified_names() {
        let source = "class UserService {\n    UserService() {\n        setup();\n    }\n    static User authenticate() {\n        return null;\n    }\n}\n";
        let constructor = JavaAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 3, end: 3 }])
            .unwrap();
        assert_eq!(constructor[0].name, "UserService.UserService");
        assert_eq!(constructor[0].kind, SymbolKind::Constructor);
        let method = JavaAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 6, end: 6 }])
            .unwrap();
        assert_eq!(method[0].name, "UserService.authenticate");
    }
}
