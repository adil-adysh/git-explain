//! The new workspace reads immutable, selected source in all serving modes.
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
async fn exercise(mode: &str) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Workspace Test"]);
    git(root, &["config", "user.email", "workspace@example.test"]);
    fs::write(
        root.join("lib.rs"),
        "fn changed() -> u32 { 1 }\nfn unchanged() {}\nfn deleted() {}\n",
    )
    .unwrap();
    fs::write(root.join("move.rs"), "fn moved() -> bool { true }\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(
        root.join("lib.rs"),
        "fn changed() -> u32 { 2 }\nfn unchanged() {}\n",
    )
    .unwrap();
    git(root, &["mv", "move.rs", "renamed.rs"]);
    fs::write(root.join("notes.txt"), "UNSUPPORTED_PRIVATE_TEXT").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config_path = root.join("user.toml");
    let config = format!("# Keep user notes\n[model]\nprofile='local'\n[profiles.local]\nprovider='openai_compatible'\nbase_url='http://127.0.0.1:1/v1'\nmodel='unused'\n[server]\nport={port}\nopen_browser=false\n[cache]\nenabled=false\n");
    fs::write(&config_path, &config).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_git-explain"));
    command
        .current_dir(root)
        .env("GIT_EXPLAIN_USER_CONFIG", &config_path)
        .env("GIT_EXPLAIN_DAEMON_DIR", root.join("daemon"))
        .env("GIT_EXPLAIN_DAEMON_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match mode {
        "daemon" => {
            command.args(["daemon", "run"]);
        }
        "file" => {
            command.args(["--direct", "file", "--", "lib.rs"]);
        }
        _ => {
            command.args(["--direct", "diff"]);
        }
    }
    let mut process = Running(command.spawn().unwrap());
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let origin = format!("http://127.0.0.1:{port}");
    let probe = if mode == "daemon" {
        "/api/health"
    } else {
        "/api/changes"
    };
    let mut ready = false;
    for _ in 0..100 {
        if client.get(format!("{origin}{probe}")).send().await.is_ok() {
            ready = true;
            break;
        }
        assert!(process.0.try_wait().unwrap().is_none(), "{mode} exited");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready, "{mode} did not start");
    let mut daemon_control = None;
    let prefix = if mode == "daemon" {
        let metadata: Value =
            serde_json::from_slice(&fs::read(root.join("daemon/daemon.json")).unwrap()).unwrap();
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
        daemon_control = Some(metadata["control_token"].as_str().unwrap().to_owned());
        assert_eq!(opened["ok"], true, "{opened}");
        format!(
            "{origin}/api/sessions/{}/ui/v1",
            opened["session_id"].as_str().unwrap()
        )
    } else {
        format!("{origin}/api/ui/v1")
    };
    let workspace_url = if mode == "daemon" {
        prefix
            .replace("/api/sessions/", "/sessions/")
            .replace("/ui/v1", "/workspace")
    } else {
        format!("{origin}/workspace")
    };
    let default_url = if mode == "daemon" {
        workspace_url.trim_end_matches("/workspace").to_owned()
    } else {
        format!("{origin}/")
    };
    let default_shell = client
        .get(&default_url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        default_shell.contains("Accessible Workspace"),
        "{mode}: default route must open the new UI"
    );
    let fallback = client
        .get(format!("{}/legacy", default_url.trim_end_matches('/')))
        .send()
        .await
        .unwrap();
    assert_eq!(fallback.status(), StatusCode::OK);
    let shell = client.get(&workspace_url).send().await.unwrap();
    assert_eq!(
        shell.status(),
        StatusCode::OK,
        "{mode}: missing bundled workspace shell"
    );
    assert!(shell.headers().contains_key("content-security-policy"));
    let shell = shell.text().await.unwrap();
    assert!(shell.contains("/ui-assets/workspace.js"));
    assert!(!shell.contains("fn changed"));
    assert!(!shell.contains("data-unit-id"));
    assert!(client
        .get(format!("{origin}/ui-assets/workspace.js"))
        .send()
        .await
        .unwrap()
        .status()
        .is_success());
    let missing = client
        .get(format!("{origin}/api/ui/v1/not-an-api"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let response = client.get(format!("{prefix}/index")).send().await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{mode}: missing metadata API"
    );
    let index: Value = response.json().await.unwrap();
    assert_eq!(index["schema_version"], 1);
    assert_eq!(index["runtime"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(index["runtime"]["model"], "unused");
    assert!(index["runtime"].get("api_key").is_none());
    assert!(index["runtime"].get("base_url").is_none());
    let files = index["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        if mode == "file" { 1 } else { 2 },
        "unsupported files must be excluded: {index}"
    );
    let file = files
        .iter()
        .find(|file| file["path_label"] == "lib.rs")
        .unwrap();
    if mode != "file" {
        let renamed = files
            .iter()
            .find(|file| file["path_label"] == "renamed.rs")
            .unwrap();
        assert_eq!(renamed["status"], "renamed");
        assert_eq!(renamed["old_path_label"], "move.rs");
        let moved = &renamed["sections"][0];
        assert_eq!(moved["name"], "moved");
        assert_eq!(moved["source_side"], "after");
        let source: Value = client
            .get(format!(
                "{prefix}/sections/{}",
                moved["id"].as_str().unwrap()
            ))
            .query(&[("snapshot_id", index["snapshot_id"].as_str().unwrap())])
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(source["source"], "fn moved() -> bool { true }");
        assert_eq!(source["file_id"], renamed["id"]);
    }
    let sections = file["sections"].as_array().unwrap();
    assert_eq!(sections.len(), if mode == "file" { 2 } else { 3 });
    assert!(sections
        .iter()
        .any(|section| section["name"] == "unchanged"));
    let encoded = index.to_string();
    assert!(!encoded.contains("fn changed"));
    assert!(!encoded.contains("UNSUPPORTED_PRIVATE_TEXT"));
    assert!(sections
        .iter()
        .all(|section| section.get("source").is_none()));
    let snapshot = index["snapshot_id"].as_str().unwrap();
    let section = sections
        .iter()
        .find(|section| section["name"] == "changed")
        .unwrap();
    let id = section["id"].as_str().unwrap();
    let detail: Value = client
        .get(format!("{prefix}/sections/{id}"))
        .query(&[("snapshot_id", snapshot)])
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["snapshot_id"], snapshot);
    assert_eq!(detail["section_id"], id);
    assert_eq!(detail["source"], "fn changed() -> u32 { 2 }");
    assert_eq!(
        client
            .get(format!("{prefix}/sections/{id}"))
            .query(&[("snapshot_id", "obsolete")])
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .get(format!("{prefix}/sections/not-a-section"))
            .query(&[("snapshot_id", snapshot)])
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{prefix}/sections/{id}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    if mode != "file" {
        let deleted = sections
            .iter()
            .find(|section| section["name"] == "deleted")
            .unwrap();
        assert_eq!(deleted["source_side"], "before");
        let detail: Value = client
            .get(format!(
                "{prefix}/sections/{}",
                deleted["id"].as_str().unwrap()
            ))
            .query(&[("snapshot_id", snapshot)])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(detail["source"], "fn deleted() {}");
    }
    if let Some(control) = daemon_control {
        let second: Value = client
            .post(format!("{origin}/api/repos/open"))
            .header("x-git-explain-control-token", &control)
            .json(&json!({"repo_root":root,"profile":"local"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(second["ok"], true);
        let second_prefix = format!(
            "{origin}/api/sessions/{}/ui/v1",
            second["session_id"].as_str().unwrap()
        );
        let second_index: Value = client
            .get(format!("{second_prefix}/index"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_ne!(index["snapshot_id"], second_index["snapshot_id"]);
        assert_eq!(
            client
                .get(format!("{second_prefix}/sections/{id}"))
                .query(&[("snapshot_id", second_index["snapshot_id"].as_str().unwrap())])
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        fs::write(
            root.join("lib.rs"),
            "fn changed() -> u32 { 99 }\nfn unchanged() {}\n",
        )
        .unwrap();
        let refreshed: Value = client
            .post(format!("{origin}/api/repos/refresh"))
            .header("x-git-explain-control-token", &control)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(refreshed["ok"], true);
        assert_eq!(
            client
                .get(format!(
                    "{second_prefix}/sections/{}",
                    second_index["files"][0]["sections"][0]["id"]
                        .as_str()
                        .unwrap()
                ))
                .query(&[("snapshot_id", second_index["snapshot_id"].as_str().unwrap())])
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            client
                .get(format!("{origin}/api/sessions/expired/ui/v1/index"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::GONE
        );
    }
    assert_eq!(
        fs::read_to_string(config_path).unwrap(),
        config,
        "GET must not write settings"
    );
}

#[tokio::test]
async fn workspace_file_reads_only_selected_source() {
    exercise("file").await;
}
#[tokio::test]
async fn workspace_diff_includes_unchanged_and_deleted_sections() {
    exercise("diff").await;
}
#[tokio::test]
async fn workspace_daemon_uses_session_scoped_source() {
    exercise("daemon").await;
}
