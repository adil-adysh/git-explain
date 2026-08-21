use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: PathBuf,
    pub ranges: Vec<LineRange>,
    pub diff: String,
}

pub fn parse_unified_diff(input: &str) -> Result<Vec<FileChange>> {
    let mut out = Vec::new();
    let mut current: Option<FileChange> = None;
    for line in input.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            if let Some(file) = current.take() {
                out.push(file);
            }
            current = Some(FileChange {
                path: PathBuf::from(rest),
                ranges: vec![],
                diff: String::new(),
            });
        } else if line.starts_with("+++ /dev/null") {
            if let Some(file) = current.take() {
                out.push(file);
            }
        }
        if let Some(hunk) = line.strip_prefix("@@ ") {
            if let Some(new_part) = hunk.split_whitespace().find(|p| p.starts_with('+')) {
                let value = new_part.trim_start_matches('+');
                let mut parts = value.split(',');
                let start: usize = parts.next().context("invalid diff hunk")?.parse()?;
                let count: usize = parts.next().unwrap_or("1").parse()?;
                if let Some(file) = current.as_mut() {
                    if count > 0 {
                        file.ranges.push(LineRange {
                            start,
                            end: start + count - 1,
                        });
                    }
                }
            }
        }
        if let Some(file) = current.as_mut() {
            file.diff.push_str(line);
            file.diff.push('\n');
        }
    }
    if let Some(file) = current {
        out.push(file);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_ranges() {
        let d = "diff --git a/a.rs b/a.rs\n+++ b/a.rs\n@@ -2,0 +3,2 @@\n+x\n+y\n";
        let x = parse_unified_diff(d).unwrap();
        assert_eq!(x[0].ranges, vec![LineRange { start: 3, end: 4 }]);
    }
}
