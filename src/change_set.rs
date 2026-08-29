use crate::{
    explain::AnalysisContext,
    language::SourceUnitKind,
    model::UnitExplanation,
    snapshot::{SnapshotGeneration, SnapshotIdentity},
};
use std::path::PathBuf;

/// The complete deterministic result of analysing one Git revision selection.
///
/// Files own their semantic changes.  Rendering and inference consume this
/// hierarchy directly; there is deliberately no parallel unit collection.
#[derive(Clone, Debug)]
pub struct ChangeSet {
    pub generation: SnapshotGeneration,
    pub identity: SnapshotIdentity,
    pub context: AnalysisContext,
    pub files: Vec<FileChange>,
}

impl ChangeSet {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"generation":self.generation.0,"identity":self.identity,"files":self.files.iter().map(|file| serde_json::json!({"kind":format!("{:?}",file.kind).to_lowercase(),"old_path":file.old_path.as_ref().map(|path|path.to_string_lossy()),"new_path":file.new_path.as_ref().map(|path|path.to_string_lossy()),"similarity":file.similarity,"binary":file.binary,"semantic_changes":file.semantic_changes.iter().map(|change| serde_json::json!({"id":change.id.0,"kind":format!("{:?}",change.kind).to_lowercase(),"before":change.before.as_ref().map(unit_json),"after":change.after.as_ref().map(unit_json)})).collect::<Vec<_>>() })).collect::<Vec<_>>()})
    }
    pub fn semantic_changes(&self) -> impl Iterator<Item = &SemanticUnitChange> {
        self.files
            .iter()
            .flat_map(|file| file.semantic_changes.iter())
    }

    pub fn explainable_changes(&self) -> impl Iterator<Item = &SemanticUnitChange> {
        self.semantic_changes()
            .filter(|change| change.kind.explainable())
    }

    pub fn find_change(&self, id: &SemanticChangeId) -> Option<(&FileChange, &SemanticUnitChange)> {
        self.files.iter().find_map(|file| {
            file.semantic_changes
                .iter()
                .find(|change| &change.id == id)
                .map(|change| (file, change))
        })
    }

    pub fn find_change_mut(&mut self, id: &SemanticChangeId) -> Option<&mut SemanticUnitChange> {
        self.files
            .iter_mut()
            .flat_map(|file| file.semantic_changes.iter_mut())
            .find(|change| &change.id == id)
    }

    pub fn summary(&self) -> ChangeSetSummary {
        let mut summary = ChangeSetSummary::default();
        for file in &self.files {
            match file.kind {
                FileChangeKind::Added => summary.files_added += 1,
                FileChangeKind::Modified => summary.files_modified += 1,
                FileChangeKind::Deleted => summary.files_deleted += 1,
                FileChangeKind::Renamed => summary.files_renamed += 1,
                FileChangeKind::Copied => summary.files_copied += 1,
                FileChangeKind::TypeChanged => summary.files_type_changed += 1,
            }
            if file.binary {
                summary.files_binary += 1;
            }
            if file.mode_changed() {
                summary.files_mode_changed += 1;
            }
            for change in &file.semantic_changes {
                match change.kind {
                    SemanticUnitChangeKind::Added => summary.units_added += 1,
                    SemanticUnitChangeKind::Modified => summary.units_modified += 1,
                    SemanticUnitChangeKind::Deleted => summary.units_deleted += 1,
                    SemanticUnitChangeKind::Preserved => summary.units_preserved += 1,
                }
            }
        }
        summary
    }

    pub fn empty_state(&self) -> Option<String> {
        if self.files.is_empty() {
            if !self.context.path_filters.is_empty() {
                return Some(
                    "No changes matched the selected paths in the selected Git range.".into(),
                );
            }
            return Some("No changes found for the selected Git range.".into());
        }
        if self.explainable_changes().next().is_some() {
            return None;
        }
        let summary = self.summary();
        if summary.files_renamed > 0
            && self
                .files
                .iter()
                .all(|file| file.kind != FileChangeKind::Renamed || !file.source_content_changed())
        {
            return Some(format!("{} file{} renamed without source-content changes. No semantic changes require explanation.", summary.files_renamed, plural(summary.files_renamed)));
        }
        if summary.files_copied > 0
            && self
                .files
                .iter()
                .all(|file| file.kind != FileChangeKind::Copied || !file.source_content_changed())
        {
            return Some(format!(
                "{} file{} copied without source-content changes. No semantic changes require explanation.",
                summary.files_copied,
                plural(summary.files_copied)
            ));
        }
        if summary.files_binary > 0 {
            return Some(format!(
                "{} binary file{} changed. Binary content has no semantic source explanation.",
                summary.files_binary,
                plural(summary.files_binary)
            ));
        }
        if summary.files_type_changed > 0 {
            return Some(format!(
                "The type changed for {} file{}. No semantic source explanation is available.",
                summary.files_type_changed,
                if summary.files_type_changed == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if summary.files_mode_changed > 0 {
            return Some(format!(
                "The mode changed for {} file{}. No semantic changes require explanation.",
                summary.files_mode_changed,
                if summary.files_mode_changed == 1 {
                    ""
                } else {
                    "s"
                }
            ));
        }
        if self.files.iter().any(|file| file.is_unsupported_source()) {
            return Some(
                "Source files changed, but their languages are not currently supported.".into(),
            );
        }
        Some("Source content changed, but no explainable semantic units were found.".into())
    }
}

fn unit_json(unit: &SemanticUnit) -> serde_json::Value {
    serde_json::json!({"identity":{"kind":format!("{:?}",unit.identity.kind).to_lowercase(),"qualified_name":unit.identity.qualified_name,"occurrence":unit.identity.occurrence},"location":{"path":unit.location.path.to_string_lossy(),"start_line":unit.location.start_line,"end_line":unit.location.end_line},"language":unit.language})
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        " was"
    } else {
        "s were"
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChangeSetSummary {
    pub files_added: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
    pub files_renamed: usize,
    pub files_copied: usize,
    pub files_type_changed: usize,
    pub files_binary: usize,
    pub files_mode_changed: usize,
    pub units_added: usize,
    pub units_modified: usize,
    pub units_deleted: usize,
    pub units_preserved: usize,
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub kind: FileChangeKind,
    pub similarity: Option<u8>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub binary: bool,
    pub patch: Option<FilePatch>,
    pub before: Option<SourceFile>,
    pub after: Option<SourceFile>,
    pub semantic_changes: Vec<SemanticUnitChange>,
}

impl FileChange {
    pub fn display_path(&self) -> &PathBuf {
        self.new_path
            .as_ref()
            .or(self.old_path.as_ref())
            .expect("file change must have a path")
    }
    pub fn mode_changed(&self) -> bool {
        matches!(
            (self.old_mode.as_deref(), self.new_mode.as_deref()),
            (Some(old), Some(new)) if old != "000000" && new != "000000" && old != new
        )
    }
    pub fn source_content_changed(&self) -> bool {
        match (&self.before, &self.after) {
            (Some(before), Some(after)) => before.content != after.content,
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => self
                .patch
                .as_ref()
                .is_some_and(|patch| !patch.ranges.is_empty()),
        }
    }
    pub fn is_unsupported_source(&self) -> bool {
        !self.binary
            && (self.before.is_some() || self.after.is_some())
            && crate::language::LanguageRegistry::analyzer_for_path(self.display_path()).is_none()
    }
    pub fn status_label(&self) -> &'static str {
        match (self.kind, self.source_content_changed()) {
            (FileChangeKind::Renamed, true) => "Renamed and modified",
            (FileChangeKind::Copied, true) => "Copied and modified",
            (FileChangeKind::Added, _) => "Added",
            (FileChangeKind::Modified, _) => "Modified",
            (FileChangeKind::Deleted, _) => "Deleted",
            (FileChangeKind::Renamed, _) => "Renamed",
            (FileChangeKind::Copied, _) => "Copied",
            (FileChangeKind::TypeChanged, _) => "Type changed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    pub ranges: Vec<LineRange>,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticIdentity {
    pub kind: SourceUnitKind,
    pub qualified_name: String,
    pub occurrence: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticUnit {
    pub identity: SemanticIdentity,
    pub location: SourceLocation,
    pub source: String,
    pub language: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticUnitChangeKind {
    Added,
    Modified,
    Deleted,
    Preserved,
}
impl SemanticUnitChangeKind {
    pub const fn explainable(self) -> bool {
        !matches!(self, Self::Preserved)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SemanticChangeId(pub String);
impl std::fmt::Display for SemanticChangeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug)]
pub struct SemanticUnitChange {
    pub id: SemanticChangeId,
    pub kind: SemanticUnitChangeKind,
    pub before: Option<SemanticUnit>,
    pub after: Option<SemanticUnit>,
    pub diff: String,
    pub overview_explanation: UnitExplanation,
    pub explanation: UnitExplanation,
    pub concept_explanation: UnitExplanation,
    pub deep_explanation: Option<String>,
}
impl SemanticUnitChange {
    pub fn active_unit(&self) -> &SemanticUnit {
        self.after
            .as_ref()
            .or(self.before.as_ref())
            .expect("semantic change has a unit")
    }
}
