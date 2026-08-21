pub mod rust;
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
}
#[derive(Clone, Debug)]
pub struct SourceSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub source: String,
}
pub trait LanguageAnalyzer {
    fn supports_path(&self, path: &Path) -> bool;
    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>>;
}
