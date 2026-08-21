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
