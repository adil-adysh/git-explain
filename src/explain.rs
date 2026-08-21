use crate::{
    diff::FileChange,
    language::{LanguageRegistry, SourceSymbol},
    model::{ExplanationProvider, ExplanationRequest, FunctionExplanation},
};
use anyhow::Result;
use std::path::Path;
#[derive(Clone)]
pub struct ExplainedFunction {
    pub file: String,
    pub language: String,
    pub symbol: SourceSymbol,
    pub explanation: FunctionExplanation,
}
pub fn symbols(root: &Path, c: &FileChange) -> Result<Vec<SourceSymbol>> {
    let Some(analyzer) = LanguageRegistry::analyzer_for_path(&c.path) else {
        return Ok(vec![]);
    };
    let source = match std::fs::read_to_string(root.join(&c.path)) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: unable to read changed file: {error}", c.path.display());
            return Ok(vec![]);
        }
    };
    match analyzer.find_containing_symbols(&source, &c.ranges) {
        Ok(symbols) => Ok(symbols),
        Err(error) => {
            eprintln!("{}: language analysis failed: {error}", c.path.display());
            Ok(vec![])
        }
    }
}
pub fn print_debug(root: &Path, changes: &[FileChange]) -> Result<()> {
    for c in changes {
        for s in symbols(root, c)? {
            println!(
                "{}\n\nChanged function:\n{}\nlines {}-{}\n\n{}",
                c.path.display(),
                s.name,
                s.start_line,
                s.end_line,
                s.source
            );
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
pub async fn explain_items(
    root: &Path,
    changes: &[FileChange],
    provider: impl ExplanationProvider,
) -> Result<Vec<ExplainedFunction>> {
    let mut all = vec![];
    for c in changes {
        for s in symbols(root, c)? {
            let e = provider
                .explain(ExplanationRequest {
                    function: s.source.clone(),
                    diff: relevant_diff(c, &s),
                    language: LanguageRegistry::language_for_path(&c.path)
                        .unwrap_or("unknown")
                        .into(),
                    deep: false,
                })
                .await
                .unwrap_or(FunctionExplanation {
                    overview: "Explanation unavailable.".into(),
                    annotations: vec![],
                    deep: None,
                });
            all.push(ExplainedFunction {
                file: c.path.display().to_string(),
                language: LanguageRegistry::language_for_path(&c.path)
                    .unwrap_or("unknown")
                    .into(),
                symbol: s,
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
            ranges: vec![
                LineRange { start: 2, end: 2 },
                LineRange { start: 8, end: 8 },
            ],
            diff: "diff --git a/src/users.rs b/src/users.rs\n+++ b/src/users.rs\n@@ -2 +2 @@\n-old\n+new one\n@@ -8 +8 @@\n-old\n+new two\n".into(),
        };
        let symbol = SourceSymbol {
            name: "first".into(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 3,
            source: "fn first() {}".into(),
        };
        let diff = relevant_diff(&change, &symbol);
        assert!(diff.contains("new one"));
        assert!(!diff.contains("new two"));
    }
}
