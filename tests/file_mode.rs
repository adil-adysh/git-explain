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
        .args(["--debug", "file", "--", "src/lib.rs"])
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
        .args(["--debug", "file", "HEAD", "--", "lib.rs"])
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

#[test]
fn file_mode_expands_directories_in_the_selected_commit_tree() {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    git(directory.path(), &["config", "user.name", "Test"]);
    std::fs::create_dir_all(directory.path().join("src/nested")).unwrap();
    std::fs::write(directory.path().join("src/one.rs"), "fn one() {}\n").unwrap();
    std::fs::write(
        directory.path().join("src/nested/two.py"),
        "def two():\n    pass\n",
    )
    .unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    std::fs::remove_dir_all(directory.path().join("src")).unwrap();
    let config = directory.path().join("user-config.toml");
    std::fs::write(&config, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["--debug", "file", "HEAD", "--", "src"])
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
    assert!(stdout.contains("File: src\\one.rs") || stdout.contains("File: src/one.rs"));
    assert!(stdout.contains("two"));
    assert!(stdout.contains("Files: 2"));
}

#[test]
fn file_mode_expands_working_tree_directories_and_accepts_mixed_selectors() {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    std::fs::create_dir_all(directory.path().join("src/nested")).unwrap();
    std::fs::write(directory.path().join("src/one.rs"), "fn one() {}\n").unwrap();
    std::fs::write(
        directory.path().join("src/nested/two.py"),
        "def two():\n    pass\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("standalone.rs"),
        "fn standalone() {}\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("ignored.rs"), "fn ignored() {}\n").unwrap();
    let config = directory.path().join("user-config.toml");
    std::fs::write(&config, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["--debug", "file", "--", "src", "standalone.rs"])
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
    assert!(stdout.contains("one"));
    assert!(stdout.contains("two"));
    assert!(stdout.contains("standalone"));
    assert!(!stdout.contains("ignored"));
    assert!(stdout.contains("Files: 3"));
}
