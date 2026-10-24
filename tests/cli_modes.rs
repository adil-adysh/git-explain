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
fn single_revision_cli_compares_that_revision_to_head() {
    let directory = repository();
    std::fs::write(directory.path().join("lib.rs"), "fn first() {}\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);
    std::fs::write(
        directory.path().join("lib.rs"),
        "fn first() {}\nfn second() {}\n",
    )
    .unwrap();
    git(directory.path(), &["commit", "-qam", "second"]);
    std::fs::write(
        directory.path().join("lib.rs"),
        "fn first() {}\nfn second() {}\nfn third() {}\n",
    )
    .unwrap();
    git(directory.path(), &["commit", "-qam", "third"]);

    let output = run(directory.path(), &["--debug", "diff", "HEAD~2"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Selected Git range: HEAD~2..HEAD"));
    assert!(stdout.contains("Name: second"));
    assert!(stdout.contains("Name: third"));
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

#[test]
fn invalid_revision_and_file_selection_use_input_error_exit_code() {
    let directory = repository();
    std::fs::write(directory.path().join("lib.rs"), "fn value() {}\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(directory.path(), &["commit", "-qm", "base"]);

    let invalid_revision = run(directory.path(), &["--debug", "diff", "does-not-exist"]);
    assert_eq!(invalid_revision.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_revision.stderr)
        .contains("requested Git revision could not be resolved"));

    let missing_file = run(directory.path(), &["--debug", "file", "--", "missing.rs"]);
    assert_eq!(missing_file.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_file.stderr)
        .contains("No files matched the selected file or directory paths"));

    let outside = run(
        directory.path(),
        &["--debug", "file", "--", "../outside.rs"],
    );
    assert_eq!(outside.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&outside.stderr).contains("escapes the Git worktree"));
}

#[test]
fn explanation_command_outside_repository_uses_input_error_exit_code() {
    let directory = tempdir().unwrap();
    let output = run(directory.path(), &["--debug", "diff"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("This command must be run inside a Git repository"));
}

#[cfg(windows)]
#[test]
fn normalize_selections_handles_windows_path_casing_and_absent_paths() {
    let repo_dir = tempdir().unwrap();
    let repo_path = repo_dir.path();
    let sub_dir = repo_path.join("sub");
    std::fs::create_dir_all(&sub_dir).unwrap();

    let existing_file = sub_dir.join("existing.rs");
    std::fs::write(&existing_file, "fn test() {}\n").unwrap();

    // Invocation path with uppercase drive/letters pointing to real directory
    let invocation_upper = std::path::PathBuf::from(sub_dir.to_string_lossy().to_ascii_uppercase());

    // Relative selection from invocation dir: "existing.rs"
    let normalized = git_explain::git::normalize_selections(
        repo_path,
        &invocation_upper,
        &[std::path::PathBuf::from("existing.rs")],
    )
    .expect("casing differences should normalize to real worktree");
    assert_eq!(
        normalized,
        vec![std::path::Path::new("sub").join("existing.rs")]
    );

    // Absolute existing file with different casing
    let existing_upper =
        std::path::PathBuf::from(existing_file.to_string_lossy().to_ascii_uppercase());
    let normalized_existing_abs =
        git_explain::git::normalize_selections(repo_path, repo_path, &[existing_upper])
            .expect("existing absolute path with different casing should normalize");
    assert_eq!(
        normalized_existing_abs,
        vec![std::path::Path::new("sub").join("existing.rs")]
    );

    // Absolute nonexisting / deleted file with different casing on existing ancestor (e.g. for revision mode)
    let nonexisting_abs =
        std::path::PathBuf::from(repo_path.to_string_lossy().to_ascii_uppercase())
            .join("sub")
            .join("deleted_revision_file.rs");
    let normalized_nonexisting_abs =
        git_explain::git::normalize_selections(repo_path, repo_path, &[nonexisting_abs])
            .expect("nonexisting absolute path in revision mode should normalize");
    assert_eq!(
        normalized_nonexisting_abs,
        vec![std::path::Path::new("sub").join("deleted_revision_file.rs")]
    );

    // Absolute nonexisting file where the nonexisting component itself is uppercase
    let nonexisting_file = sub_dir.join("deleted_revision_file.rs");
    let nonexisting_upper =
        std::path::PathBuf::from(nonexisting_file.to_string_lossy().to_ascii_uppercase());
    let normalized_nonexisting_upper =
        git_explain::git::normalize_selections(repo_path, repo_path, &[nonexisting_upper])
            .expect("nonexisting absolute path with uppercase tail should normalize");
    assert_eq!(
        normalized_nonexisting_upper,
        vec![std::path::Path::new("sub").join("DELETED_REVISION_FILE.RS")]
    );

    // Parent escape rejection
    let escape_err = git_explain::git::normalize_selections(
        repo_path,
        repo_path,
        &[std::path::PathBuf::from("../outside.rs")],
    );
    assert!(escape_err.is_err());
    assert!(escape_err
        .unwrap_err()
        .to_string()
        .contains("escapes the Git worktree"));
}
