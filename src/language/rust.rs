use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct RustAnalyzer;
impl RustAnalyzer {
    fn enclosing_type(node: Node, source: &[u8]) -> Option<String> {
        let mut current = node.parent();
        while let Some(parent) = current {
            if matches!(parent.kind(), "impl_item" | "trait_item") {
                if let Some(name) = parent
                    .child_by_field_name("type")
                    .or_else(|| parent.child_by_field_name("name"))
                    .and_then(|name| name.utf8_text(source).ok())
                {
                    return Some(name.to_string());
                }
                for child in parent.children(&mut parent.walk()) {
                    if matches!(child.kind(), "type_identifier" | "scoped_type_identifier") {
                        return child.utf8_text(source).ok().map(str::to_string);
                    }
                }
            }
            current = parent.parent();
        }
        None
    }

    fn walk(
        node: Node,
        source: &[u8],
        out: &mut Vec<(usize, usize, String, SymbolKind, Option<String>)>,
    ) {
        if node.kind() == "function_item" {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("function")
                .to_string();
            let qualified_name =
                Self::enclosing_type(node, source).map(|owner| format!("{owner}::{name}"));
            let kind = if qualified_name.is_some() {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            out.push((
                node.start_position().row + 1,
                node.end_position().row + 1,
                name,
                kind,
                qualified_name,
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
    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Rust parse failed"))?;
        let mut found = vec![];
        Self::walk(tree.root_node(), source.as_bytes(), &mut found);
        let mut result = vec![];
        for (s, e, name, kind, qualified_name) in found {
            result.push(SourceSymbol {
                name,
                qualified_name,
                kind,
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
