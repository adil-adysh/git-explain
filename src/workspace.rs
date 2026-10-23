use crate::{
    change_set::{ChangeSet, FileChangeKind, SemanticUnitChangeKind},
    file_analysis::{unit_key, FileAnalysis},
    runtime::explanation_regions,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceIndex {
    pub schema_version: u32,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub generation: u64,
    pub snapshot_id: String,
    pub revision_label: String,
    pub capabilities: WorkspaceCapabilities,
    pub files: Vec<WorkspaceFile>,
}

impl WorkspaceIndex {
    pub fn from_file(analysis: &FileAnalysis) -> Self {
        Workspace::from_file(analysis).index
    }

    pub fn from_diff(change_set: &ChangeSet) -> Self {
        Workspace::from_diff(change_set).index
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCapabilities {
    pub changes_view: bool,
    pub learning_profiles: bool,
    pub section_override: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    pub id: String,
    pub path_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path_label: Option<String>,
    pub status: String,
    pub language: String,
    pub section_count: usize,
    pub sections: Vec<WorkspaceSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSection {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub source_side: String,
    pub change_status: String,
    pub capabilities: Vec<String>,
    pub generation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationRegion {
    pub id: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionDetail {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub generation: u64,
    pub file_id: String,
    pub section_id: String,
    pub generation_id: String,
    pub language: String,
    pub source_side: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub line_range: LineRange,
    pub valid_annotation_regions: Vec<AnnotationRegion>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceError {
    UnknownSection(String),
    StaleSnapshot { expected: String, actual: String },
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSection(id) => write!(f, "Unknown section id: {id}"),
            Self::StaleSnapshot { expected, actual } => {
                write!(f, "Stale snapshot: expected {expected}, actual {actual}")
            }
        }
    }
}

impl std::error::Error for WorkspaceError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    index: WorkspaceIndex,
    details: BTreeMap<String, SectionDetail>,
}

impl Workspace {
    /// Separate otherwise identical source selections owned by different sessions.
    pub fn with_context(mut self, context: &str) -> Self {
        let scoped = |id: &str| {
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(context, id)).expect("identity tuple"))
            )
        };
        self.index.snapshot_id = scoped(&self.index.snapshot_id);
        for file in &mut self.index.files {
            file.id = scoped(&file.id);
            for section in &mut file.sections {
                section.id = scoped(&section.id);
            }
        }
        self.details = self
            .details
            .into_values()
            .map(|mut detail| {
                detail.snapshot_id = self.index.snapshot_id.clone();
                detail.file_id = scoped(&detail.file_id);
                detail.section_id = scoped(&detail.section_id);
                (detail.section_id.clone(), detail)
            })
            .collect();
        self
    }

    pub fn from_file(analysis: &FileAnalysis) -> Self {
        let revision_label = analysis
            .revision
            .as_deref()
            .unwrap_or("working tree")
            .to_string();

        let mut hasher = Sha256::new();
        hasher.update(b"file_mode:\n");
        hasher.update(revision_label.as_bytes());
        hasher.update(b"\n");
        for doc in &analysis.documents {
            hasher.update(doc.source.path.to_string_lossy().as_bytes());
            hasher.update(b":");
            hasher.update(doc.source.content.as_bytes());
            hasher.update(b"\n");
        }
        let snapshot_id = format!("{:x}", hasher.finalize());

        let mut files = Vec::new();
        let mut details = BTreeMap::new();

        for doc in &analysis.documents {
            let Some(doc_language) = &doc.language else {
                continue;
            };

            let path_label = doc.source.path.to_string_lossy().into_owned();

            let mut file_hasher = Sha256::new();
            file_hasher.update(snapshot_id.as_bytes());
            file_hasher.update(b":file:file_mode:");
            file_hasher.update(path_label.as_bytes());
            file_hasher.update(b":");
            file_hasher.update(doc.source.content.as_bytes());
            let file_id = format!("{:x}", file_hasher.finalize());

            let mut sections = Vec::new();
            for unit in &doc.units {
                let gen_id = unit_key(unit);
                let name = unit.identity.qualified_name.clone();
                let kind = format!("{:?}", unit.identity.kind).to_lowercase();
                let start_line = unit.location.start_line;
                let end_line = unit.location.end_line;
                let source_side = "after".to_string();
                let change_status = "unchanged".to_string();
                let capabilities = vec![
                    "read_source".into(),
                    "overview".into(),
                    "concepts".into(),
                    "deep".into(),
                ];

                let mut sec_hasher = Sha256::new();
                sec_hasher.update(snapshot_id.as_bytes());
                sec_hasher.update(b":section:file_mode:");
                sec_hasher.update(path_label.as_bytes());
                sec_hasher.update(b":after:");
                sec_hasher.update(name.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(kind.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(unit.identity.occurrence.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(start_line.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(end_line.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(unit.source.as_bytes());
                let section_id = format!("{:x}", sec_hasher.finalize());

                let regions = explanation_regions(unit, None);
                let valid_annotation_regions = regions
                    .into_iter()
                    .map(|r| AnnotationRegion {
                        id: r.id,
                        start_line: r.start_line,
                        end_line: r.end_line,
                        source: r.source,
                    })
                    .collect();

                let detail = SectionDetail {
                    schema_version: 1,
                    snapshot_id: snapshot_id.clone(),
                    generation: 0,
                    file_id: file_id.clone(),
                    section_id: section_id.clone(),
                    generation_id: gen_id.clone(),
                    language: doc_language.clone(),
                    source_side: source_side.clone(),
                    source: unit.source.clone(),
                    old_source: None,
                    diff: None,
                    line_range: LineRange {
                        start: start_line,
                        end: end_line,
                    },
                    valid_annotation_regions,
                    capabilities: capabilities.clone(),
                };

                details.insert(section_id.clone(), detail);

                sections.push(WorkspaceSection {
                    id: section_id,
                    name,
                    kind,
                    start_line,
                    end_line,
                    source_side,
                    change_status,
                    capabilities,
                    generation_id: gen_id,
                });
            }

            sections.sort_by(|a, b| {
                a.start_line
                    .cmp(&b.start_line)
                    .then_with(|| a.end_line.cmp(&b.end_line))
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.id.cmp(&b.id))
            });

            files.push(WorkspaceFile {
                id: file_id,
                path_label,
                old_path_label: None,
                status: "unchanged".into(),
                language: doc_language.clone(),
                section_count: sections.len(),
                sections,
            });
        }

        Self {
            index: WorkspaceIndex {
                schema_version: 1,
                mode: "file".into(),
                session_id: None,
                generation: 0,
                snapshot_id,
                revision_label,
                capabilities: WorkspaceCapabilities {
                    changes_view: false,
                    learning_profiles: true,
                    section_override: true,
                },
                files,
            },
            details,
        }
    }

    pub fn from_diff(change_set: &ChangeSet) -> Self {
        let revision_label = match &change_set.context.mode {
            crate::explain::AnalysisMode::WorkingTree => "working tree".to_string(),
            crate::explain::AnalysisMode::Commit { oid, .. } => format!("commit {oid}"),
            crate::explain::AnalysisMode::Range { expression, .. } => expression.clone(),
        };

        let mut hasher = Sha256::new();
        hasher.update(b"diff_mode:\n");
        hasher.update(format!("{:?}\n", change_set.identity).as_bytes());
        hasher.update(change_set.context.prompt_context().as_bytes());
        hasher.update(b"\n");
        for file in &change_set.files {
            if let Some(p) = &file.old_path {
                hasher.update(p.to_string_lossy().as_bytes());
            }
            hasher.update(b":");
            if let Some(p) = &file.new_path {
                hasher.update(p.to_string_lossy().as_bytes());
            }
            hasher.update(b"\n");
            if let Some(before) = &file.before {
                hasher.update(before.content.as_bytes());
            }
            hasher.update(b":");
            if let Some(after) = &file.after {
                hasher.update(after.content.as_bytes());
            }
            hasher.update(b":");
            if let Some(patch) = &file.patch {
                hasher.update(patch.text.as_bytes());
            }
            hasher.update(b"\n");
        }
        let snapshot_id = format!("{:x}", hasher.finalize());

        let mut files = Vec::new();
        let mut details = BTreeMap::new();

        for file in &change_set.files {
            if file.binary
                || crate::language::LanguageRegistry::analyzer_for_path(file.display_path())
                    .is_none()
            {
                continue;
            }

            let path_label = file.display_path().to_string_lossy().into_owned();
            let old_path_label = file
                .old_path
                .as_ref()
                .filter(|_| {
                    file.kind == FileChangeKind::Renamed || file.kind == FileChangeKind::Copied
                })
                .map(|p| p.to_string_lossy().into_owned());

            let status = match file.kind {
                FileChangeKind::Added => "added",
                FileChangeKind::Modified => "modified",
                FileChangeKind::Deleted => "deleted",
                FileChangeKind::Renamed => "renamed",
                FileChangeKind::Copied => "copied",
                FileChangeKind::TypeChanged => "type_changed",
            }
            .to_string();

            let language = file
                .semantic_changes
                .first()
                .map(|c| c.active_unit().language.clone())
                .or_else(|| {
                    crate::language::LanguageRegistry::language_for_path(file.display_path())
                        .map(|s| s.to_lowercase())
                })
                .unwrap_or_else(|| "unknown".into());

            let mut file_hasher = Sha256::new();
            file_hasher.update(snapshot_id.as_bytes());
            file_hasher.update(b":file:diff_mode:");
            file_hasher.update(path_label.as_bytes());
            file_hasher.update(b":");
            if let Some(old) = &old_path_label {
                file_hasher.update(old.as_bytes());
            }
            file_hasher.update(b":");
            if let Some(before) = &file.before {
                file_hasher.update(before.content.as_bytes());
            }
            file_hasher.update(b":");
            if let Some(after) = &file.after {
                file_hasher.update(after.content.as_bytes());
            }
            let file_id = format!("{:x}", file_hasher.finalize());

            let mut sections = Vec::new();
            for change in &file.semantic_changes {
                let unit = change.active_unit();
                let gen_id = change.id.0.clone();
                let name = unit.identity.qualified_name.clone();
                let kind = format!("{:?}", unit.identity.kind).to_lowercase();
                let start_line = unit.location.start_line;
                let end_line = unit.location.end_line;
                let source_side = if change.after.is_some() {
                    "after".to_string()
                } else {
                    "before".to_string()
                };

                let change_status = match change.kind {
                    SemanticUnitChangeKind::Added => "added",
                    SemanticUnitChangeKind::Modified => "modified",
                    SemanticUnitChangeKind::Deleted => "deleted",
                    SemanticUnitChangeKind::Preserved => "unchanged",
                }
                .to_string();

                let capabilities = if change.kind.explainable() {
                    vec![
                        "read_source".into(),
                        "overview".into(),
                        "changes".into(),
                        "concepts".into(),
                        "deep".into(),
                    ]
                } else {
                    vec!["read_source".into()]
                };

                let mut sec_hasher = Sha256::new();
                sec_hasher.update(snapshot_id.as_bytes());
                sec_hasher.update(b":section:diff_mode:");
                sec_hasher.update(path_label.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(source_side.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(name.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(kind.as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(unit.identity.occurrence.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(start_line.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(end_line.to_string().as_bytes());
                sec_hasher.update(b":");
                sec_hasher.update(unit.source.as_bytes());
                let section_id = format!("{:x}", sec_hasher.finalize());

                let regions = explanation_regions(
                    unit,
                    (change.after.is_some()).then(|| {
                        file.patch
                            .as_ref()
                            .map(|p| p.ranges.as_slice())
                            .unwrap_or(&[])
                    }),
                );
                let valid_annotation_regions = regions
                    .into_iter()
                    .map(|r| AnnotationRegion {
                        id: r.id,
                        start_line: r.start_line,
                        end_line: r.end_line,
                        source: r.source,
                    })
                    .collect();

                let old_source = match change.kind {
                    SemanticUnitChangeKind::Modified => {
                        change.before.as_ref().map(|b| b.source.clone())
                    }
                    _ => None,
                };

                let diff = match change.kind {
                    SemanticUnitChangeKind::Preserved => None,
                    _ => Some(change.diff.clone()),
                };

                let detail = SectionDetail {
                    schema_version: 1,
                    snapshot_id: snapshot_id.clone(),
                    generation: change_set.generation.0,
                    file_id: file_id.clone(),
                    section_id: section_id.clone(),
                    generation_id: gen_id.clone(),
                    language: unit.language.clone(),
                    source_side: source_side.clone(),
                    source: unit.source.clone(),
                    old_source,
                    diff,
                    line_range: LineRange {
                        start: start_line,
                        end: end_line,
                    },
                    valid_annotation_regions,
                    capabilities: capabilities.clone(),
                };

                details.insert(section_id.clone(), detail);

                sections.push(WorkspaceSection {
                    id: section_id,
                    name,
                    kind,
                    start_line,
                    end_line,
                    source_side,
                    change_status,
                    capabilities,
                    generation_id: gen_id,
                });
            }

            sections.sort_by(|a, b| {
                a.start_line
                    .cmp(&b.start_line)
                    .then_with(|| a.end_line.cmp(&b.end_line))
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.id.cmp(&b.id))
            });

            files.push(WorkspaceFile {
                id: file_id,
                path_label,
                old_path_label,
                status,
                language,
                section_count: sections.len(),
                sections,
            });
        }

        Self {
            index: WorkspaceIndex {
                schema_version: 1,
                mode: "diff".into(),
                session_id: None,
                generation: change_set.generation.0,
                snapshot_id,
                revision_label,
                capabilities: WorkspaceCapabilities {
                    changes_view: true,
                    learning_profiles: true,
                    section_override: true,
                },
                files,
            },
            details,
        }
    }

    pub fn index(&self) -> &WorkspaceIndex {
        &self.index
    }

    pub fn section(
        &self,
        id: &str,
        expected_snapshot: &str,
    ) -> Result<&SectionDetail, WorkspaceError> {
        if expected_snapshot != self.index.snapshot_id {
            return Err(WorkspaceError::StaleSnapshot {
                expected: expected_snapshot.to_string(),
                actual: self.index.snapshot_id.clone(),
            });
        }
        self.details
            .get(id)
            .ok_or_else(|| WorkspaceError::UnknownSection(id.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        change_set::{
            FileChange, FileChangeKind, FilePatch, LineRange as DiffLineRange, SemanticChangeId,
            SemanticIdentity, SemanticUnit, SemanticUnitChange, SourceFile, SourceLocation,
        },
        explain::AnalysisContext,
        language::SourceUnitKind,
        semantic::SemanticDocument,
        snapshot::{SnapshotGeneration, SnapshotIdentity},
    };
    use std::path::PathBuf;

    fn dummy_explanation() -> crate::model::UnitExplanation {
        crate::model::UnitExplanation {
            teaching: None,
            overview: String::new(),
            annotations: vec![],
            deep: None,
        }
    }

    fn sample_unit(
        path: &str,
        name: &str,
        start_line: usize,
        end_line: usize,
        source: &str,
    ) -> SemanticUnit {
        SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: name.into(),
                occurrence: 0,
            },
            location: SourceLocation {
                path: PathBuf::from(path),
                start_line,
                end_line,
            },
            source: source.into(),
            language: "rust".into(),
        }
    }

    #[test]
    fn session_context_prevents_cross_session_section_reuse() {
        let document = crate::semantic::extract_document(&SourceFile {
            path: "lib.rs".into(),
            content: "fn example() {}".into(),
        })
        .unwrap();
        let analysis = FileAnalysis {
            revision: None,
            documents: vec![document],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };
        let first = Workspace::from_file(&analysis).with_context("session-one");
        let second = Workspace::from_file(&analysis).with_context("session-two");
        let section = &first.index().files[0].sections[0];
        assert_ne!(first.index().snapshot_id, second.index().snapshot_id);
        assert_ne!(first.index().files[0].id, second.index().files[0].id);
        assert!(matches!(
            second.section(&section.id, &second.index().snapshot_id),
            Err(WorkspaceError::UnknownSection(_))
        ));
        let detail = first
            .section(&section.id, &first.index().snapshot_id)
            .unwrap();
        assert_eq!(detail.section_id, section.id);
        assert_eq!(detail.file_id, first.index().files[0].id);
        assert_eq!(detail.snapshot_id, first.index().snapshot_id);
    }

    #[test]
    fn metadata_excludes_source() {
        let doc = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/main.rs"),
                content: "fn test() { println!(\"secret\"); }\n".into(),
            },
            language: Some("rust".into()),
            units: vec![sample_unit(
                "src/main.rs",
                "test",
                1,
                1,
                "fn test() { println!(\"secret\"); }",
            )],
        };
        let analysis = FileAnalysis {
            revision: Some("main".into()),
            documents: vec![doc],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };

        let workspace = Workspace::from_file(&analysis);
        let index = workspace.index();
        assert_eq!(index.files.len(), 1, "supported file must be in index");
        assert_eq!(index.files[0].sections.len(), 1, "section must be in index");

        let json = serde_json::to_value(index).unwrap();
        let json_str = serde_json::to_string(&json).unwrap();
        assert!(!json_str.contains("secret"));
        assert!(!json_str.contains("println"));

        fn check_value(v: &serde_json::Value) {
            match v {
                serde_json::Value::Object(map) => {
                    assert!(!map.contains_key("source"));
                    assert!(!map.contains_key("old_source"));
                    assert!(!map.contains_key("diff"));
                    assert!(!map.contains_key("overview"));
                    assert!(!map.contains_key("explanation"));
                    for val in map.values() {
                        check_value(val);
                    }
                }
                serde_json::Value::Array(arr) => {
                    for val in arr {
                        check_value(val);
                    }
                }
                _ => {}
            }
        }
        check_value(&json);
    }

    #[test]
    fn complete_preserved_and_deleted() {
        let preserved_unit =
            sample_unit("src/lib.rs", "preserved_fn", 1, 3, "fn preserved_fn() {}");
        let deleted_unit = sample_unit("src/lib.rs", "deleted_fn", 5, 8, "fn deleted_fn() {}");
        let modified_before = sample_unit(
            "src/lib.rs",
            "modified_fn",
            10,
            15,
            "fn modified_fn() {\n    old();\n}",
        );
        let modified_after = sample_unit(
            "src/lib.rs",
            "modified_fn",
            10,
            16,
            "fn modified_fn() {\n    new();\n}",
        );
        let added_unit = sample_unit("src/lib.rs", "added_fn", 18, 22, "fn added_fn() {}");

        let change_set = ChangeSet {
            generation: SnapshotGeneration(1),
            identity: SnapshotIdentity::WorkingTree {
                fingerprint: "wt1".into(),
            },
            context: AnalysisContext::working_tree(),
            files: vec![FileChange {
                old_path: Some("src/lib.rs".into()),
                new_path: Some("src/lib.rs".into()),
                kind: FileChangeKind::Modified,
                similarity: None,
                old_mode: None,
                new_mode: None,
                binary: false,
                patch: Some(FilePatch {
                    ranges: vec![DiffLineRange { start: 10, end: 22 }],
                    text: "dummy patch".into(),
                }),
                before: Some(SourceFile {
                    path: "src/lib.rs".into(),
                    content: "source before".into(),
                }),
                after: Some(SourceFile {
                    path: "src/lib.rs".into(),
                    content: "source after".into(),
                }),
                semantic_changes: vec![
                    SemanticUnitChange {
                        id: SemanticChangeId("chg-pres".into()),
                        kind: SemanticUnitChangeKind::Preserved,
                        before: Some(preserved_unit.clone()),
                        after: Some(preserved_unit.clone()),
                        diff: String::new(),
                        overview_explanation: dummy_explanation(),
                        explanation: dummy_explanation(),
                        concept_explanation: dummy_explanation(),
                        deep_explanation: None,
                    },
                    SemanticUnitChange {
                        id: SemanticChangeId("chg-del".into()),
                        kind: SemanticUnitChangeKind::Deleted,
                        before: Some(deleted_unit.clone()),
                        after: None,
                        diff: "-fn deleted_fn() {}".into(),
                        overview_explanation: dummy_explanation(),
                        explanation: dummy_explanation(),
                        concept_explanation: dummy_explanation(),
                        deep_explanation: None,
                    },
                    SemanticUnitChange {
                        id: SemanticChangeId("chg-mod".into()),
                        kind: SemanticUnitChangeKind::Modified,
                        before: Some(modified_before.clone()),
                        after: Some(modified_after.clone()),
                        diff: " modified diff".into(),
                        overview_explanation: dummy_explanation(),
                        explanation: dummy_explanation(),
                        concept_explanation: dummy_explanation(),
                        deep_explanation: None,
                    },
                    SemanticUnitChange {
                        id: SemanticChangeId("chg-add".into()),
                        kind: SemanticUnitChangeKind::Added,
                        before: None,
                        after: Some(added_unit.clone()),
                        diff: "+fn added_fn() {}".into(),
                        overview_explanation: dummy_explanation(),
                        explanation: dummy_explanation(),
                        concept_explanation: dummy_explanation(),
                        deep_explanation: None,
                    },
                ],
            }],
        };

        let workspace = Workspace::from_diff(&change_set);
        let index = workspace.index();
        assert_eq!(index.files.len(), 1);
        let file = &index.files[0];
        assert_eq!(file.sections.len(), 4, "all 4 sections must be present");

        let pres_sec = file
            .sections
            .iter()
            .find(|s| s.name == "preserved_fn")
            .unwrap();
        assert_eq!(pres_sec.change_status, "unchanged");
        assert_eq!(pres_sec.capabilities, vec!["read_source"]);
        assert_eq!(pres_sec.source_side, "after");

        let del_sec = file
            .sections
            .iter()
            .find(|s| s.name == "deleted_fn")
            .unwrap();
        assert_eq!(del_sec.change_status, "deleted");
        assert_eq!(del_sec.source_side, "before");
        assert!(del_sec.capabilities.contains(&"changes".to_string()));

        let mod_sec = file
            .sections
            .iter()
            .find(|s| s.name == "modified_fn")
            .unwrap();
        assert_eq!(mod_sec.change_status, "modified");
        assert_eq!(mod_sec.source_side, "after");

        let add_sec = file.sections.iter().find(|s| s.name == "added_fn").unwrap();
        assert_eq!(add_sec.change_status, "added");
        assert_eq!(add_sec.source_side, "after");

        // Verify detail queries
        let pres_detail = workspace.section(&pres_sec.id, &index.snapshot_id).unwrap();
        assert_eq!(pres_detail.source, preserved_unit.source);
        assert_eq!(pres_detail.capabilities, vec!["read_source"]);
        assert!(pres_detail.diff.is_none());

        let del_detail = workspace.section(&del_sec.id, &index.snapshot_id).unwrap();
        assert_eq!(del_detail.source, deleted_unit.source);
        assert_eq!(del_detail.source_side, "before");
        assert_eq!(del_detail.diff.as_deref(), Some("-fn deleted_fn() {}"));
    }

    #[test]
    fn supported_empty_and_unsupported() {
        let empty_supported_doc = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/empty.rs"),
                content: "// only comments\n".into(),
            },
            language: Some("rust".into()),
            units: vec![],
        };
        let unsupported_doc = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("notes.txt"),
                content: "plain text\n".into(),
            },
            language: None,
            units: vec![],
        };

        let analysis = FileAnalysis {
            revision: None,
            documents: vec![empty_supported_doc, unsupported_doc],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };

        let workspace = Workspace::from_file(&analysis);
        let index = workspace.index();
        assert_eq!(index.files.len(), 1, "unsupported file must be omitted");
        assert_eq!(index.files[0].path_label, "src/empty.rs");
        assert_eq!(index.files[0].section_count, 0);
        assert!(index.files[0].sections.is_empty());

        // In diff mode
        let diff_supported_empty = FileChange {
            old_path: Some("src/empty.rs".into()),
            new_path: Some("src/empty.rs".into()),
            kind: FileChangeKind::Modified,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: None,
            before: Some(SourceFile {
                path: "src/empty.rs".into(),
                content: "".into(),
            }),
            after: Some(SourceFile {
                path: "src/empty.rs".into(),
                content: "// comment".into(),
            }),
            semantic_changes: vec![],
        };
        let diff_binary = FileChange {
            old_path: Some("image.png".into()),
            new_path: Some("image.png".into()),
            kind: FileChangeKind::Added,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: true,
            patch: None,
            before: None,
            after: None,
            semantic_changes: vec![],
        };
        let diff_unsupported = FileChange {
            old_path: Some("notes.txt".into()),
            new_path: Some("notes.txt".into()),
            kind: FileChangeKind::Modified,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: None,
            before: Some(SourceFile {
                path: "notes.txt".into(),
                content: "a".into(),
            }),
            after: Some(SourceFile {
                path: "notes.txt".into(),
                content: "b".into(),
            }),
            semantic_changes: vec![],
        };

        let change_set = ChangeSet {
            generation: SnapshotGeneration(1),
            identity: SnapshotIdentity::WorkingTree {
                fingerprint: "wt".into(),
            },
            context: AnalysisContext::working_tree(),
            files: vec![diff_supported_empty, diff_binary, diff_unsupported],
        };

        let workspace_diff = Workspace::from_diff(&change_set);
        let index_diff = workspace_diff.index();
        assert_eq!(
            index_diff.files.len(),
            1,
            "only supported empty file retained"
        );
        assert_eq!(index_diff.files[0].path_label, "src/empty.rs");
        assert_eq!(index_diff.files[0].section_count, 0);
    }

    #[test]
    fn ids_differ_same_symbol_source_across_paths_modes_snapshots() {
        let fn_source = "fn shared() -> bool { true }";
        let unit_a = sample_unit("src/a.rs", "shared", 1, 1, fn_source);
        let unit_b = sample_unit("src/b.rs", "shared", 1, 1, fn_source);

        let doc_a = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/a.rs"),
                content: fn_source.into(),
            },
            language: Some("rust".into()),
            units: vec![unit_a.clone()],
        };
        let doc_b = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/b.rs"),
                content: fn_source.into(),
            },
            language: Some("rust".into()),
            units: vec![unit_b.clone()],
        };

        let analysis_1 = FileAnalysis {
            revision: Some("v1".into()),
            documents: vec![doc_a.clone(), doc_b.clone()],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };

        let ws_1 = Workspace::from_file(&analysis_1);
        let sec_a1 = &ws_1.index().files[0].sections[0];
        let sec_b1 = &ws_1.index().files[1].sections[0];

        // 1. Same symbol & source across different paths -> different file and section IDs
        assert_ne!(ws_1.index().files[0].id, ws_1.index().files[1].id);
        assert_ne!(
            sec_a1.id, sec_b1.id,
            "different paths must produce different section IDs"
        );

        // 2. Same symbol & source across different snapshots (e.g. revision change or file content change)
        let analysis_2 = FileAnalysis {
            revision: Some("v2".into()),
            documents: vec![doc_a.clone(), doc_b.clone()],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };
        let ws_2 = Workspace::from_file(&analysis_2);
        assert_ne!(ws_1.index().snapshot_id, ws_2.index().snapshot_id);
        let sec_a2 = &ws_2.index().files[0].sections[0];
        assert_ne!(
            sec_a1.id, sec_a2.id,
            "different snapshots must produce different section IDs"
        );

        // 3. Same symbol across modes (file mode vs diff mode)
        let change_set = ChangeSet {
            generation: SnapshotGeneration(1),
            identity: SnapshotIdentity::WorkingTree {
                fingerprint: "wt".into(),
            },
            context: AnalysisContext::working_tree(),
            files: vec![FileChange {
                old_path: Some("src/a.rs".into()),
                new_path: Some("src/a.rs".into()),
                kind: FileChangeKind::Modified,
                similarity: None,
                old_mode: None,
                new_mode: None,
                binary: false,
                patch: None,
                before: Some(SourceFile {
                    path: "src/a.rs".into(),
                    content: fn_source.into(),
                }),
                after: Some(SourceFile {
                    path: "src/a.rs".into(),
                    content: fn_source.into(),
                }),
                semantic_changes: vec![SemanticUnitChange {
                    id: SemanticChangeId("chg-a".into()),
                    kind: SemanticUnitChangeKind::Modified,
                    before: Some(unit_a.clone()),
                    after: Some(unit_a.clone()),
                    diff: String::new(),
                    overview_explanation: dummy_explanation(),
                    explanation: dummy_explanation(),
                    concept_explanation: dummy_explanation(),
                    deep_explanation: None,
                }],
            }],
        };
        let ws_diff = Workspace::from_diff(&change_set);
        let sec_diff = &ws_diff.index().files[0].sections[0];
        assert_ne!(
            sec_a1.id, sec_diff.id,
            "different modes must produce different section IDs"
        );
    }

    #[test]
    fn stale_and_unknown_rejection() {
        let doc = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/main.rs"),
                content: "fn test() {}\n".into(),
            },
            language: Some("rust".into()),
            units: vec![sample_unit("src/main.rs", "test", 1, 1, "fn test() {}")],
        };
        let analysis = FileAnalysis {
            revision: Some("main".into()),
            documents: vec![doc],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };

        let workspace = Workspace::from_file(&analysis);
        let snap_id = &workspace.index().snapshot_id;
        let sec_id = &workspace.index().files[0].sections[0].id;

        // Valid query
        assert!(workspace.section(sec_id, snap_id).is_ok());

        // Unknown section
        let err_unknown = workspace
            .section("nonexistent_section_id", snap_id)
            .unwrap_err();
        match err_unknown {
            WorkspaceError::UnknownSection(id) => assert_eq!(id, "nonexistent_section_id"),
            _ => panic!("expected UnknownSection"),
        }

        // Stale snapshot
        let err_stale = workspace.section(sec_id, "wrong_snapshot_id").unwrap_err();
        match err_stale {
            WorkspaceError::StaleSnapshot { expected, actual } => {
                assert_eq!(expected, "wrong_snapshot_id");
                assert_eq!(actual, *snap_id);
            }
            _ => panic!("expected StaleSnapshot"),
        }
    }

    #[test]
    fn source_detail_accurate() {
        let fn_src = "fn compute() -> i32 {\n    42\n}";
        let unit = sample_unit("src/calc.rs", "compute", 10, 12, fn_src);
        let doc = SemanticDocument {
            source: SourceFile {
                path: PathBuf::from("src/calc.rs"),
                content: fn_src.into(),
            },
            language: Some("rust".into()),
            units: vec![unit.clone()],
        };
        let analysis = FileAnalysis {
            revision: None,
            documents: vec![doc],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };

        let workspace = Workspace::from_file(&analysis);
        let index = workspace.index();
        let sec = &index.files[0].sections[0];
        let detail = workspace.section(&sec.id, &index.snapshot_id).unwrap();

        assert_eq!(detail.source, fn_src);
        assert_eq!(detail.source_side, "after");
        assert!(detail.old_source.is_none());
        assert!(detail.diff.is_none());
        assert_eq!(detail.line_range, LineRange { start: 10, end: 12 });
        assert_eq!(
            detail.capabilities,
            vec!["read_source", "overview", "concepts", "deep"]
        );
        assert_eq!(detail.generation_id, crate::file_analysis::unit_key(&unit));
        assert!(!detail.valid_annotation_regions.is_empty());
        assert_eq!(detail.valid_annotation_regions[0].start_line, 1);
        assert_eq!(detail.valid_annotation_regions[0].end_line, 3);
    }
}
