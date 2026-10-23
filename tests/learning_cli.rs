use git_explain::learning::LearningStore;
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
fn run(path: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_git-explain"))
        .args(args)
        .env("GIT_EXPLAIN_USER_CONFIG", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn learning_commands_save_only_explicitly_confirmed_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("user.toml");
    std::fs::write(&path, "[model]\nprofile='local'\n").unwrap();
    success(run(
        &path,
        &["learning", "add", "rust"],
        "explain\nexecution\ndetailed\nn\ny\n",
    ));
    let store = LearningStore::new(path.clone());
    let saved = store.inspect().unwrap();
    assert!(saved.config.profiles.contains_key("rust"));
    success(run(&path, &["learning", "use", "rust"], ""));
    assert_eq!(
        store.inspect().unwrap().config.profile.as_deref(),
        Some("rust")
    );
    assert!(success(run(&path, &["learning", "list"], "")).contains("rust (default)"));
    assert!(success(run(&path, &["learning", "show", "rust"], "")).contains("execution"));
    let before = store.inspect().unwrap().revision;
    success(run(
        &path,
        &["learning", "edit", "rust"],
        "skip\nreasoning\nconcise\nn\nn\n",
    ));
    assert_eq!(store.inspect().unwrap().revision, before);
    success(run(
        &path,
        &["learning", "edit", "rust"],
        "brief\nsyntax\nbalanced\nn\ny\n",
    ));
    assert!(success(run(&path, &["learning", "show", "rust"], "")).contains("syntax"));
    assert!(!run(&path, &["learning", "remove", "rust"], "")
        .status
        .success());
    success(run(&path, &["learning", "add", "other"], "\n\n\nn\ny\n"));
    success(run(&path, &["learning", "use", "other"], ""));
    success(run(&path, &["learning", "remove", "rust"], ""));
    assert!(!store
        .inspect()
        .unwrap()
        .config
        .profiles
        .contains_key("rust"));
    assert!(std::fs::read_to_string(path).unwrap().contains("local"));
}
#[test]
fn learning_editor_preserves_unspecified_familiarity_and_supports_explicit_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("user.toml");
    success(run(&path,&["learning","add","learner"],"automatic\nexecution\nbalanced\ny\nPython, JavaScript\nRust\n\nTokio\nHTTP\nGit\nMutex\nAwait\ny\n"));
    let store = LearningStore::new(path.clone());
    let initial = store.inspect().unwrap();
    let profile = &initial.config.profiles["learner"];
    assert_eq!(
        profile.preferences.known_languages.as_ref().unwrap(),
        &vec!["Python".to_string(), "JavaScript".to_string()]
    );
    success(run(
        &path,
        &["learning", "edit", "learner"],
        "automatic\nexecution\nbalanced\ny\n\n\n\n\n-\n\n\n\ny\n",
    ));
    let edited = store.inspect().unwrap();
    let profile = &edited.config.profiles["learner"];
    assert_eq!(
        profile.preferences.known_languages,
        initial.config.profiles["learner"]
            .preferences
            .known_languages
    );
    assert_eq!(profile.preferences.known_domains, Some(vec![]));
    assert!(profile.preferences.known_frameworks.is_none());
}
#[test]
fn invalid_learning_commands_leave_saved_preferences_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("user.toml");
    let store = LearningStore::new(path.clone());
    let initial = store.inspect().unwrap().revision;
    for args in [
        ["learning", "use", "missing"],
        ["learning", "edit", "missing"],
        ["learning", "remove", "missing"],
    ] {
        assert!(!run(&path, &args, "").status.success());
        assert_eq!(store.inspect().unwrap().revision, initial);
    }
    assert!(!run(
        &path,
        &["learning", "add", "invalid"],
        "expert\nsyntax\nconcise\n"
    )
    .status
    .success());
    assert_eq!(store.inspect().unwrap().revision, initial);
}
