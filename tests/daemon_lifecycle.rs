use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct Metadata {
    pid: u32,
    port: u16,
    control_token: String,
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_git-explain"))
}

fn cli(dir: &Path, port: u16, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("GIT_EXPLAIN_DAEMON_DIR", dir)
        .env("GIT_EXPLAIN_DAEMON_PORT", port.to_string())
        .env("GIT_EXPLAIN_USER_CONFIG", user_config_path(dir))
        .output()
        .unwrap()
}

fn spawn_daemon(dir: &Path) -> Child {
    write_user_config(dir);
    Command::new(binary())
        .args(["daemon", "run"])
        .env("GIT_EXPLAIN_DAEMON_DIR", dir)
        .env("GIT_EXPLAIN_DAEMON_PORT", "0")
        .env("GIT_EXPLAIN_USER_CONFIG", user_config_path(dir))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn user_config_path(dir: &Path) -> PathBuf {
    dir.join("user-config.toml")
}

fn write_user_config(dir: &Path) {
    fs::write(user_config_path(dir), "[model]\nprofile = \"test\"\n\n[profiles.test]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:1/v1\"\nmodel = \"test-model\"\n").unwrap();
}

async fn read_metadata(dir: &Path) -> Metadata {
    let path = dir.join("daemon.json");
    for _ in 0..100 {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(value) = serde_json::from_str(&text) {
                return value;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("daemon metadata did not appear: {}", path.display());
}

async fn wait_stopped(metadata: &Metadata) {
    let client = Client::new();
    for _ in 0..100 {
        if client
            .get(format!("http://127.0.0.1:{}/api/health", metadata.port))
            .send()
            .await
            .is_err()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("daemon did not stop");
}

fn git(dir: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success());
}

fn repository(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
    fs::write(dir.join("lib.rs"), "fn first() {}\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "base"]);
    fs::write(dir.join("lib.rs"), "fn first() { let changed = true; }\n").unwrap();
}

async fn open(client: &Client, metadata: &Metadata, root: &Path) -> serde_json::Value {
    client
        .post(format!("http://127.0.0.1:{}/api/repos/open", metadata.port))
        .header("x-git-explain-control-token", &metadata.control_token)
        .json(&json!({"repo_root": root, "revision": null, "profile": null}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn daemon_lifecycle_covers_invalid_open_replacement_and_shutdown() {
    let daemon_dir = tempdir().unwrap();
    let repo = tempdir().unwrap();
    repository(repo.path());
    let other_repo = tempdir().unwrap();
    repository(other_repo.path());
    fs::write(
        other_repo.path().join("lib.rs"),
        "fn second() { let changed = true; }\n",
    )
    .unwrap();
    let mut child = spawn_daemon(daemon_dir.path());
    let metadata = read_metadata(daemon_dir.path()).await;
    assert!(metadata.pid > 0);

    let client = Client::new();
    let invalid = client
        .post(format!("http://127.0.0.1:{}/api/repos/open", metadata.port))
        .header("x-git-explain-control-token", &metadata.control_token)
        .json(&json!({"repo_root": daemon_dir.path().join("missing"), "revision": null, "profile": null}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let first = open(&client, &metadata, repo.path()).await;
    assert!(first["ok"].as_bool().unwrap());
    let first_id = first["session_id"].as_str().unwrap().to_owned();
    let second = open(&client, &metadata, other_repo.path()).await;
    assert!(second["ok"].as_bool().unwrap());
    let second_id = second["session_id"].as_str().unwrap();
    assert_ne!(first_id, second_id);

    let old_page = client
        .get(format!(
            "http://127.0.0.1:{}/sessions/{}",
            metadata.port, first_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(old_page.status(), StatusCode::OK);
    let new_page = client
        .get(format!(
            "http://127.0.0.1:{}/sessions/{}",
            metadata.port, second_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(new_page.status(), StatusCode::OK);

    let first_snapshot: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/sessions/{}/snapshot",
            metadata.port, first_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let second_snapshot: serde_json::Value = client
        .get(format!(
            "http://127.0.0.1:{}/api/sessions/{}/snapshot",
            metadata.port, second_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(first_snapshot["identity"], second_snapshot["identity"]);

    let shutdown = client
        .post(format!(
            "http://127.0.0.1:{}/api/daemon/shutdown",
            metadata.port
        ))
        .header("x-git-explain-control-token", &metadata.control_token)
        .send()
        .await
        .unwrap();
    assert_eq!(shutdown.status(), StatusCode::OK);
    wait_stopped(&metadata).await;
    let _ = child.wait();
    assert!(!daemon_dir.path().join("daemon.json").exists());
}

#[tokio::test]
async fn daemon_start_is_idempotent_and_stale_metadata_is_removed() {
    let daemon_dir = tempdir().unwrap();
    let mut child = spawn_daemon(daemon_dir.path());
    let metadata = read_metadata(daemon_dir.path()).await;
    let start = cli(daemon_dir.path(), 0, &["daemon", "start"]);
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let output = String::from_utf8_lossy(&start.stdout);
    assert!(output.contains(&format!("pid={}", metadata.pid)));
    assert!(output.contains(&format!("url=http://127.0.0.1:{}", metadata.port)));

    let stop = cli(daemon_dir.path(), 0, &["daemon", "stop"]);
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    wait_stopped(&metadata).await;
    let _ = child.wait();

    let stale_dir = tempdir().unwrap();
    fs::write(
        stale_dir.path().join("daemon.json"),
        serde_json::to_vec(&json!({
            "pid": 999999,
            "port": 1,
            "started_at": "stale",
            "protocol_version": 1,
            "control_token": "stale-token"
        }))
        .unwrap(),
    )
    .unwrap();
    let status = cli(stale_dir.path(), 0, &["daemon", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("not running"));
    assert!(!stale_dir.path().join("daemon.json").exists());
}
