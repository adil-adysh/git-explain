use crate::{
    diff::{ChangeKind, FileChange},
    language::{LanguageRegistry, SourceSymbol, SourceUnit},
    model::{ExplanationProvider, ExplanationRegion, ExplanationRequest, FunctionExplanation},
};
use anyhow::Context;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub trait SourceProvider {
    fn read_file(&self, path: &Path) -> Result<String>;
}

pub struct WorkingTreeSourceProvider {
    root: PathBuf,
}

impl WorkingTreeSourceProvider {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into() }
    }
}

impl SourceProvider for WorkingTreeSourceProvider {
    fn read_file(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(self.root.join(path))
            .with_context(|| format!("read working-tree source {}", path.display()))
    }
}

pub struct GitCommitSourceProvider {
    root: PathBuf,
    oid: String,
}

impl GitCommitSourceProvider {
    pub fn new(root: &Path, oid: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            oid: oid.into(),
        }
    }
}

impl SourceProvider for GitCommitSourceProvider {
    fn read_file(&self, path: &Path) -> Result<String> {
        let git_path = path.to_string_lossy().replace('\\', "/");
        let spec = format!("{}:{}", self.oid, git_path);
        let output = std::process::Command::new("git")
            .current_dir(&self.root)
            .args(["show", &spec])
            .output()
            .with_context(|| format!("read committed source {}", path.display()))?;
        if !output.status.success() {
            anyhow::bail!(
                "git show {spec} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8(output.stdout)
            .with_context(|| format!("committed file {} is not text", path.display()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisMode {
    WorkingTree,
    Commit {
        oid: String,
        parent_oid: Option<String>,
        subject: String,
        merge_parent_count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisContext {
    pub mode: AnalysisMode,
    pub deleted_files: Vec<String>,
}

impl AnalysisContext {
    pub fn working_tree() -> Self {
        Self {
            mode: AnalysisMode::WorkingTree,
            deleted_files: vec![],
        }
    }

    pub fn prompt_context(&self) -> String {
        match &self.mode {
            AnalysisMode::WorkingTree => "Change source: working tree".into(),
            AnalysisMode::Commit { oid, subject, .. } => {
                format!("Change source: commit {oid}\nCommit subject: {subject}")
            }
        }
    }
}
#[derive(Clone)]
pub struct ExplainedUnit {
    pub file: String,
    pub language: String,
    pub diff: String,
    pub regions: Vec<ExplanationRegion>,
    pub unit: SourceUnit,
    pub explanation: FunctionExplanation,
}
pub type ExplainedFunction = ExplainedUnit;
#[allow(dead_code)]
pub fn symbols(root: &Path, c: &FileChange) -> Result<Vec<SourceSymbol>> {
    let provider = WorkingTreeSourceProvider::new(root);
    symbols_from_provider(&provider, c)
}

pub fn symbols_from_provider(
    provider: &dyn SourceProvider,
    c: &FileChange,
) -> Result<Vec<SourceSymbol>> {
    if c.kind == ChangeKind::Deleted {
        return Ok(vec![]);
    }
    if LanguageRegistry::analyzer_for_path(&c.path).is_none() {
        return Ok(vec![]);
    }
    let source = match provider.read_file(&c.path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: unable to read changed file: {error}", c.path.display());
            return Ok(vec![]);
        }
    };
    match LanguageRegistry::find_changed_units(&c.path, &source, &c.ranges) {
        Ok(symbols) => Ok(symbols),
        Err(error) => {
            eprintln!("{}: language analysis failed: {error}", c.path.display());
            Ok(vec![])
        }
    }
}
pub fn print_debug(
    provider: &dyn SourceProvider,
    changes: &[FileChange],
    context: &AnalysisContext,
) -> Result<()> {
    match &context.mode {
        AnalysisMode::WorkingTree => println!("Mode: working tree\n"),
        AnalysisMode::Commit {
            oid,
            parent_oid,
            subject,
            merge_parent_count,
        } => {
            println!(
                "Mode: commit\n\nCommit: {}\nParent: {}\nSubject: {}",
                oid,
                parent_oid.as_deref().unwrap_or("<empty tree>"),
                subject
            );
            if *merge_parent_count > 1 {
                println!("Merge commit detected. Showing changes relative to first parent.");
            }
            println!();
        }
    }
    for c in changes {
        if c.kind == ChangeKind::Renamed {
            if let Some(old_path) = &c.old_path {
                println!(
                    "Renamed file: {} -> {}",
                    old_path.display(),
                    c.path.display()
                );
            }
        }
        if c.kind == ChangeKind::Deleted {
            println!("Deleted file: {}\nDetailed annotated source explanation is not currently supported.\n", c.path.display());
            continue;
        }
        for s in symbols_from_provider(provider, c)? {
            println!(
                "{}\n\nChanged unit:\nkind: {:?}\nname: {}\nlines {}-{}\n\n{}",
                c.path.display(),
                s.kind,
                s.name,
                s.start_line,
                s.end_line,
                s.source
            );
            println!("Regions:");
            for region in regions_for_change(c, &s) {
                println!(
                    "\nRegion {}\nlines {}-{}\n{}",
                    region.id, region.start_line, region.end_line, region.source
                );
            }
        }
    }
    Ok(())
}

fn relevant_diff(change: &FileChange, symbol: &SourceSymbol) -> String {
    let mut result = String::new();
    let mut range_index = 0;
    let mut selected = false;
    for line in change.diff.lines() {
        if line.starts_with("@@ ") {
            selected = change.ranges.get(range_index).is_some_and(|range| {
                range.start <= symbol.end_line && range.end >= symbol.start_line
            });
            range_index += 1;
            if selected {
                result.push_str(line);
                result.push('\n');
            }
        } else if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            result.push_str(line);
            result.push('\n');
        } else if selected {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

pub fn regions_for(change: &FileChange, symbol: &SourceSymbol) -> Vec<ExplanationRegion> {
    change
        .ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.max(symbol.start_line);
            let end = range.end.min(symbol.end_line);
            (start <= end).then_some((start, end))
        })
        .enumerate()
        .map(|(index, (start, end))| {
            let source_lines: Vec<_> = symbol.source.lines().collect();
            let relative_start = start - symbol.start_line + 1;
            let relative_end = end - symbol.start_line + 1;
            ExplanationRegion {
                id: index + 1,
                start_line: relative_start,
                end_line: relative_end,
                source: source_lines
                    .get(relative_start.saturating_sub(1)..relative_end.min(source_lines.len()))
                    .unwrap_or(&[])
                    .join("\n"),
            }
        })
        .collect()
}

fn regions_for_change(change: &FileChange, symbol: &SourceSymbol) -> Vec<ExplanationRegion> {
    let regions = regions_for(change, symbol);
    if regions.is_empty() {
        vec![ExplanationRegion {
            id: 1,
            start_line: 1,
            end_line: symbol.source.lines().count().max(1),
            source: symbol.source.clone(),
        }]
    } else {
        regions
    }
}
pub async fn explain_items(
    source_provider: &dyn SourceProvider,
    changes: &[FileChange],
    model_provider: impl ExplanationProvider,
    context: &AnalysisContext,
    deep: bool,
) -> Result<Vec<ExplainedFunction>> {
    let mut all = vec![];
    for c in changes {
        for s in symbols_from_provider(source_provider, c)? {
            let regions = regions_for_change(c, &s);
            let diff = relevant_diff(c, &s);
            let e = match model_provider
                .explain(ExplanationRequest {
                    function: s.source.clone(),
                    unit_name: s.name.clone(),
                    unit_kind: format!("{:?}", s.kind),
                    diff: diff.clone(),
                    language: LanguageRegistry::language_for_path(&c.path)
                        .unwrap_or("unknown")
                        .into(),
                    git_context: context.prompt_context(),
                    regions: regions.clone(),
                    prior_explanation: None,
                    deep,
                })
                .await
            {
                Ok(explanation) => explanation,
                Err(error) => {
                    eprintln!(
                        "{} / {}: explanation request failed: {error:#}",
                        c.path.display(),
                        s.name
                    );
                    FunctionExplanation {
                        overview: "Explanation unavailable.".into(),
                        annotations: vec![],
                        deep: None,
                    }
                }
            };
            all.push(ExplainedFunction {
                file: c.path.display().to_string(),
                language: LanguageRegistry::language_for_path(&c.path)
                    .unwrap_or("unknown")
                    .into(),
                diff,
                regions,
                unit: s,
                explanation: e,
            });
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diff::LineRange, language::SymbolKind};
    use std::path::PathBuf;

    #[test]
    fn selects_only_the_hunk_for_the_changed_function() {
        let change = FileChange {
            path: PathBuf::from("src/users.rs"),
            old_path: None,
            kind: crate::diff::ChangeKind::Modified,
            ranges: vec![
                LineRange { start: 2, end: 2 },
                LineRange { start: 8, end: 8 },
            ],
            diff: "diff --git a/src/users.rs b/src/users.rs\n+++ b/src/users.rs\n@@ -2 +2 @@\n-old\n+new one\n@@ -8 +8 @@\n-old\n+new two\n".into(),
        };
        let symbol = SourceSymbol {
            name: "first".into(),
            qualified_name: None,
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 3,
            source: "fn first() {}".into(),
        };
        let diff = relevant_diff(&change, &symbol);
        assert!(diff.contains("new one"));
        assert!(!diff.contains("new two"));
    }

    #[test]
    fn regions_are_relative_to_the_selected_function() {
        let change = FileChange {
            path: PathBuf::from("src/users.rs"),
            old_path: None,
            kind: crate::diff::ChangeKind::Modified,
            ranges: vec![LineRange { start: 4, end: 5 }],
            diff: String::new(),
        };
        let symbol = SourceSymbol {
            name: "first".into(),
            qualified_name: None,
            kind: SymbolKind::Function,
            start_line: 3,
            end_line: 7,
            source: "fn first() {\n    let a = 1;\n    changed();\n    finish();\n}".into(),
        };

        let regions = regions_for(&change, &symbol);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].id, 1);
        assert_eq!((regions[0].start_line, regions[0].end_line), (2, 3));
        assert_eq!(regions[0].source, "    let a = 1;\n    changed();");
    }
}
