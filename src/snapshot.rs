use crate::{
    diff::FileChange,
    explain::{AnalysisContext, ExplainedUnit},
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotGeneration(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapshotIdentity {
    WorkingTree { fingerprint: String },
    Commit { oid: String },
}

impl SnapshotIdentity {
    pub fn mode(&self) -> &'static str {
        match self {
            Self::WorkingTree { .. } => "working-tree",
            Self::Commit { .. } => "commit",
        }
    }
    pub fn short(&self) -> String {
        match self {
            Self::WorkingTree { fingerprint } => {
                fingerprint[..fingerprint.len().min(12)].to_string()
            }
            Self::Commit { oid } => oid[..oid.len().min(12)].to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitId(pub String);
impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone)]
pub struct AnalysisSnapshot {
    pub generation: SnapshotGeneration,
    pub identity: SnapshotIdentity,
    pub context: AnalysisContext,
    pub changes: Vec<FileChange>,
    pub units: Vec<ExplainedUnit>,
}

pub fn stable_unit_id(item: &ExplainedUnit) -> UnitId {
    let mut input = String::new();
    for value in [
        item.file.as_str(),
        &format!("{:?}", item.unit.kind),
        item.unit
            .qualified_name
            .as_deref()
            .unwrap_or(&item.unit.name),
        &item.unit.start_line.to_string(),
        &item.unit.end_line.to_string(),
        item.unit.source.as_str(),
    ] {
        input.push_str(&format!("{}:{}\n", value.len(), value));
    }
    let digest = sha2::Sha256::digest(input.as_bytes());
    UnitId(digest.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn working_tree_fingerprint(head: &str, diff_target: &str, changes: &[FileChange]) -> String {
    let mut input = format!("head={head}\ndiff_target={diff_target}\n");
    for change in changes {
        input.push_str(&format!(
            "path={}\nkind={:?}\nold={:?}\nranges={:?}\ndiff={}\n",
            change.path.display(),
            change.kind,
            change.old_path,
            change.ranges,
            change.diff
        ));
    }
    sha2::Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        explain::ExplainedUnit,
        language::{SourceUnit, SourceUnitKind},
        model::UnitExplanation,
    };
    fn unit(source: &str) -> ExplainedUnit {
        ExplainedUnit {
            id: UnitId("test".into()),
            file: "src/lib.rs".into(),
            language: "Rust".into(),
            diff: String::new(),
            regions: vec![],
            unit: SourceUnit {
                name: "f".into(),
                qualified_name: None,
                kind: SourceUnitKind::Function,
                start_line: 1,
                end_line: 1,
                source: source.into(),
            },
            explanation: UnitExplanation {
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: None,
        }
    }
    #[test]
    fn unit_id_is_deterministic_and_content_sensitive() {
        assert_eq!(stable_unit_id(&unit("a")), stable_unit_id(&unit("a")));
        assert_ne!(stable_unit_id(&unit("a")), stable_unit_id(&unit("b")));
    }

    #[test]
    fn working_tree_fingerprint_is_stable_and_changes_with_inputs() {
        let changes = vec![];
        assert_eq!(
            working_tree_fingerprint("head", "HEAD", &changes),
            working_tree_fingerprint("head", "HEAD", &changes)
        );
        assert_ne!(
            working_tree_fingerprint("head", "HEAD", &changes),
            working_tree_fingerprint("other-head", "HEAD", &changes)
        );
    }
}
