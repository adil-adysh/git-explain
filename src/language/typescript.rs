use super::{javascript::find_with_language, LanguageAnalyzer, SourceSymbol};
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;

pub struct TypeScriptAnalyzer;

impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("ts" | "tsx")))
    }

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceSymbol>> {
        let language = if source.contains("<") && source.contains(">") {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        find_with_language(source, ranges, language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_typed_export_and_arrow_function() {
        let source = "export const loadUser = async (id: string): Promise<string> => {\n    return id;\n};\n";
        let symbols = TypeScriptAnalyzer
            .find_containing_symbols(source, &[LineRange { start: 2, end: 2 }])
            .unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "loadUser");
    }
}
