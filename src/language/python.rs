use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct PythonAnalyzer;

impl PythonAnalyzer {
    fn walk(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
        if node.kind() == "function_definition" {
            let decorated = node
                .parent()
                .filter(|parent| parent.kind() == "decorated_definition");
            let range_node = decorated.unwrap_or(node);
            let name = node
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .unwrap_or("function");
            let mut classes = Vec::new();
            let mut ancestor = node.parent();
            while let Some(current) = ancestor {
                if current.kind() == "class_definition" {
                    if let Some(class_name) = current
                        .child_by_field_name("name")
                        .and_then(|class_name| class_name.utf8_text(source).ok())
                    {
                        classes.push(class_name.to_string());
                    }
                }
                ancestor = current.parent();
            }
            classes.reverse();
            let qualified_name = if classes.is_empty() {
                name.to_string()
            } else {
                format!("{}.{}", classes.join("."), name)
            };
            let start = range_node.start_position().row + 1;
            let end = range_node.end_position().row + 1;
            out.push(SourceSymbol {
                name: qualified_name,
                kind: if classes.is_empty() {
                    SymbolKind::Function
                } else {
                    SymbolKind::Method
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
        for child in node.children(&mut node.walk()) {
            Self::walk(child, source, out);
        }
    }
}

impl LanguageAnalyzer for PythonAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "py")
    }

    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Python parse failed"))?;
        let mut symbols = Vec::new();
        Self::walk(tree.root_node(), source.as_bytes(), &mut symbols);
        Ok(smallest_symbols(symbols, ranges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_async_decorated_method_with_qualified_name() {
        let source = "class UserService:\n    @router.get(\"/users\")\n    async def authenticate(self, user):\n        return user.id\n";
        let symbols = PythonAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 4, end: 4 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "UserService.authenticate");
        assert!(symbols[0].source.contains("@router.get"));
    }

    #[test]
    fn chooses_nested_function_when_change_is_inside_it() {
        let source = "def outer():\n    def inner():\n        return 1\n    return inner()\n";
        let symbols = PythonAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 3, end: 3 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "inner");
    }

    #[test]
    fn ignores_changes_outside_functions() {
        let source = "VALUE = 1\n\ndef work():\n    return VALUE\n";
        let symbols = PythonAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 1, end: 1 }])
            .unwrap();
        assert!(symbols.is_empty());
    }

    #[test]
    fn returns_multiple_changed_functions_without_duplicates() {
        let source = "def first():\n    return 1\n\ndef second():\n    return 2\n";
        let symbols = PythonAnalyzer
            .find_containing_symbols(
                source,
                &[
                    LineRange { start: 2, end: 2 },
                    LineRange { start: 2, end: 2 },
                    LineRange { start: 5, end: 5 },
                ],
            )
            .unwrap();
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }
}
