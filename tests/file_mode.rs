use std::{path::Path, process::Command};
use tempfile::tempdir;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn file_mode_extracts_a_working_tree_semantic_document_without_change_labels() {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    std::fs::create_dir_all(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("src/lib.rs"),
        "fn working_tree_unit() { current(); }\n",
    )
    .unwrap();
    let config = directory.path().join("user-config.toml");
    std::fs::write(&config, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["file", "--", "src/lib.rs"])
        .env("GIT_EXPLAIN_USER_CONFIG", config)
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Revision: working tree"));
    assert!(stdout.contains("Name: working_tree_unit"));
    assert!(!stdout.contains("Status: Added"));
    assert!(!stdout.contains("Semantic change"));
}

#[test]
fn file_mode_reads_the_selected_commit_not_the_working_tree() {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    git(directory.path(), &["config", "user.name", "Test"]);
    std::fs::write(directory.path().join("lib.rs"), "fn committed() {}\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    std::fs::write(directory.path().join("lib.rs"), "fn working() {}\n").unwrap();
    let config = directory.path().join("user-config.toml");
    std::fs::write(&config, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["file", "HEAD", "--", "lib.rs"])
        .env("GIT_EXPLAIN_USER_CONFIG", config)
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Name: committed"));
    assert!(!stdout.contains("Name: working"));
}
