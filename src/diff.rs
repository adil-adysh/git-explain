//! Unified patches are supplemental text metadata. Git raw status is the
//! authority for whether a file exists in a change set and how it changed.
use crate::change_set::FilePatch;
pub use crate::change_set::LineRange;
use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPatch {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub patch: FilePatch,
}

pub fn parse_unified_patches(input: &str) -> Result<Vec<ParsedPatch>> {
    let mut result = Vec::new();
    let mut current: Option<ParsedPatch> = None;
    let mut old_path = None;
    for line in input.lines() {
        if let Some(path) = line.strip_prefix("--- ") {
            let path = path.split('\t').next().unwrap_or(path);
            old_path = (path != "/dev/null")
                .then(|| PathBuf::from(path.strip_prefix("a/").unwrap_or(path)));
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if let Some(patch) = current.take() {
                result.push(patch);
            }
            let path = path.split('\t').next().unwrap_or(path);
            let new_path = (path != "/dev/null")
                .then(|| PathBuf::from(path.strip_prefix("b/").unwrap_or(path)));
            current = Some(ParsedPatch {
                old_path: old_path.clone(),
                new_path,
                patch: FilePatch {
                    ranges: vec![],
                    text: String::new(),
                },
            });
        }
        if let Some(hunk) = line.strip_prefix("@@ ") {
            if let Some(new_part) = hunk.split_whitespace().find(|part| part.starts_with('+')) {
                let mut values = new_part.trim_start_matches('+').split(',');
                let start = values.next().context("invalid diff hunk")?.parse()?;
                let count = values.next().unwrap_or("1").parse::<usize>()?;
                if let Some(current) = current.as_mut() {
                    if count > 0 {
                        current.patch.ranges.push(LineRange {
                            start,
                            end: start + count - 1,
                        });
                    }
                }
            }
        }
        if let Some(current) = current.as_mut() {
            current.patch.text.push_str(line);
            current.patch.text.push('\n');
        }
    }
    if let Some(patch) = current {
        result.push(patch);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_ranges_without_deciding_file_status() {
        let patches = parse_unified_patches(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -2,0 +3,2 @@\n+x\n+y\n",
        )
        .unwrap();
        assert_eq!(patches[0].new_path, Some(PathBuf::from("a.rs")));
        assert_eq!(
            patches[0].patch.ranges,
            vec![LineRange { start: 3, end: 4 }]
        );
    }
}
