use crate::change_set::{ChangeSet, FileChange};
use anyhow::{Context, Result};
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
        let spec = format!("{}:{}", self.oid, path.to_string_lossy().replace('\\', "/"));
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
pub struct GitIndexSourceProvider {
    root: PathBuf,
}
impl GitIndexSourceProvider {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into() }
    }
}
impl SourceProvider for GitIndexSourceProvider {
    fn read_file(&self, path: &Path) -> Result<String> {
        let spec = format!(":{}", path.to_string_lossy().replace('\\', "/"));
        let output = std::process::Command::new("git")
            .current_dir(&self.root)
            .args(["show", &spec])
            .output()
            .with_context(|| format!("read index source {}", path.display()))?;
        if !output.status.success() {
            anyhow::bail!(
                "git show {spec} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8(output.stdout)
            .with_context(|| format!("index file {} is not text", path.display()))
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
}
impl AnalysisContext {
    pub fn working_tree() -> Self {
        Self {
            mode: AnalysisMode::WorkingTree,
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

pub fn print_debug(change_set: &ChangeSet) -> Result<()> {
    println!("Git analysis\n\nFiles:\n{}", change_set.files.len());
    for file in &change_set.files {
        print_file_debug(file);
    }
    let summary = change_set.summary();
    println!("\nSemantic analysis:\n  Preserved: {}\n  Added: {}\n  Modified: {}\n  Deleted: {}\n\nModel requests:\n  {}", summary.units_preserved, summary.units_added, summary.units_modified, summary.units_deleted, change_set.explainable_changes().count());
    Ok(())
}
fn print_file_debug(file: &FileChange) {
    println!(
        "\n{}\n  Kind: {}",
        file.display_path().display(),
        file.status_label().to_lowercase()
    );
    if let Some(path) = &file.old_path {
        if Some(path) != file.new_path.as_ref() {
            println!("  Old path: {}", path.display());
        }
    }
    if let Some(similarity) = file.similarity {
        println!("  Similarity: {similarity}%");
    }
    if file.binary {
        println!("  Content: binary");
    }
    if file.mode_changed() {
        println!(
            "  Mode: {} -> {}",
            file.old_mode.as_deref().unwrap_or("<none>"),
            file.new_mode.as_deref().unwrap_or("<none>")
        );
    }
    println!(
        "  Textual hunks: {}",
        file.patch.as_ref().map_or(0, |patch| patch.ranges.len())
    );
}
