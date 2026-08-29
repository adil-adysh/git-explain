use git_explain::{
    analyzer::RepositoryAnalyzer,
    change_set::{FileChangeKind, SemanticUnitChangeKind},
    config::ResolvedConfig,
    snapshot::SnapshotGeneration,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

fn git(dir: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap()
        .success());
}
fn git_output(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
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
    let html = git_explain::web::render(&changes);
    assert!(html.contains("Change summary"));
    assert!(html.contains("Previous path: old.rs"));
    assert!(html.contains("Semantic units preserved"));
    assert!(html.contains("No semantic changes require explanation."));
    assert!(!html.contains("<article data-unit-id="));
}

#[test]
fn path_filter_matches_both_names_of_a_rename_and_distinguishes_no_match() {
    let dir = repo();
    std::fs::write(dir.path().join("old.rs"), "fn keep() {}\n").unwrap();
    std::fs::write(dir.path().join("other.rs"), "fn other() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.rs", "new.rs"]);
    std::fs::write(dir.path().join("other.rs"), "fn other() { changed(); }\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "changes"]);
    let analyzer = RepositoryAnalyzer::new(dir.path(), config(dir.path()));

    for path in ["old.rs", "new.rs"] {
        let changes = analyzer
            .analyze_commit_with_paths("HEAD", SnapshotGeneration(1), &[PathBuf::from(path)])
            .unwrap();
        assert_eq!(changes.files.len(), 1);
        assert_eq!(changes.files[0].kind, FileChangeKind::Renamed);
    }

    let changes = analyzer
        .analyze_commit_with_paths(
            "HEAD",
            SnapshotGeneration(1),
            &[PathBuf::from("missing.rs")],
        )
        .unwrap();
    assert_eq!(
        changes.empty_state().as_deref(),
        Some("No changes matched the selected paths in the selected Git range.")
    );
}

#[test]
fn staged_selection_excludes_unstaged_edits() {
    let dir = repo();
    std::fs::write(dir.path().join("staged.rs"), "fn staged() {}\n").unwrap();
    std::fs::write(dir.path().join("unstaged.rs"), "fn unstaged() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("staged.rs"), "fn staged() { changed(); }\n").unwrap();
    git(dir.path(), &["add", "staged.rs"]);
    std::fs::write(
        dir.path().join("unstaged.rs"),
        "fn unstaged() { changed(); }\n",
    )
    .unwrap();

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_staged(SnapshotGeneration(1), &[])
        .unwrap();
    assert_eq!(changes.files.len(), 1);
    assert_eq!(changes.files[0].display_path(), Path::new("staged.rs"));
}

#[test]
fn working_tree_untracked_policy_is_respected() {
    let dir = repo();
    std::fs::write(dir.path().join(".gitignore"), "ignored.rs\nconfig.toml\n").unwrap();
    git(dir.path(), &["add", ".gitignore"]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("new.rs"), "fn introduced() {}\n").unwrap();
    std::fs::write(dir.path().join("ignored.rs"), "fn ignored() {}\n").unwrap();

    let disabled_config = config(dir.path());
    let disabled = RepositoryAnalyzer::new(dir.path(), disabled_config)
        .analyze_working_tree(SnapshotGeneration(1))
        .unwrap();
    assert!(disabled.files.is_empty());

    let mut enabled_config = config(dir.path());
    enabled_config.git.include_untracked = true;
    let enabled = RepositoryAnalyzer::new(dir.path(), enabled_config)
        .analyze_working_tree(SnapshotGeneration(1))
        .unwrap();
    assert_eq!(enabled.files.len(), 1);
    let file = &enabled.files[0];
    assert_eq!(file.kind, FileChangeKind::Added);
    assert_eq!(file.new_path.as_deref(), Some(Path::new("new.rs")));
    assert!(file
        .semantic_changes
        .iter()
        .any(|change| change.kind == SemanticUnitChangeKind::Added));
    assert_eq!(enabled.explainable_changes().count(), 1);
}

#[test]
fn explicit_range_uses_its_two_endpoints_not_the_working_tree() {
    let dir = repo();
    std::fs::write(dir.path().join("lib.rs"), "fn value() { one(); }\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    std::fs::write(dir.path().join("lib.rs"), "fn value() { two(); }\n").unwrap();
    git(dir.path(), &["commit", "-am", "two", "-q"]);
    std::fs::write(dir.path().join("lib.rs"), "fn value() { working(); }\n").unwrap();

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_revision_or_range("HEAD~1..HEAD", SnapshotGeneration(1), &[])
        .unwrap();
    let change = changes.explainable_changes().next().unwrap();
    assert!(change.before.as_ref().unwrap().source.contains("one()"));
    assert!(change.after.as_ref().unwrap().source.contains("two()"));
    assert!(!change.after.as_ref().unwrap().source.contains("working()"));
    assert!(matches!(
        changes.identity,
        git_explain::snapshot::SnapshotIdentity::Range { .. }
    ));
}

#[test]
fn rename_plus_edit_preserves_other_units() {
    let dir = repo();
    std::fs::write(
        dir.path().join("old.rs"),
        "fn changed() { let value = 1; }\nfn kept_one() {}\nfn kept_two() {}\nfn kept_three() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.rs", "new.rs"]);
    std::fs::write(
        dir.path().join("new.rs"),
        "fn changed() { let value = 2; }\nfn kept_one() {}\nfn kept_two() {}\nfn kept_three() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "rename and edit"]);
    let mut changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let file = &changes.files[0];
    assert_eq!(file.kind, FileChangeKind::Renamed);
    assert!(file
        .semantic_changes
        .iter()
        .any(|change| change.kind == SemanticUnitChangeKind::Modified));
    assert!(file
        .semantic_changes
        .iter()
        .any(|change| change.kind == SemanticUnitChangeKind::Preserved));
    assert_eq!(changes.explainable_changes().count(), 1);
    let html = git_explain::web::render_for_session_at_generation(&changes, "session", 1);
    assert!(html.contains("Previous path: old.rs"));
    assert!(html.contains("<article data-unit-id="));
    assert!(html.contains("Explain this code in depth"));
    assert!(html.contains("Read code as text"));
    assert!(html.contains("Show Git diff"));
    assert!(html.contains("Reload updated snapshot"));
    let debug = git_explain::explain::format_debug(&changes);
    assert!(debug.contains("Status: Renamed and modified"));
    assert!(debug.contains("Name: changed"));
    assert!(debug.contains("Lines: 1-1"));
    assert!(debug.contains("fn changed() { let value = 2; }"));

    let change = changes
        .files
        .iter_mut()
        .flat_map(|file| &mut file.semantic_changes)
        .find(|change| change.kind == SemanticUnitChangeKind::Modified)
        .unwrap();
    change.explanation.overview = "Cached overview".into();
    change.deep_explanation = Some("Cached detail".into());
    let cached_html = git_explain::web::render(&changes);
    assert!(cached_html.contains("Regenerate explanation"));
    assert!(cached_html.contains("Regenerate in-depth explanation"));
    assert!(cached_html.contains("Cached overview"));
    assert!(cached_html.contains("Cached detail"));
    assert!(cached_html.contains("deepButton.dataset.endpoint"));
}

#[test]
fn rename_with_changes_outside_semantic_units_is_not_reported_as_pure() {
    let dir = repo();
    std::fs::write(
        dir.path().join("old.rs"),
        "const BEFORE: usize = 1;\nfn preserved_one() {}\nfn preserved_two() {}\nfn preserved_three() {}\nfn preserved_four() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.rs", "new.rs"]);
    std::fs::write(
        dir.path().join("new.rs"),
        "const BEFORE: usize = 2;\nfn preserved_one() {}\nfn preserved_two() {}\nfn preserved_three() {}\nfn preserved_four() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "rename and edit metadata"]);

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let file = &changes.files[0];
    assert_eq!(file.kind, FileChangeKind::Renamed);
    assert_eq!(file.status_label(), "Renamed and modified");
    assert_eq!(changes.explainable_changes().count(), 0);
    assert_eq!(
        changes.empty_state().as_deref(),
        Some("Source content changed, but no explainable semantic units were found.")
    );
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

#[test]
fn modification_and_line_movement_keep_semantic_identity() {
    let dir = repo();
    std::fs::write(
        dir.path().join("lib.rs"),
        "fn keep() { let value = 1; }\nfn move_me() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(
        dir.path().join("lib.rs"),
        "// inserted\n// inserted again\nfn keep() { let value = 2; }\nfn move_me() {}\n",
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "modify"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let file = &changes.files[0];
    assert!(file.semantic_changes.iter().any(|change| change.kind
        == SemanticUnitChangeKind::Modified
        && change.before.as_ref().unwrap().identity == change.after.as_ref().unwrap().identity));
    assert!(file
        .semantic_changes
        .iter()
        .any(|change| change.kind == SemanticUnitChangeKind::Preserved
            && change.after.as_ref().unwrap().identity.qualified_name == "move_me"));
}

#[test]
fn binary_and_mode_only_changes_are_deterministic_and_not_explainable() {
    let dir = repo();
    std::fs::write(dir.path().join("asset.bin"), [0_u8, 1, 2, 3]).unwrap();
    std::fs::write(dir.path().join("script.sh"), "#!/bin/sh\necho hi\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("asset.bin"), [0_u8, 8, 9, 10]).unwrap();
    git(dir.path(), &["update-index", "--chmod=+x", "script.sh"]);
    git(dir.path(), &["add", "asset.bin"]);
    git(dir.path(), &["commit", "-qm", "metadata"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert!(changes
        .files
        .iter()
        .any(|file| file.binary && file.semantic_changes.is_empty()));
    assert!(changes
        .files
        .iter()
        .any(|file| file.mode_changed() && file.semantic_changes.is_empty()));
    assert_eq!(changes.explainable_changes().count(), 0);
    assert!(changes
        .empty_state()
        .unwrap()
        .contains("Binary content has no semantic source explanation"));
}

#[test]
fn type_changes_are_deterministic_and_not_semantically_parsed() {
    let dir = repo();
    std::fs::write(dir.path().join("typed.rs"), "fn before() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("link-target"), "target.rs\n").unwrap();
    let blob = git_output(dir.path(), &["hash-object", "-w", "link-target"]);
    let cache_info = format!("120000,{blob},typed.rs");
    git(dir.path(), &["update-index", "--cacheinfo", &cache_info]);
    git(dir.path(), &["commit", "-qm", "change type"]);

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert_eq!(changes.files.len(), 1);
    let file = &changes.files[0];
    assert_eq!(file.kind, FileChangeKind::TypeChanged);
    assert!(file.semantic_changes.is_empty());
    assert_eq!(changes.explainable_changes().count(), 0);
    assert_eq!(
        changes.empty_state().as_deref(),
        Some("The type changed for 1 file. No semantic source explanation is available.")
    );
}

#[test]
fn mode_and_semantic_content_changes_are_both_preserved() {
    let dir = repo();
    std::fs::write(dir.path().join("script.rs"), "fn run() { before(); }\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("script.rs"), "fn run() { after(); }\n").unwrap();
    git(dir.path(), &["add", "script.rs"]);
    git(dir.path(), &["update-index", "--chmod=+x", "script.rs"]);
    git(dir.path(), &["commit", "-qm", "content and mode"]);

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let file = &changes.files[0];
    assert!(file.mode_changed());
    assert!(file
        .semantic_changes
        .iter()
        .any(|change| change.kind == SemanticUnitChangeKind::Modified));
    assert_eq!(changes.explainable_changes().count(), 1);
    assert!(changes.empty_state().is_none());
    let debug = git_explain::explain::format_debug(&changes);
    assert!(debug.contains("Mode: 100644 -> 100755"));
    assert!(debug.contains("Status: Modified"));
    assert!(debug.contains("Name: run"));
}

#[test]
fn exact_copy_and_weird_unicode_paths_stay_file_level_metadata() {
    let dir = repo();
    std::fs::write(dir.path().join("source.rs"), "fn copied() {}\n").unwrap();
    std::fs::write(dir.path().join("sp ace-λ.rs"), "fn original() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::copy(dir.path().join("source.rs"), dir.path().join("copy.rs")).unwrap();
    std::fs::write(dir.path().join("sp ace-λ.rs"), "fn changed() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "copy"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let copy = changes
        .files
        .iter()
        .find(|file| file.new_path.as_deref() == Some(Path::new("copy.rs")))
        .unwrap();
    assert_eq!(copy.kind, FileChangeKind::Copied);
    assert_eq!(copy.similarity, Some(100));
    assert!(copy
        .semantic_changes
        .iter()
        .all(|change| change.kind == SemanticUnitChangeKind::Preserved));
    assert!(changes.empty_state().is_none());
    assert!(changes
        .files
        .iter()
        .any(|file| file.new_path.as_deref() == Some(Path::new("sp ace-λ.rs"))));
}

#[test]
fn exact_copy_only_has_a_deterministic_empty_state() {
    let dir = repo();
    std::fs::write(dir.path().join("source.rs"), "fn copied() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::copy(dir.path().join("source.rs"), dir.path().join("copy.rs")).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "copy"]);

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert_eq!(changes.files.len(), 1);
    assert_eq!(changes.files[0].kind, FileChangeKind::Copied);
    assert_eq!(changes.explainable_changes().count(), 0);
    assert_eq!(
        changes.empty_state().as_deref(),
        Some(
            "1 file was copied without source-content changes. No semantic changes require explanation."
        )
    );
}

#[test]
fn unsupported_and_unitless_source_changes_have_distinct_empty_states() {
    let dir = repo();
    std::fs::write(dir.path().join("notes.txt"), "before\n").unwrap();
    std::fs::write(dir.path().join("constants.rs"), "\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);

    std::fs::write(dir.path().join("notes.txt"), "after\n").unwrap();
    git(dir.path(), &["commit", "-qam", "unsupported"]);
    let analyzer = RepositoryAnalyzer::new(dir.path(), config(dir.path()));
    let unsupported = analyzer
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert_eq!(
        unsupported.empty_state().as_deref(),
        Some("Source files changed, but their languages are not currently supported.")
    );

    std::fs::write(dir.path().join("constants.rs"), "\n\n").unwrap();
    git(dir.path(), &["commit", "-qam", "unitless"]);
    let unitless = analyzer
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    assert_eq!(
        unitless.empty_state().as_deref(),
        Some("Source content changed, but no explainable semantic units were found.")
    );
}

#[test]
fn added_unsupported_file_is_not_mislabeled_as_a_mode_change() {
    let dir = repo();
    std::fs::write(dir.path().join("base.rs"), "fn base() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    std::fs::write(dir.path().join("notes.txt"), "new notes\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "add notes"]);

    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let file = &changes.files[0];
    assert_eq!(file.kind, FileChangeKind::Added);
    assert!(!file.mode_changed());
    assert_eq!(
        changes.empty_state().as_deref(),
        Some("Source files changed, but their languages are not currently supported.")
    );
}

#[test]
fn json_is_nested_by_file_and_semantic_change() {
    let dir = repo();
    std::fs::write(dir.path().join("old.rs"), "fn keep() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["mv", "old.rs", "new.rs"]);
    git(dir.path(), &["commit", "-qm", "rename"]);
    let changes = RepositoryAnalyzer::new(dir.path(), config(dir.path()))
        .analyze_commit("HEAD", SnapshotGeneration(1))
        .unwrap();
    let value = changes.json();
    let file = &value["files"][0];
    assert_eq!(file["kind"], "renamed");
    assert_eq!(file["old_path"], "old.rs");
    assert_eq!(file["new_path"], "new.rs");
    assert!(file["semantic_changes"].is_array());
    assert!(value.get("units").is_none());
}
