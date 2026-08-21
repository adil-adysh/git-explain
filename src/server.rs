use crate::{
    cache::ExplanationCache,
    config::{ExplanationConfig, ModelConfig, ReaderConfig, ServerConfig},
    explain::ExplainedUnit,
    model::{ExplanationProvider, UnitExplanation},
    runtime,
    snapshot::{AnalysisSnapshot, SnapshotGeneration, UnitId},
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
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct StateData {
    snapshot: AnalysisSnapshot,
    items: Mutex<HashMap<UnitId, ExplainedUnit>>,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ModelConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
#[derive(Debug, Deserialize)]
struct SnapshotRequest {
    generation: Option<SnapshotGeneration>,
}
pub async fn serve(
    snapshot: AnalysisSnapshot,
    provider: impl ExplanationProvider + 'static,
    config: ServerConfig,
    cache: Option<ExplanationCache>,
    model: ModelConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
) -> Result<()> {
    let mut items: HashMap<_, _> = snapshot
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
            &model,
            &reader,
            &explanation,
        );
    }
    let state = Arc::new(StateData {
        snapshot,
        items: Mutex::new(items),
        provider: Arc::new(provider),
        cache,
        model,
        reader,
        explanation,
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/units/{id}/explain", post(explain))
        .route("/api/units/{id}/deep", post(deep))
        .route("/api/units/{id}/deep/regenerate", post(deep_regenerate))
        .route("/api/units/{id}/regenerate", post(regenerate))
        .route("/api/deep/{id}", post(deep))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("git explain: {}", url);
    if config.open_browser {
        let _ = webbrowser::open(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}
async fn index(State(state): State<Arc<StateData>>) -> Html<String> {
    let items = state.items.lock().unwrap();
    let ordered: Vec<_> = state
        .snapshot
        .units
        .iter()
        .filter_map(|unit| items.get(&unit.id))
        .cloned()
        .collect();
    Html(web::render(&ordered, &state.snapshot.context))
}
async fn explain(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, false, false, body.map(|Json(body)| body), state).await
}
async fn deep(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, true, false, body.map(|Json(body)| body), state).await
}
async fn deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, true, true, body.map(|Json(body)| body), state).await
}
async fn regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(id, false, true, body.map(|Json(body)| body), state).await
}
async fn generate(
    id: String,
    deep: bool,
    regenerate: bool,
    body: Option<SnapshotRequest>,
    state: Arc<StateData>,
) -> Json<serde_json::Value> {
    let requested = body.and_then(|body| body.generation);
    if requested.is_some_and(|generation| generation != state.snapshot.generation) {
        return stale();
    }
    let unit_id = UnitId(id);
    let item = { state.items.lock().unwrap().get(&unit_id).cloned() };
    let Some(item) = item else {
        return Json(serde_json::json!({"ok":false,"error":"Unknown code unit."}));
    };
    let request = runtime::request_for(&item, &state.snapshot.context, deep);
    let key = ExplanationCache::key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(e) = serde_json::from_str::<UnitExplanation>(&json) {
                    if update(&state, &unit_id, state.snapshot.generation, e.clone(), deep) {
                        return Json(runtime::result(e, true, deep));
                    }
                    return stale();
                }
            }
        }
    }
    match state.provider.explain(request.clone()).await {
        Ok(e) => {
            if let Some(cache) = &state.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &state.model,
                    &serde_json::to_string(&e).unwrap(),
                );
            }
            if update(&state, &unit_id, state.snapshot.generation, e.clone(), deep) {
                Json(runtime::result(e, false, deep))
            } else {
                stale()
            }
        }
        Err(error) => {
            eprintln!("unit {} explanation failed: {error:#}", unit_id);
            Json(serde_json::json!({"ok":false,"error":"Explanation unavailable."}))
        }
    }
}
fn update(
    state: &StateData,
    id: &UnitId,
    generation: SnapshotGeneration,
    e: UnitExplanation,
    deep: bool,
) -> bool {
    if generation != state.snapshot.generation {
        return false;
    }
    let mut items = state.items.lock().unwrap();
    let Some(item) = items.get_mut(id) else {
        return false;
    };
    runtime::apply(item, e, deep);
    true
}
fn stale() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok":false,"stale":true,"error":"Repository snapshot has changed."}))
}
