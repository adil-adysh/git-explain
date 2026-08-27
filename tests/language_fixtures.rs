use git_explain::{
    diff::{FileChange, LineRange},
    explain::symbols,
    language::LanguageRegistry,
};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn registry_handles_mixed_rust_python_and_go_changes() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(
        directory.path().join("src/lib.rs"),
        "fn rust_work() {\n    changed();\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/api.py"),
        "async def python_work():\n    changed()\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("src/service.go"),
        "package service\n\nfunc GoWork() {\n    changed()\n}\n",
    )
    .unwrap();

    let cases = [
        ("src/lib.rs", "rust_work", "Rust"),
        ("src/api.py", "python_work", "Python"),
        ("src/service.go", "GoWork", "Go"),
    ];
    for (path, name, language) in cases {
        let path_buf = std::path::PathBuf::from(path);
        assert!(LanguageRegistry::analyzer_for_path(&path_buf).is_some());
        assert_eq!(
            LanguageRegistry::language_for_path(&path_buf),
            Some(language)
        );
        let changed_line = if language == "Go" { 3 } else { 2 };
        let change = FileChange {
            path: path_buf,
            old_path: None,
            kind: git_explain::diff::ChangeKind::Modified,
            ranges: vec![LineRange {
                start: changed_line,
                end: changed_line,
            }],
            diff: String::new(),
        };
        let found = symbols(directory.path(), &change).unwrap();
        assert_eq!(found.len(), 1, "{path}");
        assert_eq!(found[0].name, name);
    }
}

#[test]
fn unsupported_files_are_ignored_without_parsing_errors() {
    let path = std::path::PathBuf::from("README.md");
    assert!(LanguageRegistry::analyzer_for_path(&path).is_none());
    assert!(LanguageRegistry::language_for_path(&path).is_none());
}

#[test]
fn git_explain_debug_discovers_mixed_language_changes_in_one_run() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::create_dir_all(directory.path().join("tools")).unwrap();
    fs::create_dir_all(directory.path().join("internal")).unwrap();
    fs::write(
        directory.path().join("src/auth.rs"),
        "fn authenticate() {\n    old_rust();\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("tools/process.py"),
        "def process():\n    old_python()\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("internal/cache.go"),
        "package internal\n\nfunc Refresh() {\n    oldGo()\n}\n",
    )
    .unwrap();
    run_git(directory.path(), &["init"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-m", "initial"]);

    fs::write(
        directory.path().join("src/auth.rs"),
        "fn authenticate() {\n    new_rust();\n}\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("tools/process.py"),
        "def process():\n    new_python()\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("internal/cache.go"),
        "package internal\n\nfunc Refresh() {\n    newGo()\n}\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .arg("--debug")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("authenticate"));
    assert!(stdout.contains("process"));
    assert!(stdout.contains("Refresh"));
    assert!(stdout.contains("new_rust"));
    assert!(stdout.contains("new_python"));
    assert!(stdout.contains("newGo"));
}

#[test]
fn debug_clean_tree_is_a_successful_no_op() {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init", "-q"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::write(directory.path().join("README.md"), "initial\n").unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-qm", "initial"]);

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .arg("--debug")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Working tree is clean. Nothing to explain."
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn debug_unsupported_only_changes_are_a_successful_no_op() {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init", "-q"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::write(directory.path().join("README.md"), "initial\n").unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-qm", "initial"]);
    fs::write(directory.path().join("README.md"), "updated\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .arg("--debug")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Changes exist, but none are in supported source files."
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_reports_repository_errors_with_a_user_facing_exit_code() {
    let directory = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .arg("--debug")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must be run inside a Git repository"));
    assert!(!stderr.starts_with("Error:"));
}

#[test]
fn cli_reports_invalid_revision_with_a_user_facing_exit_code() {
    let directory = tempdir().unwrap();
    run_git(directory.path(), &["init", "-q"]);
    run_git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    run_git(directory.path(), &["config", "user.name", "Test"]);
    fs::write(directory.path().join("README.md"), "initial\n").unwrap();
    run_git(directory.path(), &["add", "."]);
    run_git(directory.path(), &["commit", "-qm", "initial"]);

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["missing-revision", "--debug"])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not be resolved"));
}

#[test]
fn unreadable_or_malformed_file_does_not_block_valid_file() {
    let directory = tempdir().unwrap();
    fs::write(directory.path().join("broken.py"), [0xff, 0xfe, 0xfd]).unwrap();
    fs::write(
        directory.path().join("valid.go"),
        "package main\n\nfunc Serve() {\n    ready()\n}\n",
    )
    .unwrap();
    let broken = symbols(
        directory.path(),
        &FileChange {
            path: "broken.py".into(),
            old_path: None,
            kind: git_explain::diff::ChangeKind::Modified,
            ranges: vec![LineRange { start: 1, end: 1 }],
            diff: String::new(),
        },
    )
    .unwrap();
    let valid = symbols(
        directory.path(),
        &FileChange {
            path: "valid.go".into(),
            old_path: None,
            kind: git_explain::diff::ChangeKind::Modified,
            ranges: vec![LineRange { start: 4, end: 4 }],
            diff: String::new(),
        },
    )
    .unwrap();
    assert!(broken.is_empty());
    assert_eq!(valid[0].name, "Serve");
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
