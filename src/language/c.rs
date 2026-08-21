use super::{smallest_symbols, source_for_lines, LanguageAnalyzer, SourceSymbol, SymbolKind};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct CAnalyzer;

fn declarator_name(node: Node, source: &[u8]) -> Option<String> {
    if matches!(node.kind(), "identifier" | "field_identifier") {
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

impl LanguageAnalyzer for CAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "c")
    }

    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into())?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("C parse failed"))?;
        let mut symbols = Vec::new();
        collect(tree.root_node(), source.as_bytes(), &mut symbols);
        Ok(smallest_symbols(symbols, ranges))
    }
}

fn collect(node: Node, source: &[u8], out: &mut Vec<SourceSymbol>) {
    if node.kind() == "function_definition" {
        if let Some(name) = node
            .child_by_field_name("declarator")
            .and_then(|declarator| declarator_name(declarator, source))
        {
            let start = node.start_position().row + 1;
            let end = node.end_position().row + 1;
            out.push(SourceSymbol {
                name,
                qualified_name: None,
                kind: SymbolKind::Function,
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
    for child in node.children(&mut node.walk()) {
        collect(child, source, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_c_function() {
        let source = "int calculate_total(int a) {\n    return a + 1;\n}\n";
        let symbols = CAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 2, end: 2 }])
            .unwrap();
        assert_eq!(symbols[0].name, "calculate_total");
    }
}
