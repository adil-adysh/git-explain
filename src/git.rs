use crate::config::GitConfig;
use crate::diff::{parse_unified_diff, FileChange};
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub fn repository_root() -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("run git")?;
    if !out.status.success() {
        anyhow::bail!("not inside a Git repository");
    }
    Ok(PathBuf::from(String::from_utf8(out.stdout)?.trim()))
}
pub fn working_tree_changes(root: &Path, config: &GitConfig) -> Result<Vec<FileChange>> {
    let mut args = vec!["diff".to_string(), "--unified=0".to_string()];
    if config.include_staged {
        args.push(config.diff_target.clone());
    }
    args.push("--no-color".into());
    let out = Command::new("git")
        .current_dir(root)
        .args(&args)
        .output()
        .context("run git diff")?;
    if !out.status.success() {
        anyhow::bail!("git diff failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    parse_unified_diff(&String::from_utf8(out.stdout)?)
}

#[derive(Clone, Debug)]
pub struct CommitAnalysis {
    pub oid: String,
    pub parent_oid: Option<String>,
    pub parent_count: usize,
    pub subject: String,
    pub changes: Vec<FileChange>,
}

pub fn commit_analysis(root: &Path, revision: &str) -> Result<CommitAnalysis> {
    let oid = resolve_revision(root, revision)?;
    let parent_line = git_output(root, &["rev-list", "--parents", "-n", "1", &oid])?;
    let parents: Vec<_> = parent_line
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect();
    let parent_oid = parents.first().cloned();
    let from = parent_oid
        .clone()
        .unwrap_or_else(|| "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into());
    let diff = git_output(root, &["diff", "--unified=0", "--no-color", &from, &oid])?;
    let subject = git_output(root, &["show", "-s", "--format=%s", &oid])?;
    Ok(CommitAnalysis {
        oid,
        parent_oid,
        parent_count: parents.len(),
        subject,
        changes: parse_unified_diff(&diff)?,
    })
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
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}
