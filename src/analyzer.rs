use crate::{
    config::{GitConfig, ResolvedConfig},
    diff::ChangeKind,
    explain::{
        analysis_items, AnalysisContext, AnalysisMode, GitCommitSourceProvider,
        WorkingTreeSourceProvider,
    },
    git,
    snapshot::{
        stable_unit_id, working_tree_fingerprint, AnalysisSnapshot, SnapshotGeneration,
        SnapshotIdentity,
    },
};
use anyhow::Result;
use std::path::PathBuf;

#[derive(Clone)]
pub struct RepositoryAnalyzer {
    repo_root: PathBuf,
    git_config: GitConfig,
}
impl RepositoryAnalyzer {
    pub fn new(repo_root: impl Into<PathBuf>, config: ResolvedConfig) -> Self {
        Self {
            repo_root: repo_root.into(),
            git_config: config.git,
        }
    }
    pub fn with_git_config(repo_root: impl Into<PathBuf>, git_config: GitConfig) -> Self {
        Self {
            repo_root: repo_root.into(),
            git_config,
        }
    }
    pub fn analyze_working_tree(&self, generation: SnapshotGeneration) -> Result<AnalysisSnapshot> {
        let changes = git::working_tree_changes(&self.repo_root, &self.git_config)?;
        let provider = WorkingTreeSourceProvider::new(&self.repo_root);
        let mut context = AnalysisContext::working_tree();
        let mut units = analysis_items(&provider, &changes)?;
        assign_ids(&mut units);
        context.no_op = no_op_message(&context, &changes, &units);
        let head = git::head_oid(&self.repo_root).unwrap_or_default();
        let fingerprint = working_tree_fingerprint(&head, &self.git_config.diff_target, &changes);
        Ok(AnalysisSnapshot {
            generation,
            identity: SnapshotIdentity::WorkingTree { fingerprint },
            context,
            changes,
            units,
        })
    }
    pub fn analyze_commit(
        &self,
        revision: &str,
        generation: SnapshotGeneration,
    ) -> Result<AnalysisSnapshot> {
        let analysis = git::commit_analysis(&self.repo_root, revision)?;
        let mut context = AnalysisContext {
            mode: AnalysisMode::Commit {
                oid: analysis.oid.clone(),
                parent_oid: analysis.parent_oid.clone(),
                subject: analysis.subject.clone(),
                merge_parent_count: analysis.parent_count,
            },
            deleted_files: analysis
                .changes
                .iter()
                .filter(|c| c.kind == ChangeKind::Deleted)
                .map(|c| c.path.display().to_string())
                .collect(),
            no_op: None,
        };
        let provider = GitCommitSourceProvider::new(&self.repo_root, analysis.oid.clone());
        let mut units = analysis_items(&provider, &analysis.changes)?;
        assign_ids(&mut units);
        context.no_op = no_op_message(&context, &analysis.changes, &units);
        Ok(AnalysisSnapshot {
            generation,
            identity: SnapshotIdentity::Commit { oid: analysis.oid },
            context,
            changes: analysis.changes,
            units,
        })
    }
}

fn no_op_message(
    context: &AnalysisContext,
    changes: &[crate::diff::FileChange],
    units: &[crate::explain::ExplainedUnit],
) -> Option<String> {
    if !units.is_empty() || !context.deleted_files.is_empty() {
        return None;
    }
    if changes.is_empty() {
        return Some(
            match context.mode {
                AnalysisMode::WorkingTree => "Working tree is clean. Nothing to explain.",
                AnalysisMode::Commit { .. } => "Commit contains no file changes to explain.",
            }
            .into(),
        );
    }
    let supported = changes
        .iter()
        .any(|change| crate::language::LanguageRegistry::analyzer_for_path(&change.path).is_some());
    Some(
        if supported {
            "Supported files changed, but no changed code units were detected."
        } else {
            "Changes exist, but none are in supported source files."
        }
        .into(),
    )
}
fn assign_ids(units: &mut [crate::explain::ExplainedUnit]) {
    for unit in units {
        unit.id = stable_unit_id(unit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, process::Command};
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }

    fn config(dir: &Path) -> ResolvedConfig {
        let path = dir.join("config.toml");
        std::fs::write(&path, "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"").unwrap();
        crate::config::ConfigLoader::with_paths(path, None)
            .resolve(Some("local"))
            .unwrap()
    }

    #[test]
    fn analyzer_retains_working_tree_changes_and_context() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "struct Config { value: i32 }\n",
        )
        .unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "base"]);
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "struct Config { value: i64 }\n",
        )
        .unwrap();
        let snapshot = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
            .analyze_working_tree(SnapshotGeneration(1))
            .unwrap();
        assert!(matches!(snapshot.context.mode, AnalysisMode::WorkingTree));
        assert!(!snapshot.changes.is_empty());
        assert!(snapshot.units.iter().all(|unit| !unit.id.0.is_empty()));
        assert_eq!(snapshot.generation, SnapshotGeneration(1));
    }

    #[test]
    fn analyzer_commit_identity_uses_resolved_oid() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("lib.rs"), "fn first() {}\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "first"]);
        let oid = String::from_utf8(
            Command::new("git")
                .current_dir(dir.path())
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        std::fs::write(dir.path().join("lib.rs"), "fn first() { let value = 1; }\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "second"]);
        let snapshot = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
            .analyze_commit("HEAD", SnapshotGeneration(1))
            .unwrap();
        assert!(matches!(snapshot.identity, SnapshotIdentity::Commit { .. }));
        assert_ne!(
            oid,
            match snapshot.identity {
                SnapshotIdentity::Commit { oid } => oid,
                _ => unreachable!(),
            }
        );
    }
}
