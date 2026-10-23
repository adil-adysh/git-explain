//! Real-process checks for learning settings on every transport.
use fs2::FileExt;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

async fn exercise(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Learning Test"]);
    git(root, &["config", "user.email", "learning@example.test"]);
    fs::write(root.join("lib.rs"), "fn read_value() -> u32 { 1 }\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("lib.rs"), "fn read_value() -> u32 { 2 }\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = root.join("user.toml");
    fs::write(&config,format!("[model]\nprofile='local'\n[profiles.local]\nprovider='openai_compatible'\nbase_url='http://127.0.0.1:1/v1'\nmodel='unused'\n[server]\nport={port}\nopen_browser=false\n[cache]\nenabled=false\n")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-explain"));
    command
        .current_dir(root)
        .env("GIT_EXPLAIN_USER_CONFIG", &config)
        .env("GIT_EXPLAIN_DAEMON_DIR", root.join("daemon"))
        .env("GIT_EXPLAIN_DAEMON_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if mode == "daemon" {
        command.args(["daemon", "run"]);
    } else if mode == "file" {
        command.args(["--direct", "file", "--", "lib.rs"]);
    } else {
        command.args(["--direct", "diff"]);
    }
    let mut process = Running(command.spawn().unwrap());
    let origin = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let probe = if mode == "daemon" { "/api/health" } else { "/" };
    let mut ready = false;
    for _ in 0..100 {
        if client.get(format!("{origin}{probe}")).send().await.is_ok() {
            ready = true;
            break;
        }
        if let Some(status) = process.0.try_wait().unwrap() {
            panic!("{mode} exited {status}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "{mode} did not start");
    let mut session = None;
    let endpoint = if mode == "daemon" {
        let metadata: Value =
            serde_json::from_str(&fs::read_to_string(root.join("daemon/daemon.json")).unwrap())
                .unwrap();
        let opened: Value = client
            .post(format!("{origin}/api/repos/open"))
            .header(
                "x-git-explain-control-token",
                metadata["control_token"].as_str().unwrap(),
            )
            .json(&json!({"repo_root":root,"profile":"local"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(opened["ok"], true, "{opened}");
        let id = opened["session_id"].as_str().unwrap().to_owned();
        session = Some(id.clone());
        format!("{origin}/api/sessions/{id}/learning")
    } else {
        format!("{origin}/api/learning")
    };
    let initial: Value = client
        .get(&endpoint)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(initial["ok"], true, "{initial}");
    assert!(initial["config"].get("profiles").is_some());
    assert!(initial.get("model").is_none());
    let token = initial["token"].as_str().unwrap();
    let mut config_value = initial["config"].clone();
    config_value["defaults"]["detail"] = json!("detailed");
    let payload = json!({"revision":initial["revision"],"config":config_value});
    let denied = client
        .put(&endpoint)
        .header("origin", &origin)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let denied = client
        .put(&endpoint)
        .header("origin", "http://other.test")
        .header("x-git-explain-learning-token", token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    // A real concurrent writer is a retryable conflict in every transport.
    let original_config = fs::read(&config).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(config.with_extension("learning.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let busy = client
        .put(&endpoint)
        .header("origin", &origin)
        .header("x-git-explain-learning-token", token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(busy.status(), StatusCode::CONFLICT, "{mode}");
    let busy_body: Value = busy.json().await.unwrap();
    assert_eq!(
        busy_body["error"],
        "learning preferences are being edited; retry after the other save completes"
    );
    assert_eq!(fs::read(&config).unwrap(), original_config);
    drop(lock);
    let mut invalid_payload = payload.clone();
    invalid_payload["config"]["profile"] = json!("changed");
    let invalid = client
        .put(&endpoint)
        .header("origin", &origin)
        .header("x-git-explain-learning-token", token)
        .json(&invalid_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST, "{mode}");
    assert_eq!(fs::read(&config).unwrap(), original_config);
    let saved = client
        .put(&endpoint)
        .header("origin", &origin)
        .header("x-git-explain-learning-token", token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK, "{mode}");
    let stale = client
        .put(&endpoint)
        .header("origin", &origin)
        .header("x-git-explain-learning-token", token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let resolved: Value = client
        .post(format!("{endpoint}/resolve"))
        .json(&json!({"language":"Rust","selection":{"overrides":{"focus":"reasoning"}}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resolved["policy"]["detail"], "detailed");
    assert_eq!(resolved["policy"]["focus"], "reasoning");
    assert_eq!(resolved["provenance"]["focus"], "current view");
    assert!(fs::read_to_string(&config).unwrap().contains("unused"));
    if let Some(id) = session {
        let metadata: Value =
            serde_json::from_str(&fs::read_to_string(root.join("daemon/daemon.json")).unwrap())
                .unwrap();
        let refreshed: Value = client
            .post(format!("{origin}/api/repos/refresh"))
            .header(
                "x-git-explain-control-token",
                metadata["control_token"].as_str().unwrap(),
            )
            .json(&json!({"repo_root":root}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(refreshed.get("ok").is_some(), "{id}");
    }
}
#[tokio::test]
async fn file_learning_settings_are_protected_and_persistent() {
    exercise("file").await;
}
#[tokio::test]
async fn direct_learning_settings_are_protected_and_persistent() {
    exercise("diff").await;
}
#[tokio::test]
async fn daemon_learning_settings_are_protected_and_persistent() {
    exercise("daemon").await;
}
