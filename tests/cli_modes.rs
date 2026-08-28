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

fn repository() -> tempfile::TempDir {
    let directory = tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    git(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    );
    git(directory.path(), &["config", "user.name", "Test"]);
    directory
}

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    let config = root.join("user-config.toml");
    if !config.exists() {
        std::fs::write(&config, "").unwrap();
    }
    Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(args)
        .env("GIT_EXPLAIN_USER_CONFIG", config)
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn diff_range_cli_uses_explicit_endpoints_and_path_filter() {
    let directory = repository();
    std::fs::write(
        directory.path().join("selected.rs"),
        "fn selected() { one(); }\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("other.rs"), "fn other() { one(); }\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "one"]);
    std::fs::write(
        directory.path().join("selected.rs"),
        "fn selected() { two(); }\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("other.rs"), "fn other() { two(); }\n").unwrap();
    git(directory.path(), &["commit", "-qam", "two"]);
    std::fs::write(directory.path().join("selected.rs"), "fn working() {}\n").unwrap();

    let output = run(
        directory.path(),
        &["diff", "HEAD~1..HEAD", "--debug", "--", "selected.rs"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Selected Git range: HEAD~1..HEAD"));
    assert!(stdout.contains("Name: selected"));
    assert!(!stdout.contains("Name: other"));
    assert!(!stdout.contains("Name: working"));
}

#[test]
fn staged_cli_excludes_unstaged_files() {
    let directory = repository();
    std::fs::write(directory.path().join("staged.rs"), "fn staged() {}\n").unwrap();
    std::fs::write(directory.path().join("unstaged.rs"), "fn unstaged() {}\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    std::fs::write(
        directory.path().join("staged.rs"),
        "fn staged() { changed(); }\n",
    )
    .unwrap();
    git(directory.path(), &["add", "staged.rs"]);
    std::fs::write(
        directory.path().join("unstaged.rs"),
        "fn unstaged() { changed(); }\n",
    )
    .unwrap();

    let output = run(directory.path(), &["diff", "--staged", "--debug"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Name: staged"));
    assert!(!stdout.contains("Name: unstaged"));
}

#[test]
fn direct_diff_accepts_explicit_ranges() {
    let directory = repository();
    std::fs::write(directory.path().join("lib.rs"), "fn value() { one(); }\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "one"]);
    std::fs::write(directory.path().join("lib.rs"), "fn value() { two(); }\n").unwrap();
    git(directory.path(), &["commit", "-qam", "two"]);
    std::fs::write(
        directory.path().join("user-config.toml"),
        "[model]\nprofile = \"local\"\n\n[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8080/v1\"\nmodel = \"test\"\n",
    )
    .unwrap();

    // A non-matching filter makes direct mode return after deterministic
    // analysis instead of starting its one-shot HTTP server.
    let output = run(
        directory.path(),
        &["--direct", "diff", "HEAD~1..HEAD", "--", "missing.rs"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("No changes matched the selected paths in the selected Git range."));
}

#[test]
fn help_describes_current_semantic_modes_and_transport() {
    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Print deterministic semantic analysis"));
    assert!(stdout.contains("file mode is always one-shot"));
    assert!(!stdout.contains("changed supported-language functions"));
}

#[test]
fn diff_paths_are_relative_to_a_nested_invocation_directory() {
    let directory = repository();
    std::fs::create_dir_all(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("lib.rs"),
        "fn root_file() { one(); }\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("src/lib.rs"),
        "fn nested_file() { one(); }\n",
    )
    .unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    std::fs::write(
        directory.path().join("lib.rs"),
        "fn root_file() { two(); }\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("src/lib.rs"),
        "fn nested_file() { two(); }\n",
    )
    .unwrap();
    let config = directory.path().join("user-config.toml");
    std::fs::write(&config, "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(["--debug", "diff", "--", "lib.rs"])
        .env("GIT_EXPLAIN_USER_CONFIG", config)
        .current_dir(directory.path().join("src"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Name: nested_file"));
    assert!(!stdout.contains("Name: root_file"));
}
