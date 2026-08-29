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
    #[allow(dead_code)]
    Commit {
        oid: String,
        parent_oid: Option<String>,
        subject: String,
        merge_parent_count: usize,
    },
    Range {
        expression: String,
        from_oid: String,
        to_oid: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisContext {
    pub mode: AnalysisMode,
    pub path_filters: Vec<PathBuf>,
}
impl AnalysisContext {
    pub fn working_tree() -> Self {
        Self {
            mode: AnalysisMode::WorkingTree,
            path_filters: vec![],
        }
    }
    pub fn prompt_context(&self) -> String {
        match &self.mode {
            AnalysisMode::WorkingTree => "Change source: working tree".into(),
            AnalysisMode::Commit { oid, subject, .. } => {
                format!("Change source: commit {oid}\nCommit subject: {subject}")
            }
            AnalysisMode::Range {
                expression,
                from_oid,
                to_oid,
            } => format!(
                "Change source: Git range {expression}\nBefore revision: {from_oid}\nAfter revision: {to_oid}"
            ),
        }
    }
}

pub fn print_debug(change_set: &ChangeSet) -> Result<()> {
    print!("{}", format_debug(change_set));
    Ok(())
}

pub fn format_debug(change_set: &ChangeSet) -> String {
    use std::fmt::Write;

    let mut output = String::new();
    writeln!(output, "Git analysis").unwrap();
    match &change_set.context.mode {
        AnalysisMode::WorkingTree => {
            writeln!(output, "\nSelected Git range: working tree").unwrap()
        }
        AnalysisMode::Commit {
            oid,
            parent_oid,
            subject,
            merge_parent_count,
        } => {
            writeln!(output, "\nSelected Git range: commit").unwrap();
            writeln!(output, "Commit: {oid}").unwrap();
            writeln!(
                output,
                "Compared with: {}",
                parent_oid.as_deref().unwrap_or("<empty tree>")
            )
            .unwrap();
            writeln!(output, "Subject: {subject}").unwrap();
            if *merge_parent_count > 1 {
                writeln!(output, "Merge policy: first parent").unwrap();
            }
        }
        AnalysisMode::Range {
            expression,
            from_oid,
            to_oid,
        } => {
            writeln!(output, "\nSelected Git range: {expression}").unwrap();
            writeln!(output, "Before revision: {from_oid}").unwrap();
            writeln!(output, "After revision: {to_oid}").unwrap();
        }
    }
    if !change_set.context.path_filters.is_empty() {
        writeln!(output, "\nSelected paths:").unwrap();
        for path in &change_set.context.path_filters {
            writeln!(output, "  {}", path.display()).unwrap();
        }
    }
    writeln!(output, "\nFiles: {}", change_set.files.len()).unwrap();
    for file in &change_set.files {
        format_file_debug(&mut output, file);
    }
    let summary = change_set.summary();
    write!(output, "\nSemantic analysis:\n  Preserved: {}\n  Added: {}\n  Modified: {}\n  Deleted: {}\n\nModel requests: {}\n", summary.units_preserved, summary.units_added, summary.units_modified, summary.units_deleted, change_set.explainable_changes().count()).unwrap();
    output
}
fn format_file_debug(output: &mut String, file: &FileChange) {
    use std::fmt::Write;

    writeln!(
        output,
        "\nFile: {}\n  Status: {}",
        file.display_path().display(),
        file.status_label()
    )
    .unwrap();
    if let Some(path) = &file.old_path {
        if Some(path) != file.new_path.as_ref() {
            writeln!(output, "  Previous path: {}", path.display()).unwrap();
        }
    }
    if let Some(similarity) = file.similarity {
        writeln!(output, "  Similarity: {similarity}%").unwrap();
    }
    if file.binary {
        writeln!(output, "  Content: Binary").unwrap();
    }
    if file.mode_changed() {
        writeln!(
            output,
            "  Mode: {} -> {}",
            file.old_mode.as_deref().unwrap_or("<none>"),
            file.new_mode.as_deref().unwrap_or("<none>")
        )
        .unwrap();
    }
    writeln!(
        output,
        "  Textual hunks: {}",
        file.patch.as_ref().map_or(0, |patch| patch.ranges.len())
    )
    .unwrap();
    for change in file
        .semantic_changes
        .iter()
        .filter(|change| change.kind.explainable())
    {
        let unit = change.active_unit();
        writeln!(
            output,
            "\n  Semantic change\n    Status: {:?}\n    Kind: {:?}\n    Name: {}\n    Lines: {}-{}\n    ID: {}\n\n{}",
            change.kind,
            unit.identity.kind,
            unit.identity.qualified_name,
            unit.location.start_line,
            unit.location.end_line,
            change.id,
            unit.source
        )
        .unwrap();
    }
}
