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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceUnitKind {
    Function,
    Method,
    Constructor,
    Destructor,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Impl,
    Property,
    Constant,
    TypeAlias,
    ImportBlock,
    TopLevelBlock,
    Lambda,
    Module,
    Other,
}
#[derive(Clone, Debug)]
pub struct SourceUnit {
    pub name: String,
    pub qualified_name: Option<String>,
    pub kind: SourceUnitKind,
    pub start_line: usize,
    pub end_line: usize,
    pub source: String,
}
pub type SourceSymbol = SourceUnit;
pub type SymbolKind = SourceUnitKind;
pub trait LanguageAnalyzer {
    fn supports_path(&self, path: &Path) -> bool;
    fn find_containing_symbols(
        &self,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceSymbol>>;

    fn find_changed_units(&self, source: &str, ranges: &[LineRange]) -> Result<Vec<SourceUnit>> {
        self.find_containing_symbols(source, ranges)
    }
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

    pub fn find_changed_units(
        path: &Path,
        source: &str,
        ranges: &[LineRange],
    ) -> Result<Vec<SourceUnit>> {
        let Some(analyzer) = Self::analyzer_for_path(path) else {
            return Ok(vec![]);
        };
        let mut units = analyzer.find_changed_units(source, ranges)?;
        let covered = units.clone();
        units.extend(fallback_units(path, source, ranges, &covered));
        Ok(smallest_units(units, ranges))
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
    smallest_units(symbols, ranges)
}

pub fn smallest_units(units: Vec<SourceUnit>, ranges: &[LineRange]) -> Vec<SourceUnit> {
    let mut symbols: Vec<_> = units
        .into_iter()
        .filter(|symbol| overlaps(symbol.start_line, symbol.end_line, ranges))
        .collect();
    let all_symbols = symbols.clone();
    symbols.retain(|symbol| {
        !all_symbols.iter().any(|other| {
            other.start_line >= symbol.start_line
                && other.end_line <= symbol.end_line
                && (other.start_line > symbol.start_line || other.end_line < symbol.end_line)
                && ranges
                    .iter()
                    .filter(|range| {
                        overlaps(
                            symbol.start_line,
                            symbol.end_line,
                            std::slice::from_ref(range),
                        )
                    })
                    .all(|range| range.start >= other.start_line && range.end <= other.end_line)
        })
    });
    symbols.sort_by_key(|symbol| (symbol.start_line, symbol.end_line));
    symbols.dedup_by(|a, b| {
        a.name == b.name
            && a.kind == b.kind
            && a.start_line == b.start_line
            && a.end_line == b.end_line
    });
    symbols
}

fn fallback_units(
    path: &Path,
    source: &str,
    ranges: &[LineRange],
    existing: &[SourceUnit],
) -> Vec<SourceUnit> {
    let lines: Vec<_> = source.lines().collect();
    let mut result: Vec<SourceUnit> = vec![];
    for range in ranges {
        if existing
            .iter()
            .any(|unit| overlaps(unit.start_line, unit.end_line, std::slice::from_ref(range)))
        {
            continue;
        }
        let current_line = lines
            .get(range.start.saturating_sub(1))
            .copied()
            .unwrap_or("")
            .trim();
        let current_kind = classify_declaration(path, current_line).0;
        let leaf_change = matches!(
            current_kind,
            SourceUnitKind::Constant | SourceUnitKind::ImportBlock | SourceUnitKind::TypeAlias
        );
        let (anchor, line) = if leaf_change {
            (range.start, current_line)
        } else {
            enclosing_declaration(&lines, range.start)
                .map(|(line_number, text)| (line_number, text.trim()))
                .unwrap_or((range.start, current_line))
        };
        let (kind, name) = classify_declaration(path, line);
        let (start, end) = declaration_bounds(&lines, anchor, range.end.max(anchor));
        let unit = SourceUnit {
            name,
            qualified_name: None,
            kind,
            start_line: start,
            end_line: end,
            source: source_for_lines(source, start, end),
        };
        if let Some(previous) = result.last_mut() {
            if previous.kind == SourceUnitKind::ImportBlock
                && unit.kind == SourceUnitKind::ImportBlock
                && unit.start_line <= previous.end_line + 1
            {
                previous.end_line = previous.end_line.max(unit.end_line);
                previous.source = source_for_lines(source, previous.start_line, previous.end_line);
                continue;
            }
        }
        result.push(unit);
    }
    result
}

fn enclosing_declaration<'a>(lines: &[&'a str], line: usize) -> Option<(usize, &'a str)> {
    for index in (0..line.min(lines.len())).rev() {
        let text = lines[index].trim_start();
        if text.contains('{')
            && [
                "struct ",
                "enum ",
                "trait ",
                "impl ",
                "class ",
                "interface ",
                "type ",
            ]
            .iter()
            .any(|prefix| text.starts_with(prefix) || text.contains(&format!(" {prefix}")))
        {
            let mut depth = 0isize;
            for nested in lines.iter().skip(index).take(line.saturating_sub(index)) {
                depth += nested.chars().filter(|c| *c == '{').count() as isize;
                depth -= nested.chars().filter(|c| *c == '}').count() as isize;
            }
            if depth > 0 {
                return Some((index + 1, lines[index]));
            }
        }
    }
    None
}

fn classify_declaration(path: &Path, line: &str) -> (SourceUnitKind, String) {
    let trimmed = line
        .trim_start_matches(['@', '#'])
        .trim_start_matches("pub ")
        .trim_start_matches("export ");
    let language = path.extension().and_then(|x| x.to_str()).unwrap_or("");
    let kind = if trimmed.starts_with("use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || line.starts_with("#include")
        || trimmed.starts_with("using ")
    {
        SourceUnitKind::ImportBlock
    } else if trimmed.starts_with("struct ")
        || trimmed.starts_with("type ") && line.contains("struct")
    {
        SourceUnitKind::Struct
    } else if trimmed.starts_with("enum ") || trimmed.contains(" enum ") {
        SourceUnitKind::Enum
    } else if trimmed.starts_with("trait ") {
        SourceUnitKind::Trait
    } else if trimmed.starts_with("interface ") || line.contains(" interface ") {
        SourceUnitKind::Interface
    } else if trimmed.starts_with("impl ") {
        SourceUnitKind::Impl
    } else if trimmed.starts_with("class ") || line.contains(" class ") {
        SourceUnitKind::Class
    } else if trimmed.starts_with("type ")
        || (language == "ts" && trimmed.starts_with("export type"))
        || trimmed.starts_with("typedef ")
    {
        SourceUnitKind::TypeAlias
    } else if trimmed.starts_with("const ")
        || trimmed.starts_with("static ")
        || trimmed.starts_with("var ")
        || trimmed.contains(" = ")
    {
        SourceUnitKind::Constant
    } else {
        SourceUnitKind::TopLevelBlock
    };
    let tokens: Vec<_> = trimmed.split_whitespace().collect();
    let name = if kind == SourceUnitKind::ImportBlock {
        "Imports".into()
    } else {
        let keyword = [
            "struct",
            "enum",
            "trait",
            "impl",
            "class",
            "interface",
            "type",
            "typedef",
            "const",
            "static",
            "var",
        ]
        .iter()
        .find_map(|keyword| tokens.iter().position(|token| token == keyword));
        keyword
            .and_then(|index| tokens.get(index + 1).copied())
            .unwrap_or_else(|| tokens.first().copied().unwrap_or("changed code"))
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
            .to_string()
    };
    (
        kind,
        if name.is_empty() {
            "changed code".into()
        } else {
            name
        },
    )
}

fn declaration_bounds(lines: &[&str], start: usize, end: usize) -> (usize, usize) {
    let requested_end = end.max(start).min(lines.len().max(1));
    let mut finish = start.min(lines.len().max(1));
    let mut depth = 0isize;
    let mut saw_brace = false;
    for line in lines.iter().skip(start.saturating_sub(1)) {
        depth += line.chars().filter(|c| *c == '{').count() as isize;
        depth -= line.chars().filter(|c| *c == '}').count() as isize;
        if line.contains('{') {
            saw_brace = true;
        }
        if saw_brace && depth <= 0 {
            return (start, finish.min(lines.len().max(start)));
        }
        finish += 1;
        if !saw_brace && finish >= requested_end {
            return (start, requested_end);
        }
    }
    (start, finish.min(lines.len().max(start)))
}
