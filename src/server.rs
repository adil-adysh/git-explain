use crate::{
    cache::ExplanationCache,
    change_set::{ChangeSet, SemanticChangeId},
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile, ServerConfig},
    file_analysis::{self, FileAnalysis},
    model::{user_facing_error, ExplanationProvider, UnitExplanation},
    runtime,
    snapshot::SnapshotGeneration,
    web,
};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

struct StateData {
    change_set: Mutex<ChangeSet>,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
struct FileStateData {
    analysis: Mutex<FileAnalysis>,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
#[derive(Debug, Deserialize)]
struct SnapshotRequest {
    generation: Option<SnapshotGeneration>,
}
pub async fn serve(
    mut change_set: ChangeSet,
    provider: impl ExplanationProvider + 'static,
    config: ServerConfig,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
) -> Result<()> {
    if let Some(cache_ref) = &cache {
        runtime::hydrate(&mut change_set, cache_ref, &model, &reader, &explanation);
    }
    let state = Arc::new(StateData {
        change_set: Mutex::new(change_set),
        provider: Arc::new(provider),
        cache,
        model,
        reader,
        explanation,
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/changes", get(changes))
        .route("/api/units/{id}/explain", post(explain))
        .route("/api/units/{id}/deep", post(deep))
        .route("/api/units/{id}/deep/regenerate", post(deep_regenerate))
        .route("/api/units/{id}/regenerate", post(regenerate))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("git explain: {url}");
    if config.open_browser {
        let _ = webbrowser::open(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn serve_file(
    mut analysis: FileAnalysis,
    provider: impl ExplanationProvider + 'static,
    config: ServerConfig,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
) -> Result<()> {
    if let Some(cache_ref) = &cache {
        analysis.hydrate(cache_ref, &model, &reader, &explanation);
    }
    let state = Arc::new(FileStateData {
        analysis: Mutex::new(analysis),
        provider: Arc::new(provider),
        cache,
        model,
        reader,
        explanation,
    });
    let app = Router::new()
        .route("/", get(file_index))
        .route("/api/file-units/{id}/explain", post(file_explain))
        .route("/api/file-units/{id}/deep", post(file_deep))
        .route("/api/file-units/{id}/regenerate", post(file_regenerate))
        .route(
            "/api/file-units/{id}/deep/regenerate",
            post(file_deep_regenerate),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("git explain: {url}");
    if config.open_browser {
        let _ = webbrowser::open(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}

async fn file_index(State(state): State<Arc<FileStateData>>) -> Html<String> {
    Html(web::render_file_analysis(&state.analysis.lock().unwrap()))
}

async fn file_explain(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, false, state).await
}

async fn file_deep(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, true, false, state).await
}

async fn file_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, true, state).await
}

async fn file_deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, true, true, state).await
}

async fn generate_file(
    id: String,
    deep: bool,
    regenerate: bool,
    state: Arc<FileStateData>,
) -> Json<serde_json::Value> {
    let request = {
        let analysis = state.analysis.lock().unwrap();
        let Some(unit) = analysis.find_unit(&id) else {
            return Json(
                serde_json::json!({"ok":false,"code":"unit_not_found","error":"This code unit is no longer available.","retryable":false}),
            );
        };
        let prior = deep
            .then(|| {
                analysis
                    .explanations
                    .get(&id)
                    .map(|value| value.overview.clone())
            })
            .flatten();
        file_analysis::request_for(unit, analysis.revision.as_deref(), deep, prior)
    };
    let key =
        ExplanationCache::snapshot_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    state
                        .analysis
                        .lock()
                        .unwrap()
                        .apply(&id, result.clone(), deep);
                    return Json(runtime::result(result, true, deep));
                }
            }
        }
    }
    match state.provider.explain(request.clone()).await {
        Ok(result) => {
            if let Some(cache) = &state.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &state.model,
                    &serde_json::to_string(&result).unwrap(),
                );
            }
            state
                .analysis
                .lock()
                .unwrap()
                .apply(&id, result.clone(), deep);
            Json(runtime::result(result, false, deep))
        }
        Err(error) => {
            let info = user_facing_error(&error);
            Json(
                serde_json::json!({"ok":false,"code":info.code,"error":info.message,"retryable":info.retryable}),
            )
        }
    }
}
async fn index(State(state): State<Arc<StateData>>) -> Html<String> {
    Html(web::render(&state.change_set.lock().unwrap()))
}
async fn changes(State(state): State<Arc<StateData>>) -> Json<serde_json::Value> {
    Json(state.change_set.lock().unwrap().json())
}
async fn explain(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, false, false, body.map(|Json(value)| value), state).await
}
async fn deep(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, true, false, body.map(|Json(value)| value), state).await
}
async fn deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, true, true, body.map(|Json(value)| value), state).await
}
async fn regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, false, true, body.map(|Json(value)| value), state).await
}
async fn generate(
    id: String,
    deep: bool,
    regenerate: bool,
    body: Option<SnapshotRequest>,
    state: Arc<StateData>,
) -> Json<serde_json::Value> {
    let id = SemanticChangeId(id);
    let (generation, request) = {
        let changes = state.change_set.lock().unwrap();
        if body
            .and_then(|value| value.generation)
            .is_some_and(|wanted| wanted != changes.generation)
        {
            return stale();
        }
        let Some((file, change)) = changes.find_change(&id) else {
            return Json(
                serde_json::json!({"ok":false,"code":"unit_not_found","error":"This code unit is no longer available.","retryable":false}),
            );
        };
        (
            changes.generation,
            runtime::request_for(file, change, &changes.context, deep),
        )
    };
    let key =
        ExplanationCache::change_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    if apply(&state, &id, generation, result.clone(), deep) {
                        return Json(runtime::result(result, true, deep));
                    }
                    return stale();
                }
            }
        }
    }
    match state.provider.explain(request.clone()).await {
        Ok(result) => {
            if let Some(cache) = &state.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &state.model,
                    &serde_json::to_string(&result).unwrap(),
                );
            }
            if apply(&state, &id, generation, result.clone(), deep) {
                Json(runtime::result(result, false, deep))
            } else {
                stale()
            }
        }
        Err(error) => {
            let info = user_facing_error(&error);
            Json(
                serde_json::json!({"ok":false,"code":info.code,"error":info.message,"retryable":info.retryable}),
            )
        }
    }
}
fn apply(
    state: &StateData,
    id: &SemanticChangeId,
    generation: SnapshotGeneration,
    result: UnitExplanation,
    deep: bool,
) -> bool {
    let mut changes = state.change_set.lock().unwrap();
    if changes.generation != generation {
        return false;
    }
    let Some(change) = changes.find_change_mut(id) else {
        return false;
    };
    runtime::apply(change, result, deep);
    true
}
fn stale() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":false,"stale":true,"code":"snapshot_stale","error":"The repository changed while this page was open. Reload the page and try again.","retryable":true}),
    )
}
