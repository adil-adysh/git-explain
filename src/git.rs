use crate::{
    change_set::{FileChange, FileChangeKind},
    config::GitConfig,
    diff::{parse_unified_patches, ParsedPatch},
};
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryContext {
    pub worktree_root: PathBuf,
    pub git_dir: PathBuf,
}
impl RepositoryContext {
    pub fn discover() -> Result<Self> {
        let worktree_root = PathBuf::from(
            git_output_from_current(&["rev-parse", "--show-toplevel"])
                .context("not inside a Git repository")?,
        );
        Ok(Self {
            git_dir: git_dir(&worktree_root)?,
            worktree_root,
        })
    }
}
pub fn git_dir(root: &Path) -> Result<PathBuf> {
    let path = PathBuf::from(git_output(root, &["rev-parse", "--git-dir"])?);
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

#[derive(Clone, Debug)]
pub struct CommitAnalysis {
    pub oid: String,
    pub parent_oid: Option<String>,
    pub parent_count: usize,
    pub subject: String,
    pub files: Vec<FileChange>,
}
pub fn working_tree_files(root: &Path, config: &GitConfig) -> Result<Vec<FileChange>> {
    let range = config
        .include_staged
        .then_some(vec![config.diff_target.as_str()])
        .unwrap_or_default();
    collect_files(root, &range)
}
pub fn commit_analysis(root: &Path, revision: &str) -> Result<CommitAnalysis> {
    let oid = resolve_revision(root, revision)?;
    let parents: Vec<_> = git_output(root, &["rev-list", "--parents", "-n", "1", &oid])?
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect();
    let parent_oid = parents.first().cloned();
    let from = parent_oid
        .clone()
        .unwrap_or_else(|| "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into());
    Ok(CommitAnalysis {
        files: collect_files(root, &[&from, &oid])?,
        subject: git_output(root, &["show", "-s", "--format=%s", &oid])?,
        oid,
        parent_oid,
        parent_count: parents.len(),
    })
}

/// Raw NUL-delimited Git records decide file identity; patches merely supply
/// textual hunk ranges and may be absent (for example, for R100).
fn collect_files(root: &Path, range: &[&str]) -> Result<Vec<FileChange>> {
    let raw = git_bytes(
        root,
        &args(range, &["diff", "--raw", "-z", "--no-abbrev", "-M", "-C"]),
    )?;
    let mut files = parse_raw_metadata(&raw)?;
    let patch = git_output(
        root,
        &args(
            range,
            &["diff", "--patch", "--unified=0", "--no-color", "-M", "-C"],
        ),
    )?;
    let patches = parse_unified_patches(&patch)?;
    for file in &mut files {
        file.patch = matching_patch(file, &patches).map(|patch| patch.patch.clone());
    }
    mark_binary(
        &mut files,
        &git_bytes(root, &args(range, &["diff", "--numstat", "-z", "-M", "-C"]))?,
    )?;
    Ok(files)
}
fn args<'a>(range: &'a [&'a str], prefix: &[&'a str]) -> Vec<&'a str> {
    let mut args = prefix.to_vec();
    args.extend_from_slice(range);
    args
}

fn parse_raw_metadata(raw: &[u8]) -> Result<Vec<FileChange>> {
    let tokens: Vec<&[u8]> = raw
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect();
    let mut files = vec![];
    let mut index = 0;
    while index < tokens.len() {
        let header = std::str::from_utf8(tokens[index]).context("Git raw metadata is not UTF-8")?;
        index += 1;
        let parts: Vec<_> = header.split_whitespace().collect();
        if parts.len() < 5 || !parts[0].starts_with(':') {
            anyhow::bail!("unexpected Git raw record: {header}");
        }
        let status = parts[4];
        let code = status.chars().next().unwrap_or('?');
        let (old_path, new_path) = match code {
            'R' | 'C' => {
                let old = path(tokens.get(index).context("raw rename missing old path")?)?;
                index += 1;
                let new = path(tokens.get(index).context("raw rename missing new path")?)?;
                index += 1;
                (Some(old), Some(new))
            }
            'A' => {
                let value = path(tokens.get(index).context("raw add missing path")?)?;
                index += 1;
                (None, Some(value))
            }
            'D' => {
                let value = path(tokens.get(index).context("raw delete missing path")?)?;
                index += 1;
                (Some(value), None)
            }
            _ => {
                let value = path(tokens.get(index).context("raw diff missing path")?)?;
                index += 1;
                (Some(value.clone()), Some(value))
            }
        };
        let kind = match code {
            'A' => FileChangeKind::Added,
            'D' => FileChangeKind::Deleted,
            'R' => FileChangeKind::Renamed,
            'C' => FileChangeKind::Copied,
            'T' => FileChangeKind::TypeChanged,
            _ => FileChangeKind::Modified,
        };
        files.push(FileChange {
            old_path,
            new_path,
            kind,
            similarity: status.get(1..).and_then(|value| value.parse().ok()),
            old_mode: Some(parts[0][1..].to_string()),
            new_mode: Some(parts[1].into()),
            binary: false,
            patch: None,
            before: None,
            after: None,
            semantic_changes: vec![],
        });
    }
    Ok(files)
}
fn path(token: &&[u8]) -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::str::from_utf8(token).context("Git filename is not valid UTF-8")?,
    ))
}
fn matching_patch<'a>(file: &FileChange, patches: &'a [ParsedPatch]) -> Option<&'a ParsedPatch> {
    patches
        .iter()
        .find(|patch| patch.old_path == file.old_path && patch.new_path == file.new_path)
        .or_else(|| {
            patches
                .iter()
                .find(|patch| patch.new_path == file.new_path || patch.old_path == file.old_path)
        })
}
fn mark_binary(files: &mut [FileChange], raw: &[u8]) -> Result<()> {
    let tokens: Vec<&[u8]> = raw
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect();
    let mut index = 0;
    while index + 1 < tokens.len() {
        let counts = std::str::from_utf8(tokens[index])?;
        index += 1;
        let path = path(&tokens[index])?;
        index += 1;
        if counts == "-\t-" {
            if let Some(file) = files.iter_mut().find(|file| {
                file.new_path.as_ref() == Some(&path) || file.old_path.as_ref() == Some(&path)
            }) {
                file.binary = true;
            }
        }
    }
    Ok(())
}
pub fn head_oid(root: &Path) -> Result<String> {
    git_output(root, &["rev-parse", "HEAD"])
}
pub fn resolve_revision(root: &Path, revision: &str) -> Result<String> {
    let expression = format!("{revision}^{{commit}}");
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", &expression])
        .output()
        .context("resolve Git revision")?;
    if !output.status.success() {
        anyhow::bail!("Unable to resolve Git revision '{revision}' to a commit.");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}
fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(root, args)?)?.trim().to_owned())
}
fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(output.stdout)
}
fn git_output_from_current(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
