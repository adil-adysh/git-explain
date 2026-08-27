use crate::{
    analyzer::RepositoryAnalyzer,
    cache::ExplanationCache,
    cli::DaemonAction,
    config::{ConfigLoader, ExplanationConfig, ModelConfig, ReaderConfig},
    explain::ExplainedUnit,
    model::{
        user_facing_error, ExplanationProvider, ExplanationRequest, UnitExplanation,
        UserFacingError,
    },
    runtime,
    snapshot::{AnalysisSnapshot, SnapshotGeneration, UnitId},
    web,
};
use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path as FsPath, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::AtomicU64,
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, RwLock, Semaphore};

pub const DAEMON_PROTOCOL_VERSION: u32 = 1;
const DEFAULT_PORT: u16 = 8192;
const MAX_INFERENCE_REQUESTS: usize = 2;
const MAX_SESSIONS: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Metadata {
    pid: u32,
    port: u16,
    started_at: String,
    protocol_version: u32,
    control_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositorySessionId(pub String);

struct RepositorySession {
    id: RepositorySessionId,
    repo_root: PathBuf,
    git_dir: PathBuf,
    analyzer: RepositoryAnalyzer,
    revision: Option<String>,
    config: crate::config::ResolvedConfig,
    snapshot: AnalysisSnapshot,
    items: Mutex<HashMap<UnitId, ExplainedUnit>>,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ModelConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
    cancellation: Arc<SessionCancellation>,
    in_flight: Mutex<HashMap<String, Arc<InFlight>>>,
    last_used: AtomicU64,
}

struct DaemonState {
    port: u16,
    sessions: RwLock<HashMap<String, Arc<RepositorySession>>>,
    active_session: RwLock<Option<RepositorySessionId>>,
    sequence: AtomicU64,
    control_token: String,
    shutdown: Notify,
    inference_slots: Arc<Semaphore>,
}

struct SessionCancellation {
    cancelled: AtomicBool,
    notify: Notify,
}

impl SessionCancellation {
    fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
enum InferenceOutcome {
    Success(UnitExplanation),
    Cancelled,
    Failed(UserFacingError),
}

struct InFlight {
    result: Mutex<Option<InferenceOutcome>>,
    notify: Notify,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRequest {
    repo_root: String,
    revision: Option<String>,
    profile: Option<String>,
}
#[derive(Debug, Deserialize)]
struct SnapshotRequest {
    generation: Option<SnapshotGeneration>,
}

pub async fn command(action: &DaemonAction) -> Result<()> {
    match action {
        DaemonAction::Start => start(daemon_port()).await,
        DaemonAction::Stop => stop().await,
        DaemonAction::Status => status().await,
        DaemonAction::Refresh => refresh_repository().await,
        DaemonAction::Run => run(daemon_port()).await,
    }
}

pub async fn open_repository(
    repo: &FsPath,
    revision: Option<&str>,
    profile: Option<&str>,
    port: Option<u16>,
) -> Result<()> {
    let metadata = ensure_running(port.unwrap_or(daemon_port())).await?;
    let token = metadata.control_token.clone();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/api/repos/open", metadata.port))
        .header("x-git-explain-control-token", token)
        .json(&OpenRequest {
            repo_root: repo.to_string_lossy().into_owned(),
            revision: revision.map(str::to_owned),
            profile: profile.map(str::to_owned),
        })
        .send()
        .await
        .context("register repository with git-explain daemon")?;
    let value: serde_json::Value = response
        .json()
        .await
        .context("read daemon repository response")?;
    if !value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        anyhow::bail!(
            "daemon could not open repository: {}",
            value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
        );
    }
    let url = value["url"].as_str().unwrap_or_default();
    println!("git explain: {url}");
    let _ = webbrowser::open(url);
    Ok(())
}

async fn start(port: u16) -> Result<()> {
    let metadata = ensure_running(port).await?;
    println!(
        "git-explain daemon running: pid={} url=http://127.0.0.1:{}",
        metadata.pid, metadata.port
    );
    Ok(())
}

async fn ensure_running(port: u16) -> Result<Metadata> {
    if let Some(metadata) = discover().await? {
        return Ok(metadata);
    }
    fs::create_dir_all(daemon_dir()?)?;
    let lock = lock_path()?;
    let acquired = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .is_ok();
    if acquired {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args(["daemon", "run"])
            .env("GIT_EXPLAIN_DAEMON_PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(log_path()?)?));
        let mut child = command.spawn().context("start git-explain daemon")?;
        let _ = child.id();
        for _ in 0..50 {
            if let Some(metadata) = discover().await? {
                let _ = fs::remove_file(&lock);
                return Ok(metadata);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = child.kill();
        let _ = fs::remove_file(&lock);
        anyhow::bail!("daemon started but did not become healthy")
    } else {
        for _ in 0..50 {
            if let Some(metadata) = discover().await? {
                return Ok(metadata);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = fs::remove_file(&lock);
        Box::pin(ensure_running(port)).await
    }
}

async fn discover() -> Result<Option<Metadata>> {
    let path = metadata_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let metadata: Metadata = match serde_json::from_str(&fs::read_to_string(&path)?) {
        Ok(value) => value,
        Err(_) => {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
    };
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/api/health", metadata.port))
        .send()
        .await;
    let healthy = match response {
        Ok(response) => futures_health(response).await.unwrap_or(false),
        Err(_) => false,
    };
    if healthy && metadata.protocol_version == DAEMON_PROTOCOL_VERSION {
        Ok(Some(metadata))
    } else {
        let _ = fs::remove_file(path);
        Ok(None)
    }
}

async fn futures_health(response: reqwest::Response) -> Option<bool> {
    let value: serde_json::Value = response.json().await.ok()?;
    Some(
        value["ok"].as_bool() == Some(true)
            && value["protocol_version"].as_u64() == Some(DAEMON_PROTOCOL_VERSION as u64),
    )
}

async fn status() -> Result<()> {
    if let Some(metadata) = discover().await? {
        println!(
            "running\npid: {}\nurl: http://127.0.0.1:{}\nprotocol: {}",
            metadata.pid, metadata.port, metadata.protocol_version
        );
    } else {
        println!("not running");
    }
    Ok(())
}

async fn stop() -> Result<()> {
    let Some(metadata) = (if metadata_path()?.exists() {
        serde_json::from_str::<Metadata>(&fs::read_to_string(metadata_path()?)?).ok()
    } else {
        None
    }) else {
        println!("daemon not running");
        return Ok(());
    };
    let response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/api/daemon/shutdown",
            metadata.port
        ))
        .header("x-git-explain-control-token", metadata.control_token)
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => println!("daemon stopped"),
        _ => {
            let _ = fs::remove_file(metadata_path()?);
            println!("daemon metadata was stale; cleaned it up");
        }
    }
    Ok(())
}

async fn refresh_repository() -> Result<()> {
    let metadata = discover()
        .await?
        .context("git-explain daemon is not running")?;
    let response = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/api/repos/refresh",
            metadata.port
        ))
        .header("x-git-explain-control-token", metadata.control_token)
        .send()
        .await
        .context("refresh repository session")?;
    let value: serde_json::Value = response.json().await?;
    if !value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        anyhow::bail!(
            "daemon could not refresh repository: {}",
            value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
        );
    }
    let changed = value
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    println!(
        "git-explain: {} session {} at generation {}",
        if changed {
            "refreshed"
        } else {
            "snapshot unchanged for"
        },
        value["session_id"].as_str().unwrap_or_default(),
        value["generation"].as_u64().unwrap_or_default()
    );
    Ok(())
}

async fn run(port: u16) -> Result<()> {
    let port = std::env::var("GIT_EXPLAIN_DAEMON_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(port);
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token = token_bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .context("bind loopback daemon port")?;
    let actual_port = listener.local_addr()?.port();
    let state = Arc::new(DaemonState {
        port: actual_port,
        sessions: RwLock::new(HashMap::new()),
        active_session: RwLock::new(None),
        sequence: AtomicU64::new(0),
        control_token: token.clone(),
        shutdown: Notify::new(),
        inference_slots: Arc::new(Semaphore::new(MAX_INFERENCE_REQUESTS)),
    });
    let metadata = Metadata {
        pid: std::process::id(),
        port: actual_port,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
        protocol_version: DAEMON_PROTOCOL_VERSION,
        control_token: token,
    };
    write_metadata(&metadata)?;
    let _ = fs::remove_file(lock_path()?);
    println!("git-explain daemon listening on http://127.0.0.1:{actual_port}");
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/daemon/shutdown", post(shutdown))
        .route("/api/repos/open", post(open))
        .route("/api/repos/refresh", post(refresh))
        .route("/api/sessions/{session}/snapshot", get(snapshot_status))
        .route("/sessions/{id}", get(session_page))
        .route("/api/sessions/{session}/units/{id}/explain", post(explain))
        .route("/api/sessions/{session}/units/{id}/deep", post(deep))
        .route(
            "/api/sessions/{session}/units/{id}/regenerate",
            post(regenerate),
        )
        .route(
            "/api/sessions/{session}/units/{id}/deep/regenerate",
            post(deep_regenerate),
        )
        .with_state(state.clone());
    let shutdown_state = state.clone();
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown_state.shutdown.notified().await })
        .await;
    let _ = fs::remove_file(metadata_path()?);
    result.context("daemon server")?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"protocol_version":DAEMON_PROTOCOL_VERSION,"version":env!("CARGO_PKG_VERSION")}),
    )
}
fn authorized(headers: &HeaderMap, state: &DaemonState) -> bool {
    headers
        .get("x-git-explain-control-token")
        .and_then(|v| v.to_str().ok())
        == Some(state.control_token.as_str())
}
async fn shutdown(
    headers: HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !authorized(&headers, &state) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"ok":false,"error":"control authentication required"})),
        );
    }
    for session in state.sessions.read().await.values() {
        session.cancellation.cancel();
    }
    state.shutdown.notify_one();
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

async fn open(
    headers: HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(request): Json<OpenRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !authorized(&headers, &state) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"ok":false,"error":"control authentication required"})),
        );
    }
    let root = match fs::canonicalize(&request.repo_root) {
        Ok(root) => root,
        Err(error) => {
            return error_json(
                StatusCode::BAD_REQUEST,
                format!("invalid repository path: {error}"),
            )
        }
    };
    let loader = match ConfigLoader::for_repository(Some(&root)) {
        Ok(loader) => loader,
        Err(error) => return error_json(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let config = match loader.resolve(request.profile.as_deref()) {
        Ok(config) => config,
        Err(error) => return error_json(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let id = RepositorySessionId(new_id());
    let session = match build_session(id, root, request.revision, config, SnapshotGeneration(1)) {
        Ok(session) => session,
        Err(error) => return error_json(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let response = serde_json::json!({"ok":true,"session_id":session.id.0,"generation":session.snapshot.generation.0,"url":format!("http://127.0.0.1:{}/sessions/{}", state.port, session.id.0)});
    register_session(&state, session).await;
    (StatusCode::OK, Json(response))
}

async fn register_session(state: &DaemonState, session: Arc<RepositorySession>) {
    session.last_used.store(
        state.sequence.fetch_add(1, Ordering::AcqRel),
        Ordering::Release,
    );
    let evicted = {
        let mut sessions = state.sessions.write().await;
        let evicted = if sessions.len() >= MAX_SESSIONS {
            sessions
                .values()
                .min_by_key(|candidate| candidate.last_used.load(Ordering::Acquire))
                .map(|candidate| candidate.id.0.clone())
                .and_then(|id| sessions.remove(&id))
        } else {
            None
        };
        sessions.insert(session.id.0.clone(), session.clone());
        evicted
    };
    if let Some(evicted) = evicted {
        evicted.cancellation.cancel();
    }
    *state.active_session.write().await = Some(session.id.clone());
}

async fn refresh(
    headers: HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !authorized(&headers, &state) {
        return error_json(
            StatusCode::UNAUTHORIZED,
            "control authentication required".into(),
        );
    }
    let refreshed = match refresh_current_session(&state).await {
        Ok(refreshed) => refreshed,
        Err(error) if error.to_string() == "No repository session is open" => {
            return error_json(StatusCode::NOT_FOUND, error.to_string())
        }
        Err(error) => return error_json(StatusCode::BAD_REQUEST, error.to_string()),
    };
    let replacement = refreshed.session;
    let response = serde_json::json!({
        "ok": true,
        "changed": refreshed.changed,
        "session_id": replacement.id.0,
        "generation": replacement.snapshot.generation.0,
        "identity": replacement.snapshot.identity,
        "url": format!("http://127.0.0.1:{}/sessions/{}", state.port, replacement.id.0)
    });
    (StatusCode::OK, Json(response))
}

struct RefreshResult {
    session: Arc<RepositorySession>,
    changed: bool,
}

async fn refresh_current_session(state: &DaemonState) -> Result<RefreshResult> {
    let current_session = active_session(state)
        .await
        .context("No repository session is open")?;
    let generation = SnapshotGeneration(
        current_session
            .snapshot
            .generation
            .0
            .checked_add(1)
            .context("Snapshot generation overflow")?,
    );
    let replacement = build_session_from_analyzer(
        current_session.id.clone(),
        current_session.repo_root.clone(),
        current_session.revision.clone(),
        current_session.config.clone(),
        generation,
        current_session.analyzer.clone(),
    )?;
    let mut sessions = state.sessions.write().await;
    let active = sessions
        .get(&current_session.id.0)
        .context("No repository session is open")?;
    if !Arc::ptr_eq(active, &current_session) {
        anyhow::bail!("Repository session changed while refresh was running");
    }
    if replacement.snapshot.identity == current_session.snapshot.identity {
        return Ok(RefreshResult {
            session: current_session,
            changed: false,
        });
    }
    current_session.cancellation.cancel();
    sessions.insert(replacement.id.0.clone(), replacement.clone());
    Ok(RefreshResult {
        session: replacement,
        changed: true,
    })
}

fn build_session(
    id: RepositorySessionId,
    root: PathBuf,
    revision: Option<String>,
    config: crate::config::ResolvedConfig,
    generation: SnapshotGeneration,
) -> Result<Arc<RepositorySession>> {
    let analyzer = RepositoryAnalyzer::new(&root, config.clone());
    build_session_from_analyzer(id, root, revision, config, generation, analyzer)
}

fn build_session_from_analyzer(
    id: RepositorySessionId,
    root: PathBuf,
    revision: Option<String>,
    config: crate::config::ResolvedConfig,
    generation: SnapshotGeneration,
    analyzer: RepositoryAnalyzer,
) -> Result<Arc<RepositorySession>> {
    let snapshot = if let Some(revision) = revision.as_deref() {
        analyzer.analyze_commit(revision, generation)?
    } else {
        analyzer.analyze_working_tree(generation)?
    };
    if snapshot.changes.is_empty() {
        anyhow::bail!("No supported changes found for analysis");
    }
    let git_dir = crate::git::git_dir(&root)?;
    let cache = if config.cache.enabled {
        Some(ExplanationCache::open(&git_dir)?)
    } else {
        None
    };
    let provider = crate::model::openai::OpenAiProvider::from_config(
        config.model.clone(),
        config.reader.clone(),
        config.explanation.clone(),
    );
    let mut items: HashMap<UnitId, ExplainedUnit> = snapshot
        .units
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    if let Some(cache_ref) = &cache {
        runtime::hydrate(
            &mut items,
            cache_ref,
            &snapshot,
            &config.model,
            &config.reader,
            &config.explanation,
        );
    }
    Ok(Arc::new(RepositorySession {
        id,
        repo_root: root,
        git_dir,
        analyzer,
        revision,
        config: config.clone(),
        snapshot,
        items: Mutex::new(items),
        provider: Arc::new(provider),
        cache,
        model: config.model,
        reader: config.reader,
        explanation: config.explanation,
        cancellation: Arc::new(SessionCancellation::new()),
        in_flight: Mutex::new(HashMap::new()),
        last_used: AtomicU64::new(0),
    }))
}
fn error_json(status: StatusCode, error: String) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"ok":false,"error":error})))
}
fn new_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn session_page(
    Path(id): Path<String>,
    State(state): State<Arc<DaemonState>>,
) -> Result<Html<String>, StatusCode> {
    let session = current(&state, &id).await.ok_or(StatusCode::NOT_FOUND)?;
    let items = session.items.lock().unwrap();
    let ordered: Vec<_> = session
        .snapshot
        .units
        .iter()
        .filter_map(|unit| items.get(&unit.id))
        .cloned()
        .collect();
    Ok(Html(web::render_for_session_at_generation(
        &ordered,
        &session.snapshot.context,
        &id,
        session.snapshot.generation.0,
    )))
}
async fn snapshot_status(
    Path(session_id): Path<String>,
    State(state): State<Arc<DaemonState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session = current(&state, &session_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "session_id": session.id.0,
        "generation": session.snapshot.generation.0,
        "identity": session.snapshot.identity,
    })))
}
async fn current(state: &DaemonState, id: &str) -> Option<Arc<RepositorySession>> {
    let session = state.sessions.read().await.get(id).cloned()?;
    session.last_used.store(
        state.sequence.fetch_add(1, Ordering::AcqRel),
        Ordering::Release,
    );
    Some(session)
}

async fn active_session(state: &DaemonState) -> Option<Arc<RepositorySession>> {
    let id = state.active_session.read().await.clone()?;
    current(state, &id.0).await
}
async fn explain(
    Path((session, id)): Path<(String, String)>,
    State(state): State<Arc<DaemonState>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(session, id, false, false, body, state).await
}
async fn deep(
    Path((session, id)): Path<(String, String)>,
    State(state): State<Arc<DaemonState>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(session, id, true, false, body, state).await
}
async fn regenerate(
    Path((session, id)): Path<(String, String)>,
    State(state): State<Arc<DaemonState>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(session, id, false, true, body, state).await
}
async fn deep_regenerate(
    Path((session, id)): Path<(String, String)>,
    State(state): State<Arc<DaemonState>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(session, id, true, true, body, state).await
}
async fn generate(
    session_id: String,
    id: String,
    deep: bool,
    regenerate: bool,
    body: Option<Json<SnapshotRequest>>,
    state: Arc<DaemonState>,
) -> Json<serde_json::Value> {
    let Some(session) = current(&state, &session_id).await else {
        return Json(
            serde_json::json!({"ok":false,"stale":true,"code":"session_stale","error":"This repository session is no longer active. Reload the page and try again.","retryable":true}),
        );
    };
    if body
        .and_then(|Json(body)| body.generation)
        .is_some_and(|generation| generation != session.snapshot.generation)
    {
        return Json(
            serde_json::json!({"ok":false,"stale":true,"code":"snapshot_stale","error":"The repository changed while this page was open. Reload the page and try again.","retryable":true}),
        );
    }
    let unit_id = UnitId(id);
    let item = { session.items.lock().unwrap().get(&unit_id).cloned() };
    let Some(item) = item else {
        return Json(
            serde_json::json!({"ok":false,"code":"unit_not_found","error":"This code unit is no longer available. Reload the page and try again.","retryable":false}),
        );
    };
    if !crate::language::contains_meaningful_source(&item.unit.source) {
        return Json(
            serde_json::json!({"ok":false,"code":"no_source","error":"This code unit has no meaningful source to explain.","retryable":false}),
        );
    }
    let request = runtime::request_for(&item, &session.snapshot.context, deep);
    let key = ExplanationCache::key(
        &request,
        &session.model,
        &session.reader,
        &session.explanation,
    );
    if !regenerate {
        if let Some(cache) = &session.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(e) = serde_json::from_str::<UnitExplanation>(&json) {
                    if update_if_current(&state, &session, &unit_id, e.clone(), deep).await {
                        return Json(runtime::result(e, true, deep));
                    }
                    return stale_session();
                }
            }
        }
    }
    match infer_with_dedup(&state, &session, key.clone(), request.clone()).await {
        InferenceOutcome::Success(e) => {
            if current(&state, &session.id.0).await.is_none() {
                return Json(
                    serde_json::json!({"ok":false,"stale":true,"code":"session_stale","error":"This repository session is no longer active. Reload the page and try again.","retryable":true}),
                );
            }
            if let Some(cache) = &session.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &session.model,
                    &serde_json::to_string(&e).unwrap(),
                );
            }
            if update_if_current(&state, &session, &unit_id, e.clone(), deep).await {
                Json(runtime::result(e, false, deep))
            } else {
                stale_session()
            }
        }
        InferenceOutcome::Cancelled => stale_session(),
        InferenceOutcome::Failed(info) => Json(
            serde_json::json!({"ok":false,"code":info.code,"error":info.message,"retryable":info.retryable}),
        ),
    }
}

async fn infer_with_dedup(
    state: &DaemonState,
    session: &Arc<RepositorySession>,
    key: String,
    request: ExplanationRequest,
) -> InferenceOutcome {
    let (flight, leader) = {
        let mut in_flight = session.in_flight.lock().unwrap();
        if let Some(existing) = in_flight.get(&key) {
            (existing.clone(), false)
        } else {
            let flight = Arc::new(InFlight {
                result: Mutex::new(None),
                notify: Notify::new(),
            });
            in_flight.insert(key.clone(), flight.clone());
            (flight, true)
        }
    };
    if !leader {
        loop {
            let notified = flight.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = flight.result.lock().unwrap().clone() {
                return result;
            }
            notified.await;
        }
    }

    let outcome = if session.cancellation.is_cancelled() {
        InferenceOutcome::Cancelled
    } else {
        let permit = tokio::select! {
            _ = session.cancellation.notify.notified() => None,
            permit = state.inference_slots.clone().acquire_owned() => permit.ok(),
        };
        match permit {
            None => InferenceOutcome::Cancelled,
            Some(permit) if session.cancellation.is_cancelled() => {
                drop(permit);
                InferenceOutcome::Cancelled
            }
            Some(permit) => {
                let result = tokio::select! {
                    _ = session.cancellation.notify.notified() => InferenceOutcome::Cancelled,
                    result = session.provider.explain(request) => match result {
                        Ok(explanation) => InferenceOutcome::Success(explanation),
                        Err(error) => {
                            let info = user_facing_error(&error);
                            eprintln!("model inference failed ({}): {error:#}", info.code);
                            InferenceOutcome::Failed(info)
                        },
                    },
                };
                drop(permit);
                result
            }
        }
    };
    *flight.result.lock().unwrap() = Some(outcome.clone());
    flight.notify.notify_waiters();
    session.in_flight.lock().unwrap().remove(&key);
    outcome
}
async fn update_if_current(
    state: &DaemonState,
    session: &Arc<RepositorySession>,
    id: &UnitId,
    e: UnitExplanation,
    deep: bool,
) -> bool {
    let Some(active) = current(state, &session.id.0).await else {
        return false;
    };
    if !Arc::ptr_eq(&active, session) {
        return false;
    }
    if let Some(item) = session.items.lock().unwrap().get_mut(id) {
        runtime::apply(item, e, deep);
        true
    } else {
        false
    }
}
fn stale_session() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":false,"stale":true,"code":"session_stale","error":"This repository session is no longer active. Reload the page and try again.","retryable":true}),
    )
}

fn daemon_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("GIT_EXPLAIN_DAEMON_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(crate::config::default_user_config_path()?
        .parent()
        .context("user config has no parent")?
        .to_path_buf())
}
fn metadata_path() -> Result<PathBuf> {
    Ok(daemon_dir()?.join("daemon.json"))
}
fn lock_path() -> Result<PathBuf> {
    Ok(daemon_dir()?.join("daemon.lock"))
}
fn log_path() -> Result<PathBuf> {
    Ok(daemon_dir()?.join("daemon.log"))
}
fn daemon_port() -> u16 {
    std::env::var("GIT_EXPLAIN_DAEMON_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}
fn write_metadata(metadata: &Metadata) -> Result<()> {
    let dir = daemon_dir()?;
    fs::create_dir_all(&dir)?;
    fs::write(metadata_path()?, serde_json::to_vec_pretty(metadata)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::http::HeaderValue;
    use std::sync::atomic::AtomicUsize;
    use std::{path::Path, process::Command};
    use tempfile::tempdir;

    fn test_state(session: Option<Arc<RepositorySession>>) -> DaemonState {
        let active_session = session.as_ref().map(|session| session.id.clone());
        let sessions = session
            .map(|session| HashMap::from([(session.id.0.clone(), session)]))
            .unwrap_or_default();
        DaemonState {
            port: DEFAULT_PORT,
            sessions: RwLock::new(sessions),
            active_session: RwLock::new(active_session),
            sequence: AtomicU64::new(0),
            control_token: "secret".into(),
            shutdown: Notify::new(),
            inference_slots: Arc::new(Semaphore::new(MAX_INFERENCE_REQUESTS)),
        }
    }

    #[test]
    fn control_authentication_requires_exact_token() {
        let state = test_state(None);
        let mut headers = HeaderMap::new();
        assert!(!authorized(&headers, &state));
        headers.insert(
            "x-git-explain-control-token",
            HeaderValue::from_static("wrong"),
        );
        assert!(!authorized(&headers, &state));
        headers.insert(
            "x-git-explain-control-token",
            HeaderValue::from_static("secret"),
        );
        assert!(authorized(&headers, &state));
    }

    #[test]
    fn session_ids_are_opaque_and_unique() {
        let first = new_id();
        let second = new_id();
        assert_eq!(first.len(), 32);
        assert_ne!(first, second);
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

    #[tokio::test]
    async fn refresh_replaces_snapshot_atomically_and_increments_generation() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        fs::write(dir.path().join("lib.rs"), "fn first() {}\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "base"]);
        fs::write(
            dir.path().join("lib.rs"),
            "fn first() { let changed = true; }\n",
        )
        .unwrap();
        let config = ConfigLoader::with_paths(dir.path().join("missing"), None)
            .resolve(None)
            .unwrap();
        let session = build_session(
            RepositorySessionId("stable-session".into()),
            dir.path().to_path_buf(),
            None,
            config,
            SnapshotGeneration(1),
        )
        .unwrap();
        let old_snapshot = session.snapshot.clone();
        let state = test_state(Some(session.clone()));

        fs::write(
            dir.path().join("lib.rs"),
            "fn first() { let changed_again = true; }\n",
        )
        .unwrap();
        let refreshed = refresh_current_session(&state).await.unwrap();
        let replacement = refreshed.session;
        assert!(refreshed.changed);
        assert_eq!(replacement.id, session.id);
        assert_eq!(replacement.snapshot.generation, SnapshotGeneration(2));
        assert_ne!(replacement.snapshot.identity, old_snapshot.identity);
        let active = active_session(&state).await.unwrap();
        assert!(Arc::ptr_eq(&active, &replacement));
        assert!(!Arc::ptr_eq(&active, &session));
    }

    #[tokio::test]
    async fn failed_refresh_keeps_previous_snapshot_active() {
        let state = test_state(None);
        assert!(refresh_current_session(&state).await.is_err());
        assert!(state.sessions.read().await.is_empty());
    }

    #[tokio::test]
    async fn unchanged_refresh_keeps_generation_and_session() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        fs::write(dir.path().join("lib.rs"), "fn first() {}\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "base"]);
        fs::write(
            dir.path().join("lib.rs"),
            "fn first() { let changed = true; }\n",
        )
        .unwrap();
        let config = ConfigLoader::with_paths(dir.path().join("missing"), None)
            .resolve(None)
            .unwrap();
        let session = build_session(
            RepositorySessionId("stable-session".into()),
            dir.path().to_path_buf(),
            None,
            config,
            SnapshotGeneration(4),
        )
        .unwrap();
        let state = test_state(Some(session.clone()));

        let refreshed = refresh_current_session(&state).await.unwrap();
        assert!(!refreshed.changed);
        assert_eq!(refreshed.session.snapshot.generation, SnapshotGeneration(4));
        assert!(Arc::ptr_eq(&refreshed.session, &session));
    }

    #[tokio::test]
    async fn session_registry_evicts_oldest_session_and_cancels_work() {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test"]);
        fs::write(
            dir.path().join("lib.rs"),
            "fn first() { let changed = true; }\n",
        )
        .unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-qm", "base"]);
        fs::write(
            dir.path().join("lib.rs"),
            "fn first() { let changed = true; let current = true; }\n",
        )
        .unwrap();
        let config = ConfigLoader::with_paths(dir.path().join("missing"), None)
            .resolve(None)
            .unwrap();
        let state = test_state(None);
        let mut first = None;

        for index in 0..=MAX_SESSIONS {
            let session = build_session(
                RepositorySessionId(format!("session-{index}")),
                dir.path().to_path_buf(),
                None,
                config.clone(),
                SnapshotGeneration(1),
            )
            .unwrap();
            if index == 0 {
                first = Some(session.clone());
            }
            register_session(&state, session).await;
        }

        let sessions = state.sessions.read().await;
        assert_eq!(sessions.len(), MAX_SESSIONS);
        assert!(!sessions.contains_key("session-0"));
        assert!(sessions.contains_key(&format!("session-{MAX_SESSIONS}")));
        assert!(first.unwrap().cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn session_cancellation_is_sticky_for_late_inference_requests() {
        let cancellation = Arc::new(SessionCancellation::new());
        cancellation.cancel();
        assert!(cancellation.is_cancelled());
        let notified = cancellation.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        assert!(cancellation.is_cancelled());
    }

    struct CountingProvider {
        calls: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    #[async_trait]
    impl ExplanationProvider for CountingProvider {
        async fn explain(&self, _request: ExplanationRequest) -> anyhow::Result<UnitExplanation> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            self.release.notified().await;
            Ok(UnitExplanation {
                overview: "shared result".into(),
                annotations: vec![],
                deep: None,
            })
        }
    }

    #[tokio::test]
    async fn identical_inference_requests_share_one_provider_call() {
        let config = ConfigLoader::with_paths(PathBuf::from("missing"), None)
            .resolve(None)
            .unwrap();
        let provider = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
            started: Notify::new(),
            release: Notify::new(),
        });
        let session = Arc::new(RepositorySession {
            id: RepositorySessionId("dedup-session".into()),
            repo_root: PathBuf::from("."),
            git_dir: PathBuf::from("."),
            analyzer: RepositoryAnalyzer::new(".", config.clone()),
            revision: None,
            config: config.clone(),
            snapshot: AnalysisSnapshot {
                generation: SnapshotGeneration(1),
                identity: crate::snapshot::SnapshotIdentity::WorkingTree {
                    fingerprint: "test".into(),
                },
                context: crate::explain::AnalysisContext::working_tree(),
                changes: vec![],
                units: vec![],
            },
            items: Mutex::new(HashMap::new()),
            provider: provider.clone(),
            cache: None,
            model: config.model.clone(),
            reader: config.reader.clone(),
            explanation: config.explanation.clone(),
            cancellation: Arc::new(SessionCancellation::new()),
            in_flight: Mutex::new(HashMap::new()),
            last_used: AtomicU64::new(0),
        });
        let state = Arc::new(test_state(Some(session.clone())));
        let request = ExplanationRequest {
            source_unit: "fn f() {}".into(),
            unit_name: "f".into(),
            unit_kind: "Function".into(),
            diff: "+fn f() {}".into(),
            language: "Rust".into(),
            git_context: String::new(),
            regions: vec![],
            prior_explanation: None,
            deep: false,
        };
        let first_state = state.clone();
        let first_session = session.clone();
        let first_request = request.clone();
        let first = tokio::spawn(async move {
            infer_with_dedup(
                &first_state,
                &first_session,
                "same-key".into(),
                first_request,
            )
            .await
        });
        provider.started.notified().await;
        let second_state = state.clone();
        let second_session = session.clone();
        let second = tokio::spawn(async move {
            infer_with_dedup(&second_state, &second_session, "same-key".into(), request).await
        });
        tokio::task::yield_now().await;
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        provider.release.notify_one();
        assert!(matches!(first.await.unwrap(), InferenceOutcome::Success(_)));
        assert!(matches!(
            second.await.unwrap(),
            InferenceOutcome::Success(_)
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}
