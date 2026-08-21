use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct RustAnalyzer;
impl RustAnalyzer {
    fn walk(node: Node, source: &[u8], out: &mut Vec<(usize, usize, String)>) {
        if node.kind() == "function_item" {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("function")
                .to_string();
            out.push((
                node.start_position().row + 1,
                node.end_position().row + 1,
                name,
            ));
        }
        for c in node.children(&mut node.walk()) {
            Self::walk(c, source, out);
        }
    }
}
impl LanguageAnalyzer for RustAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|x| x == "rs")
    }
    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Rust parse failed"))?;
        let mut found = vec![];
        Self::walk(tree.root_node(), source.as_bytes(), &mut found);
        let mut result = vec![];
        for (s, e, name) in found {
            result.push(SourceSymbol {
                name,
                kind: SymbolKind::Function,
                start_line: s,
                end_line: e,
                source: source_for_lines(source, s, e),
            });
        }
        Ok(smallest_symbols(result, ranges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_changed_function() {
        let s = "fn one() {}\nfn two() {\n let x=1;\n}\n";
        let x = RustAnalyzer
            .find_containing_symbols(s, &[LineRange { start: 3, end: 3 }])
            .unwrap();
        assert_eq!(x.len(), 1);
        assert_eq!(x[0].name, "two");
    }
}
