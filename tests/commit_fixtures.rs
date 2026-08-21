use std::fs;
use std::process::Command;
use tempfile::{tempdir, TempDir};

fn repo_with_history() -> (TempDir, String, String, String) {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/value.rs"),
        "fn value() {\n    return 1;\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "root source"]);
    let root = rev(directory.path(), "HEAD");

    fs::write(
        directory.path().join("src/value.rs"),
        "fn value() {\n    return 2;\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["commit", "-am", "change to two"]);
    let middle = rev(directory.path(), "HEAD");

    fs::write(
        directory.path().join("src/value.rs"),
        "fn value() {\n    return 3;\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["commit", "-am", "change to three"]);
    let head = rev(directory.path(), "HEAD");
    (directory, root, middle, head)
}

fn run_explain(directory: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(args)
        .arg("--debug")
        .current_dir(directory)
        .output()
        .unwrap()
}

fn run_git(directory: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rev(directory: &std::path::Path, name: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", name])
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn resolves_full_short_head_and_rejects_invalid_revision() {
    let (directory, _root, middle, head) = repo_with_history();
    let full = run_explain(directory.path(), &[&middle]);
    assert!(full.status.success());
    assert!(String::from_utf8_lossy(&full.stdout).contains("Commit:"));

    let short_revision = &middle[..7];
    let short = run_explain(directory.path(), &[short_revision]);
    assert!(short.status.success());
    assert!(String::from_utf8_lossy(&short.stdout).contains("return 2;"));

    let head_output = run_explain(directory.path(), &["HEAD"]);
    assert!(head_output.status.success());
    assert!(String::from_utf8_lossy(&head_output.stdout).contains(&head));

    let invalid = run_explain(directory.path(), &["does-not-exist"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("Unable to resolve Git revision"));
}

#[test]
fn analyzes_historical_source_from_selected_commit_not_current_head() {
    let (directory, _root, middle, _head) = repo_with_history();
    let output = run_explain(directory.path(), &[&middle]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("return 2;"));
    assert!(!stdout.contains("return 3;"));
    assert!(stdout.contains("Subject: change to two"));
}

#[test]
fn analyzes_root_commit_without_parent() {
    let (directory, root, _middle, _head) = repo_with_history();
    let output = run_explain(directory.path(), &[&root]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Parent: <empty tree>"));
    assert!(stdout.contains("return 1;"));
}

#[test]
fn analyzes_multiple_languages_and_deleted_files_in_commit_mode() {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/auth.rs"),
        "fn auth() {\n    old();\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/worker.py"),
        "def worker():\n    old()\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/cache.go"),
        "package cache\n\nfunc Refresh() {\n    old()\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/old.rs"),
        "fn old_code() {\n    old();\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "base"]);
    fs::write(
        directory.path().join("src/auth.rs"),
        "fn auth() {\n    new_rust();\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/worker.py"),
        "def worker():\n    new_python()\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/cache.go"),
        "package cache\n\nfunc Refresh() {\n    new_go()\n}\n",
    )
    .unwrap();
    fs::remove_file(directory.path().join("src/old.rs")).unwrap();
    run_git(directory.path(), &["add", "-A"]);
    run_git(directory.path(), &["commit", "-m", "mixed change"]);

    let output = run_explain(directory.path(), &["HEAD"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth"));
    assert!(stdout.contains("worker"));
    assert!(stdout.contains("Refresh"));
    assert!(stdout.contains("new_rust"));
    assert!(stdout.contains("new_python"));
    assert!(stdout.contains("new_go"));
    assert!(stdout.contains("Deleted file: src/old.rs"));
}

#[test]
fn merge_commit_uses_and_documents_first_parent() {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::write(directory.path().join("main.rs"), "fn base() {}\n").unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "base"]);
    run_git(directory.path(), &["checkout", "-b", "feature"]);
    fs::write(
        directory.path().join("feature.rs"),
        "fn feature() {\n    changed();\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "feature"]);
    run_git(directory.path(), &["checkout", "-b", "mainline", "HEAD~1"]);
    fs::write(
        directory.path().join("mainline.rs"),
        "fn mainline() {\n    changed();\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "mainline"]);
    run_git(
        directory.path(),
        &["merge", "--no-ff", "feature", "-m", "merge"],
    );

    let output = run_explain(directory.path(), &["HEAD"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Merge commit detected. Showing changes relative to first parent."));
    assert!(stdout.contains("feature"));
}
