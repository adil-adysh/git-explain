use git_explain::{
    analyzer::RepositoryAnalyzer,
    change_set::{FileChangeKind, SemanticUnitChangeKind},
    config::ResolvedConfig,
    snapshot::SnapshotGeneration,
};
use std::{path::Path, process::Command};
use tempfile::tempdir;

fn git(dir: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap()
        .success());
}
fn config(dir: &Path) -> ResolvedConfig {
    let path = dir.join("config.toml");
    std::fs::write(&path, "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8080/v1\"\nmodel = \"local\"").unwrap();
    git_explain::config::ConfigLoader::with_paths(path, None)
        .resolve(Some("local"))
        .unwrap()
}
fn repo() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    dir
}
#[test]
fn pure_rename_is_a_file_change_with_preserved_semantics() {
    let dir = repo();
    std::fs::write(
        dir.path().join("old.rs"),
        "fn keep() {}\nfn also_keep() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.rs", "new.rs"]);
    git(dir.path(), &["commit", "-qm", "rename"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert_eq!(changes.files.len(), 1);
    let file = &changes.files[0];
    assert_eq!(file.kind, FileChangeKind::Renamed);
    assert_eq!(file.similarity, Some(100));
    assert_eq!(file.old_path.as_ref().unwrap(), Path::new("old.rs"));
    assert_eq!(file.new_path.as_ref().unwrap(), Path::new("new.rs"));
    assert!(file
        .semantic_changes
        .iter()
        .all(|change| change.kind == SemanticUnitChangeKind::Preserved));
    assert_eq!(changes.explainable_changes().count(), 0);
}
#[test]
fn added_and_deleted_source_units_are_first_class() {
    let dir = repo();
    std::fs::write(dir.path().join("old.rs"), "fn gone() {}\nfn other() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["rm", "old.rs"]);
    std::fs::write(
        dir.path().join("new.rs"),
        "fn added() {}\nfn another() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "change"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert!(changes
        .files
        .iter()
        .any(|file| file.kind == FileChangeKind::Deleted
            && file
                .semantic_changes
                .iter()
                .all(|change| change.kind == SemanticUnitChangeKind::Deleted)));
    assert!(changes
        .files
        .iter()
        .any(|file| file.kind == FileChangeKind::Added
            && file
                .semantic_changes
                .iter()
                .all(|change| change.kind == SemanticUnitChangeKind::Added)));
}
