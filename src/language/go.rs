use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct GoAnalyzer;

impl GoAnalyzer {
    fn receiver_type(node: Node, source: &[u8]) -> Option<String> {
        let text = node
            .utf8_text(source)
            .ok()?
            .trim_matches(|c| c == '(' || c == ')');
        let token = text
            .split('[')
            .next()
            .unwrap_or(text)
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|token| !token.is_empty())
            .last()?;
        Some(token.to_string())
    }

    fn walk(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
        let (kind, name) = match node.kind() {
            "function_declaration" => (
                SymbolKind::Function,
                node.child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    .unwrap_or("function")
                    .to_string(),
            ),
            "method_declaration" => {
                let method = node
                    .child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    .unwrap_or("method");
                let receiver = node
                    .child_by_field_name("receiver")
                    .and_then(|receiver| Self::receiver_type(receiver, source));
                (
                    SymbolKind::Method,
                    receiver
                        .map(|r| format!("{}.{}", r, method))
                        .unwrap_or_else(|| method.to_string()),
                )
            }
            _ => return Self::walk_children(node, source, out),
        };
        let start = node.start_position().row + 1;
        let end = node.end_position().row + 1;
        out.push(SourceSymbol {
            name,
            kind,
            start_line: start,
            end_line: end,
            source: source_for_lines(std::str::from_utf8(source).unwrap_or_default(), start, end),
        });
        Self::walk_children(node, source, out);
    }

    fn walk_children(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
        for child in node.children(&mut node.walk()) {
            Self::walk(child, source, out);
        }
    }
}

impl LanguageAnalyzer for GoAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "go")
    }

    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_go::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Go parse failed"))?;
        let mut symbols = Vec::new();
        Self::walk(tree.root_node(), source.as_bytes(), &mut symbols);
        Ok(smallest_symbols(symbols, ranges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_pointer_and_value_receiver_methods() {
        let source = "type Service struct{}\n\nfunc (s *Service) Authenticate() {\n    load()\n}\n\nfunc (s Service) Name() string {\n    return \"service\"\n}\n";
        let symbols = GoAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 8, end: 8 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Service.Name");
    }

    #[test]
    fn maps_nested_closure_to_containing_method() {
        let source = "type Service struct{}\n\nfunc (s *Service) Run() {\n    values := []int{1}\n    _ = func() int { return values[0] }\n}\n";
        let symbols = GoAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 5, end: 5 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Service.Run");
    }

    #[test]
    fn returns_multiple_methods_and_deduplicates_multiple_hunks() {
        let source = "type Service struct{}\n\nfunc (s *Service) First() {\n    one()\n    two()\n}\n\nfunc (s *Service) Second() {\n    three()\n}\n";
        let symbols = GoAnalyzer
            .find_containing_symbols(
                source,
                &[
                    LineRange { start: 4, end: 4 },
                    LineRange { start: 5, end: 5 },
                    LineRange { start: 9, end: 9 },
                ],
            )
            .unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Service.First");
        assert_eq!(symbols[1].name, "Service.Second");
    }
}
