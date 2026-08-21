pub mod c;
pub mod c_sharp;
pub mod cpp;
pub mod go;
pub mod java;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod typescript;
use crate::diff::LineRange;
use anyhow::Result;
use std::path::Path;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Constructor,
    Destructor,
    Class,
    Struct,
    Interface,
    Trait,
    Lambda,
    Module,
    Other,
}
#[derive(Clone, Debug)]
pub struct SourceSymbol {
    pub name: String,
    #[allow(dead_code)]
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

pub struct LanguageRegistry;

impl LanguageRegistry {
    pub fn analyzer_for_path(path: &Path) -> Option<&'static dyn LanguageAnalyzer> {
        static GO: go::GoAnalyzer = go::GoAnalyzer;
        static JAVA: java::JavaAnalyzer = java::JavaAnalyzer;
        static C_SHARP: c_sharp::CSharpAnalyzer = c_sharp::CSharpAnalyzer;
        static TYPESCRIPT: typescript::TypeScriptAnalyzer = typescript::TypeScriptAnalyzer;
        static JAVASCRIPT: javascript::JavaScriptAnalyzer = javascript::JavaScriptAnalyzer;
        static CPP: cpp::CppAnalyzer = cpp::CppAnalyzer;
        static C: c::CAnalyzer = c::CAnalyzer;
        static PYTHON: python::PythonAnalyzer = python::PythonAnalyzer;
        static RUST: rust::RustAnalyzer = rust::RustAnalyzer;

        [
            &RUST as &dyn LanguageAnalyzer,
            &PYTHON as &dyn LanguageAnalyzer,
            &GO as &dyn LanguageAnalyzer,
            &JAVA as &dyn LanguageAnalyzer,
            &C_SHARP as &dyn LanguageAnalyzer,
            &TYPESCRIPT as &dyn LanguageAnalyzer,
            &JAVASCRIPT as &dyn LanguageAnalyzer,
            &CPP as &dyn LanguageAnalyzer,
            &C as &dyn LanguageAnalyzer,
        ]
        .into_iter()
        .find(|analyzer| analyzer.supports_path(path))
    }

    pub fn language_for_path(path: &Path) -> Option<&'static str> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("rs") => Some("Rust"),
            Some("py") => Some("Python"),
            Some("go") => Some("Go"),
            Some("java") => Some("Java"),
            Some("cs") => Some("C#"),
            Some("ts" | "tsx") => Some("TypeScript"),
            Some("js" | "jsx") => Some("JavaScript"),
            Some("cpp" | "cc" | "cxx" | "hpp") => Some("C++"),
            Some("c") => Some("C"),
            _ => None,
        }
    }
}

pub fn overlaps(start: usize, end: usize, ranges: &[LineRange]) -> bool {
    ranges
        .iter()
        .any(|range| range.start <= end && range.end >= start)
}

pub fn source_for_lines(source: &str, start: usize, end: usize) -> String {
    let lines: Vec<_> = source.lines().collect();
    lines
        .get(start.saturating_sub(1)..end.min(lines.len()))
        .unwrap_or(&[])
        .join("\n")
}

pub fn smallest_symbols(symbols: Vec<SourceSymbol>, ranges: &[LineRange]) -> Vec<SourceSymbol> {
    let mut symbols: Vec<_> = symbols
        .into_iter()
        .filter(|symbol| overlaps(symbol.start_line, symbol.end_line, ranges))
        .collect();
    let all_symbols = symbols.clone();
    symbols.retain(|symbol| {
        !all_symbols.iter().any(|other| {
            other.start_line >= symbol.start_line
                && other.end_line <= symbol.end_line
                && (other.start_line > symbol.start_line || other.end_line < symbol.end_line)
        })
    });
    symbols.sort_by_key(|symbol| (symbol.start_line, symbol.end_line));
    symbols.dedup_by(|a, b| {
        a.name == b.name && a.start_line == b.start_line && a.end_line == b.end_line
    });
    symbols
}
