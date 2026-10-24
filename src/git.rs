use crate::{
    change_set::{FileChange, FileChangeKind, FilePatch, LineRange},
    config::GitConfig,
    diff::{parse_unified_patches, ParsedPatch},
};
use anyhow::{Context, Result};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

pub fn files_at_revision(
    root: &Path,
    revision: Option<&str>,
    selections: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let normalized: Vec<String> = selections
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut command = Command::new("git");
    command.current_dir(root);
    if let Some(oid) = revision {
        command.args(["ls-tree", "-r", "--name-only", "-z", oid, "--"]);
    } else {
        command.args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ]);
    }
    command.args(&normalized);
    let output = command.output().context("list selected Git files")?;
    if !output.status.success() {
        anyhow::bail!(
            "list selected Git files failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let files = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| {
            std::str::from_utf8(value)
                .map(PathBuf::from)
                .context("selected Git path is not UTF-8")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    if files.is_empty() {
        anyhow::bail!("No files matched the selected file or directory paths.");
    }
    Ok(files.into_iter().collect())
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    if let Some(s) = path.to_str() {
        if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{}", stripped));
        }
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
}

#[cfg(windows)]
fn normalize_path_identity(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return strip_verbatim_prefix(canonical);
    }
    let mut remainder = PathBuf::new();
    let mut current = Some(path);
    while let Some(ancestor) = current {
        if ancestor.as_os_str().is_empty() {
            break;
        }
        if let Ok(canonical) = ancestor.canonicalize() {
            let canonical_ancestor = strip_verbatim_prefix(canonical);
            return if remainder.as_os_str().is_empty() {
                canonical_ancestor
            } else {
                canonical_ancestor.join(remainder)
            };
        }
        if let Some(file_name) = ancestor.file_name() {
            if remainder.as_os_str().is_empty() {
                remainder = PathBuf::from(file_name);
            } else {
                remainder = PathBuf::from(file_name).join(remainder);
            }
        }
        current = ancestor.parent();
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn normalize_path_identity(path: &Path) -> PathBuf {
    path.to_path_buf()
}

pub fn normalize_selections(
    repository_root: &Path,
    invocation_directory: &Path,
    selections: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let canonical_root = normalize_path_identity(repository_root);
    let canonical_invocation = normalize_path_identity(invocation_directory);
    let invocation_prefix = canonical_invocation
        .strip_prefix(&canonical_root)
        .with_context(|| {
            format!(
                "current directory is outside the Git worktree: prefix not found (current: {}, root: {})",
                invocation_directory.display(),
                repository_root.display()
            )
        })?;
    selections
        .iter()
        .map(|selection| {
            let canonical_selection;
            let combined;
            let relative: &Path = if selection.is_absolute() {
                canonical_selection = normalize_path_identity(selection);
                canonical_selection
                    .strip_prefix(&canonical_root)
                    .with_context(|| {
                        format!(
                            "selected path '{}' is outside the Git worktree",
                            selection.display()
                        )
                    })?
            } else {
                combined = invocation_prefix.join(selection);
                &combined
            };
            normalize_relative_path(relative)
        })
        .collect()
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    anyhow::bail!(
                        "selected path '{}' escapes the Git worktree",
                        path.display()
                    );
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                anyhow::bail!(
                    "selected path '{}' is not repository-relative",
                    path.display()
                );
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    Ok(normalized)
}

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
#[allow(dead_code)]
pub struct CommitAnalysis {
    pub oid: String,
    pub parent_oid: Option<String>,
    pub parent_count: usize,
    pub subject: String,
    pub files: Vec<FileChange>,
}
pub struct RangeAnalysis {
    pub expression: String,
    pub from_oid: String,
    pub to_oid: String,
    pub files: Vec<FileChange>,
}

pub fn range_analysis(root: &Path, expression: &str) -> Result<RangeAnalysis> {
    let (from_oid, to_oid) = if let Some((left, right)) = expression.split_once("...") {
        let left = resolve_revision(root, left)?;
        let right = resolve_revision(root, right)?;
        (git_output(root, &["merge-base", &left, &right])?, right)
    } else if let Some((left, right)) = expression.split_once("..") {
        (
            resolve_revision(root, left)?,
            resolve_revision(root, right)?,
        )
    } else {
        anyhow::bail!("Git range '{expression}' must contain '..' or '...'");
    };
    Ok(RangeAnalysis {
        files: collect_files(root, &[&from_oid, &to_oid])?,
        expression: expression.into(),
        from_oid,
        to_oid,
    })
}
pub fn working_tree_files(root: &Path, config: &GitConfig) -> Result<Vec<FileChange>> {
    let range = config
        .include_staged
        .then_some(vec![config.diff_target.as_str()])
        .unwrap_or_default();
    let mut files = collect_files(root, &range)?;
    if config.include_untracked {
        append_untracked_files(root, &mut files)?;
    }
    files.sort_by(|left, right| left.display_path().cmp(right.display_path()));
    Ok(files)
}

fn append_untracked_files(root: &Path, files: &mut Vec<FileChange>) -> Result<()> {
    let output = git_bytes(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for value in output
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(
            std::str::from_utf8(value).context("untracked Git path is not valid UTF-8")?,
        );
        let absolute = root.join(&path);
        let metadata = std::fs::symlink_metadata(&absolute)
            .with_context(|| format!("inspect untracked file {}", path.display()))?;
        let (new_mode, binary, patch) = if metadata.file_type().is_symlink() {
            (Some("120000".into()), false, None)
        } else {
            let content = std::fs::read(&absolute)
                .with_context(|| format!("read untracked file {}", path.display()))?;
            let binary = content.contains(&0);
            let patch = (!binary).then(|| untracked_patch(&content));
            (Some("100644".into()), binary, patch)
        };
        files.push(FileChange {
            old_path: None,
            new_path: Some(path),
            kind: FileChangeKind::Added,
            similarity: None,
            old_mode: None,
            new_mode,
            binary,
            patch,
            before: None,
            after: None,
            semantic_changes: vec![],
        });
    }
    Ok(())
}

fn untracked_patch(content: &[u8]) -> FilePatch {
    let text = String::from_utf8_lossy(content);
    let line_count = text.lines().count();
    let mut patch = format!("@@ -0,0 +1,{line_count} @@\n");
    for line in text.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    FilePatch {
        ranges: (line_count > 0)
            .then_some(LineRange {
                start: 1,
                end: line_count,
            })
            .into_iter()
            .collect(),
        text: patch,
    }
}
pub fn staged_files(root: &Path, config: &GitConfig) -> Result<Vec<FileChange>> {
    collect_files(root, &["--cached", config.diff_target.as_str()])
}
#[allow(dead_code)]
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
        &args(
            range,
            &[
                "diff",
                "--raw",
                "-z",
                "--no-abbrev",
                "-M",
                "-C",
                "--find-copies-harder",
            ],
        ),
    )?;
    let mut files = parse_raw_metadata(&raw)?;
    let patch = git_output(
        root,
        &args(
            range,
            &[
                "diff",
                "--patch",
                "--unified=0",
                "--no-color",
                "-M",
                "-C",
                "--find-copies-harder",
            ],
        ),
    )?;
    let patches = parse_unified_patches(&patch)?;
    for file in &mut files {
        file.patch = matching_patch(file, &patches).map(|patch| patch.patch.clone());
    }
    mark_binary(
        &mut files,
        &git_bytes(
            root,
            &args(
                range,
                &[
                    "diff",
                    "--numstat",
                    "-z",
                    "-M",
                    "-C",
                    "--find-copies-harder",
                ],
            ),
        )?,
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
    let tokens: Vec<&[u8]> = raw.split(|byte| *byte == 0).collect();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].is_empty() {
            index += 1;
            continue;
        }
        let header = std::str::from_utf8(tokens[index])?;
        index += 1;
        let mut fields = header.splitn(3, '\t');
        let added = fields.next().unwrap_or("");
        let deleted = fields.next().unwrap_or("");
        let inline_path = fields.next().unwrap_or("");
        let path = if inline_path.is_empty() {
            let _old = path(
                tokens
                    .get(index)
                    .context("numstat rename missing old path")?,
            )?;
            index += 1;
            let new = path(
                tokens
                    .get(index)
                    .context("numstat rename missing new path")?,
            )?;
            index += 1;
            new
        } else {
            PathBuf::from(inline_path)
        };
        if added == "-" && deleted == "-" {
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
