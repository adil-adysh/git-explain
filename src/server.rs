use crate::learning_runtime::{LearningLookup, LearningRuntime, LearningSave};
use crate::{
    cache::ExplanationCache,
    change_set::{ChangeSet, SemanticChangeId},
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile, ServerConfig},
    file_analysis::{self, FileAnalysis},
    model::{user_facing_error, ExplanationKind, ExplanationProvider, UnitExplanation},
    runtime,
    snapshot::SnapshotGeneration,
    web,
};
use anyhow::Result;
use axum::http::{HeaderMap, StatusCode};
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

struct StateData {
    learning: LearningRuntime,
    generation_lock: tokio::sync::Mutex<()>,
    change_set: Mutex<ChangeSet>,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
struct FileStateData {
    learning: LearningRuntime,
    generation_lock: tokio::sync::Mutex<()>,
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
    #[serde(default)]
    learning: Option<crate::learning::LearningSelection>,
}
pub async fn serve(
    mut change_set: ChangeSet,
    provider: impl ExplanationProvider + 'static,
    config: ServerConfig,
    cache: Option<ExplanationCache>,
    model: ResolvedProfile,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
    learning: LearningRuntime,
) -> Result<()> {
    if let Some(cache_ref) = &cache {
        runtime::hydrate(&mut change_set, cache_ref, &model, &reader, &explanation);
    }
    let state = Arc::new(StateData {
        learning,
        generation_lock: tokio::sync::Mutex::new(()),
        change_set: Mutex::new(change_set),
        provider: Arc::new(provider),
        cache,
        model,
        reader,
        explanation,
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/learning", get(learning_inspect).put(learning_save))
        .route("/api/learning/resolve", post(learning_resolve))
        .route("/api/learning/results", post(learning_results))
        .route("/api/changes", get(changes))
        .route("/api/units/{id}/explain", post(explain))
        .route("/api/units/{id}/overview", post(overview))
        .route(
            "/api/units/{id}/overview/regenerate",
            post(overview_regenerate),
        )
        .route("/api/units/{id}/concepts", post(diff_concepts))
        .route(
            "/api/units/{id}/concepts/regenerate",
            post(diff_concepts_regenerate),
        )
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
    learning: LearningRuntime,
) -> Result<()> {
    if let Some(cache_ref) = &cache {
        analysis.hydrate(cache_ref, &model, &reader, &explanation);
    }
    let state = Arc::new(FileStateData {
        learning,
        generation_lock: tokio::sync::Mutex::new(()),
        analysis: Mutex::new(analysis),
        provider: Arc::new(provider),
        cache,
        model,
        reader,
        explanation,
    });
    let app = Router::new()
        .route("/", get(file_index))
        .route(
            "/api/learning",
            get(file_learning_inspect).put(file_learning_save),
        )
        .route("/api/learning/resolve", post(file_learning_resolve))
        .route("/api/learning/results", post(file_learning_results))
        .route("/api/file-units/{id}/explain", post(file_explain))
        .route("/api/file-units/{id}/concepts", post(file_concepts))
        .route(
            "/api/file-units/{id}/concepts/regenerate",
            post(file_concepts_regenerate),
        )
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
    let mut html = web::render_file_analysis(&state.analysis.lock().unwrap());
    let ids = {
        let analysis = state.analysis.lock().unwrap();
        analysis
            .documents
            .iter()
            .flat_map(|d| d.units.iter())
            .map(crate::file_analysis::unit_key)
            .collect::<Vec<_>>()
    };
    for id in ids {
        let mut saved = Vec::new();
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            if let Ok((request, prior)) = file_request(&state, &id, kind, None) {
                let key = ExplanationCache::snapshot_key(
                    &request,
                    &state.model,
                    &state.reader,
                    &state.explanation,
                );
                if let Some(result) =
                    state
                        .learning
                        .restore(&key, &request, prior, state.cache.as_ref())
                {
                    saved.push((kind, result));
                }
            }
        }
        LearningRuntime::decorate_article(&mut html, &id, &saved);
    }
    Html(html)
}

async fn file_explain(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        false,
        false,
        false,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}
async fn file_concepts(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        false,
        false,
        true,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}
async fn file_concepts_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        false,
        true,
        true,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}

async fn file_deep(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        true,
        false,
        false,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}

async fn file_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        false,
        true,
        false,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}

async fn file_deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(
        id,
        true,
        true,
        false,
        state,
        body.and_then(|Json(v)| v.learning),
    )
    .await
}

#[cfg(test)]
async fn generate_file(
    id: String,
    deep: bool,
    regenerate: bool,
    concepts: bool,
    state: Arc<FileStateData>,
) -> Json<serde_json::Value> {
    generate_file_with_learning(id, deep, regenerate, concepts, state, None).await
}
fn file_request(
    state: &FileStateData,
    id: &str,
    kind: ExplanationKind,
    selection: Option<&crate::learning::LearningSelection>,
) -> anyhow::Result<(crate::model::ExplanationRequest, Option<String>)> {
    let mut request = {
        let analysis = state.analysis.lock().unwrap();
        let unit = analysis
            .find_unit(id)
            .ok_or_else(|| anyhow::anyhow!("This code section is no longer available."))?;
        if kind == ExplanationKind::Concepts {
            file_analysis::concept_request_for(unit, analysis.revision.as_deref())
        } else {
            file_analysis::request_for(unit, analysis.revision.as_deref(), kind.is_deep(), None)
        }
    };
    state.learning.prepare(&mut request, selection)?;
    let prerequisite = file_prerequisite(state, &mut request);
    Ok((request, prerequisite))
}
fn file_prerequisite(
    state: &FileStateData,
    request: &mut crate::model::ExplanationRequest,
) -> Option<String> {
    request.prior_explanation = None;
    request.prerequisite_id = None;
    let mut prerequisite = None;
    if request.kind.is_deep() {
        let mut prior = request.clone();
        prior.kind = ExplanationKind::Overview;
        prior.deep = false;
        prior.prior_explanation = None;
        let key =
            ExplanationCache::snapshot_key(&prior, &state.model, &state.reader, &state.explanation);
        if let Some(value) = state
            .learning
            .restore(&key, &prior, None, state.cache.as_ref())
        {
            request.prior_explanation = Some(value.explanation.overview);
            prerequisite = Some(value.result_id);
            request.prerequisite_id = prerequisite.clone();
        }
    }
    prerequisite
}
async fn generate_file_with_learning(
    id: String,
    deep: bool,
    regenerate: bool,
    concepts: bool,
    state: Arc<FileStateData>,
    selection: Option<crate::learning::LearningSelection>,
) -> Json<serde_json::Value> {
    let kind = if deep {
        ExplanationKind::Deep
    } else if concepts {
        ExplanationKind::Concepts
    } else {
        ExplanationKind::Overview
    };
    // Freeze preferences before waiting; saved defaults may change while queued.
    let (mut request, _) = match file_request(&state, &id, kind, selection.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            return Json(
                serde_json::json!({"ok":false,"error":e.to_string(),"code":"learning_request_invalid"}),
            )
        }
    };
    let _generation = state.generation_lock.lock().await;
    // Overview may have been generated or replaced by the preceding request.
    let prerequisite = file_prerequisite(&state, &mut request);
    let key =
        ExplanationCache::snapshot_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(value) =
            state
                .learning
                .restore(&key, &request, prerequisite.clone(), state.cache.as_ref())
        {
            return Json(value.response(true, kind));
        }
    }
    match state.provider.explain(request.clone()).await {
        Ok(result) => {
            let value = state
                .learning
                .remember(&key, &request, prerequisite, result);
            if let Some(cache) = &state.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &state.model,
                    &serde_json::to_string(&value).unwrap(),
                );
            }
            Json(value.response(false, kind))
        }
        Err(e) => {
            let info = user_facing_error(&e);
            Json(
                serde_json::json!({"ok":false,"code":info.code,"error":info.message,"retryable":info.retryable}),
            )
        }
    }
}
fn learning_json(result: anyhow::Result<serde_json::Value>) -> Json<serde_json::Value> {
    Json(result.unwrap_or_else(|e| serde_json::json!({"ok":false,"error":e.to_string()})))
}
async fn learning_inspect(State(state): State<Arc<StateData>>) -> Json<serde_json::Value> {
    learning_json(state.learning.inspect())
}
async fn file_learning_inspect(State(state): State<Arc<FileStateData>>) -> Json<serde_json::Value> {
    learning_json(state.learning.inspect())
}
async fn learning_resolve(
    State(state): State<Arc<StateData>>,
    Json(body): Json<LearningLookup>,
) -> Json<serde_json::Value> {
    learning_json(state.learning.resolve_response(&body))
}
async fn file_learning_resolve(
    State(state): State<Arc<FileStateData>>,
    Json(body): Json<LearningLookup>,
) -> Json<serde_json::Value> {
    learning_json(state.learning.resolve_response(&body))
}
async fn learning_save(
    State(state): State<Arc<StateData>>,
    headers: HeaderMap,
    Json(body): Json<LearningSave>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, value) = state.learning.save(&headers, body);
    (status, Json(value))
}
async fn file_learning_save(
    State(state): State<Arc<FileStateData>>,
    headers: HeaderMap,
    Json(body): Json<LearningSave>,
) -> (StatusCode, Json<serde_json::Value>) {
    let (status, value) = state.learning.save(&headers, body);
    (status, Json(value))
}
async fn file_learning_results(
    State(state): State<Arc<FileStateData>>,
    Json(body): Json<LearningLookup>,
) -> Json<serde_json::Value> {
    let kind = body.kind.unwrap_or(ExplanationKind::Overview);
    let (request, prerequisite) = match file_request(
        &state,
        &body.unit_id,
        kind,
        body.learning.as_ref().or(body.selection.as_ref()),
    ) {
        Ok(v) => v,
        Err(e) => return learning_json(Err(e)),
    };
    let key =
        ExplanationCache::snapshot_key(&request, &state.model, &state.reader, &state.explanation);
    Json(
        state
            .learning
            .restore(&key, &request, prerequisite, state.cache.as_ref())
            .map(|v| v.response(true, kind))
            .unwrap_or_else(|| serde_json::json!({"ok":true,"result":null})),
    )
}
fn diff_request(
    state: &StateData,
    id: &SemanticChangeId,
    kind: ExplanationKind,
    selection: Option<&crate::learning::LearningSelection>,
) -> anyhow::Result<(crate::model::ExplanationRequest, Option<String>)> {
    let mut request = {
        let changes = state.change_set.lock().unwrap();
        let (file, change) = changes
            .find_change(id)
            .ok_or_else(|| anyhow::anyhow!("This code section is no longer available."))?;
        if !change.kind.explainable() {
            anyhow::bail!("This code section is unchanged.");
        }
        runtime::request_for_kind(file, change, &changes.context, kind)
    };
    state.learning.prepare(&mut request, selection)?;
    let prerequisite = diff_prerequisite(state, id, &mut request)?;
    Ok((request, prerequisite))
}
fn diff_prerequisite(
    state: &StateData,
    id: &SemanticChangeId,
    request: &mut crate::model::ExplanationRequest,
) -> anyhow::Result<Option<String>> {
    request.prior_explanation = None;
    request.prerequisite_id = None;
    let mut prerequisite = None;
    if request.kind.is_deep() {
        let mut prior = {
            let changes = state.change_set.lock().unwrap();
            let (file, change) = changes
                .find_change(id)
                .ok_or_else(|| anyhow::anyhow!("This code section is no longer available."))?;
            runtime::request_for_kind(file, change, &changes.context, ExplanationKind::Changes)
        };
        prior.policy = request.policy.clone();
        let key =
            ExplanationCache::change_key(&prior, &state.model, &state.reader, &state.explanation);
        if let Some(value) = state
            .learning
            .restore(&key, &prior, None, state.cache.as_ref())
        {
            request.prior_explanation = Some(value.explanation.overview);
            prerequisite = Some(value.result_id);
            request.prerequisite_id = prerequisite.clone();
        }
    }
    Ok(prerequisite)
}
async fn learning_results(
    State(state): State<Arc<StateData>>,
    Json(body): Json<LearningLookup>,
) -> Json<serde_json::Value> {
    let kind = body.kind.unwrap_or(ExplanationKind::Overview);
    let (request, prerequisite) = match diff_request(
        &state,
        &SemanticChangeId(body.unit_id),
        kind,
        body.learning.as_ref().or(body.selection.as_ref()),
    ) {
        Ok(v) => v,
        Err(e) => return learning_json(Err(e)),
    };
    let key =
        ExplanationCache::change_key(&request, &state.model, &state.reader, &state.explanation);
    Json(
        state
            .learning
            .restore(&key, &request, prerequisite, state.cache.as_ref())
            .map(|v| v.response(true, kind))
            .unwrap_or_else(|| serde_json::json!({"ok":true,"result":null})),
    )
}
async fn index(State(state): State<Arc<StateData>>) -> Html<String> {
    let mut html = web::render(&state.change_set.lock().unwrap());
    let ids = state
        .change_set
        .lock()
        .unwrap()
        .explainable_changes()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    for id in ids {
        let mut saved = Vec::new();
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Changes,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            if let Ok((request, prior)) = diff_request(&state, &id, kind, None) {
                let key = ExplanationCache::change_key(
                    &request,
                    &state.model,
                    &state.reader,
                    &state.explanation,
                );
                if let Some(result) =
                    state
                        .learning
                        .restore(&key, &request, prior, state.cache.as_ref())
                {
                    saved.push((kind, result));
                }
            }
        }
        LearningRuntime::decorate_article(&mut html, &id.0, &saved);
    }
    Html(html)
}
async fn changes(State(state): State<Arc<StateData>>) -> Json<serde_json::Value> {
    Json(state.change_set.lock().unwrap().json())
}
async fn explain(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Changes,
        false,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn overview(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Overview,
        false,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn overview_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Overview,
        true,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn diff_concepts(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Concepts,
        false,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn diff_concepts_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Concepts,
        true,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn deep(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Deep,
        false,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Deep,
        true,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<StateData>>,
    body: Option<Json<SnapshotRequest>>,
) -> Json<serde_json::Value> {
    generate(
        id,
        ExplanationKind::Changes,
        true,
        body.map(|Json(value)| value),
        state,
    )
    .await
}
async fn generate(
    id: String,
    kind: ExplanationKind,
    regenerate: bool,
    body: Option<SnapshotRequest>,
    state: Arc<StateData>,
) -> Json<serde_json::Value> {
    let id = SemanticChangeId(id);
    {
        let changes = state.change_set.lock().unwrap();
        if body
            .as_ref()
            .and_then(|b| b.generation)
            .is_some_and(|v| v != changes.generation)
        {
            return stale();
        }
        let Some((_, change)) = changes.find_change(&id) else {
            return Json(
                serde_json::json!({"ok":false,"code":"unit_not_found","error":"This code section is no longer available."}),
            );
        };
        if !change.kind.explainable() {
            return Json(
                serde_json::json!({"ok":false,"code":"no_explanation_required","error":"This code section is unchanged."}),
            );
        }
    }
    // Freeze preferences before waiting; saved defaults may change while queued.
    let (mut request, _) = match diff_request(
        &state,
        &id,
        kind,
        body.as_ref().and_then(|b| b.learning.as_ref()),
    ) {
        Ok(v) => v,
        Err(e) => return learning_json(Err(e)),
    };
    let _generation = state.generation_lock.lock().await;
    // Refresh Changes without resolving the captured learning policy again.
    let prerequisite = match diff_prerequisite(&state, &id, &mut request) {
        Ok(v) => v,
        Err(e) => return learning_json(Err(e)),
    };
    let key =
        ExplanationCache::change_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(value) =
            state
                .learning
                .restore(&key, &request, prerequisite.clone(), state.cache.as_ref())
        {
            return Json(value.response(true, kind));
        }
    }
    match state.provider.explain(request.clone()).await {
        Ok(result) => {
            let value = state
                .learning
                .remember(&key, &request, prerequisite, result);
            if let Some(cache) = &state.cache {
                let _ = cache.put(
                    &key,
                    &request,
                    &state.model,
                    &serde_json::to_string(&value).unwrap(),
                );
            }
            Json(value.response(false, kind))
        }
        Err(e) => {
            let info = user_facing_error(&e);
            Json(
                serde_json::json!({"ok":false,"code":info.code,"error":info.message,"retryable":info.retryable}),
            )
        }
    }
}

fn stale() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":false,"stale":true,"code":"snapshot_stale","error":"The repository changed while this page was open. Reload the page and try again.","retryable":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    struct CountingProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ExplanationProvider for CountingProvider {
        async fn explain(
            &self,
            _request: crate::model::ExplanationRequest,
        ) -> anyhow::Result<UnitExplanation> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(UnitExplanation {
                teaching: None,
                overview: format!("result {call}"),
                annotations: vec![crate::model::Annotation {
                    title: None,
                    start_line: 1,
                    end_line: 1,
                    kind: "Behavior".into(),
                    text: "annotation".into(),
                }],
                deep: None,
            })
        }
    }

    fn test_config(directory: &std::path::Path) -> crate::config::ResolvedConfig {
        let config_path = directory.join("config.toml");
        std::fs::write(
            &config_path,
            "[model]\nprofile = \"test\"\n\n[profiles.test]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8080/v1\"\nmodel = \"test\"\n",
        )
        .unwrap();
        crate::config::ConfigLoader::with_paths(config_path, None)
            .resolve(Some("test"))
            .unwrap()
    }

    #[tokio::test]
    async fn queued_file_request_keeps_the_policy_selected_before_waiting() {
        queued_file_request(false).await;
    }

    #[tokio::test]
    async fn queued_file_deep_uses_the_latest_prerequisite_with_its_original_policy() {
        queued_file_request(true).await;
    }

    async fn queued_file_request(deep: bool) {
        let directory = tempdir().unwrap();
        let config = test_config(directory.path());
        let store = crate::learning::LearningStore::new(config.paths.user.clone());
        let document = crate::semantic::extract_document(&crate::change_set::SourceFile {
            path: "lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        })
        .unwrap();
        let id = crate::file_analysis::unit_key(&document.units[0]);
        let provider = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let state = Arc::new(FileStateData {
            learning: LearningRuntime::new(
                config.paths.clone(),
                config.reader.clone(),
                Default::default(),
            )
            .unwrap(),
            generation_lock: tokio::sync::Mutex::new(()),
            analysis: Mutex::new(FileAnalysis {
                revision: None,
                documents: vec![document],
                explanations: Default::default(),
                concept_explanations: Default::default(),
                deep_explanations: Default::default(),
            }),
            provider: provider.clone(),
            cache: None,
            model: config.model,
            reader: config.reader,
            explanation: config.explanation,
        });
        let guard = state.generation_lock.lock().await;
        let mut request = Box::pin(generate_file_with_learning(
            id.clone(),
            deep,
            false,
            false,
            state.clone(),
            None,
        ));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(request.as_mut(), &mut context).is_pending());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        let prior_id = if deep {
            let (prior, _) = file_request(&state, &id, ExplanationKind::Overview, None).unwrap();
            let key = ExplanationCache::snapshot_key(
                &prior,
                &state.model,
                &state.reader,
                &state.explanation,
            );
            Some(
                state
                    .learning
                    .remember(
                        &key,
                        &prior,
                        None,
                        UnitExplanation {
                            overview: "Latest overview".into(),
                            annotations: vec![],
                            deep: None,
                            teaching: None,
                        },
                    )
                    .result_id,
            )
        } else {
            None
        };
        let before = store.inspect().unwrap();
        let mut changed = before.config;
        changed.defaults.detail = Some(crate::learning::Detail::Detailed);
        store.save(&before.revision, changed).unwrap();
        drop(guard);
        let response = request.await.0;
        assert_eq!(response["policy"]["detail"], "balanced");
        assert_eq!(response["prerequisite_id"], serde_json::json!(prior_id));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn queued_diff_request_keeps_the_policy_selected_before_waiting() {
        queued_diff_request(ExplanationKind::Overview).await;
    }

    #[tokio::test]
    async fn queued_diff_deep_uses_the_latest_prerequisite_with_its_original_policy() {
        queued_diff_request(ExplanationKind::Deep).await;
    }

    async fn queued_diff_request(kind: ExplanationKind) {
        let directory = tempdir().unwrap();
        let config = test_config(directory.path());
        let store = crate::learning::LearningStore::new(config.paths.user.clone());
        let before_source = crate::change_set::SourceFile {
            path: "lib.rs".into(),
            content: "fn inspect() { let value = 1; }\n".into(),
        };
        let after_source = crate::change_set::SourceFile {
            path: "lib.rs".into(),
            content: "fn inspect() { let value = 2; }\n".into(),
        };
        let before_document = crate::semantic::extract_document(&before_source).unwrap();
        let after_document = crate::semantic::extract_document(&after_source).unwrap();
        let semantic_changes = crate::semantic::match_documents(
            Some(&before_document),
            Some(&after_document),
            String::new(),
        );
        let id = semantic_changes[0].id.clone();
        let provider = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let state = Arc::new(StateData {
            learning: LearningRuntime::new(
                config.paths.clone(),
                config.reader.clone(),
                Default::default(),
            )
            .unwrap(),
            generation_lock: tokio::sync::Mutex::new(()),
            change_set: Mutex::new(ChangeSet {
                generation: SnapshotGeneration(1),
                identity: crate::snapshot::SnapshotIdentity::WorkingTree {
                    fingerprint: "queued-policy".into(),
                },
                context: crate::explain::AnalysisContext::working_tree(),
                files: vec![crate::change_set::FileChange {
                    old_path: Some("lib.rs".into()),
                    new_path: Some("lib.rs".into()),
                    kind: crate::change_set::FileChangeKind::Modified,
                    similarity: None,
                    old_mode: Some("100644".into()),
                    new_mode: Some("100644".into()),
                    binary: false,
                    patch: None,
                    before: Some(before_source),
                    after: Some(after_source),
                    semantic_changes,
                }],
            }),
            provider: provider.clone(),
            cache: None,
            model: config.model,
            reader: config.reader,
            explanation: config.explanation,
        });
        let guard = state.generation_lock.lock().await;
        let mut request = Box::pin(generate(id.0.clone(), kind, false, None, state.clone()));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(std::future::Future::poll(request.as_mut(), &mut context).is_pending());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        let prior_id = if kind.is_deep() {
            let (prior, _) = diff_request(&state, &id, ExplanationKind::Changes, None).unwrap();
            let key = ExplanationCache::change_key(
                &prior,
                &state.model,
                &state.reader,
                &state.explanation,
            );
            Some(
                state
                    .learning
                    .remember(
                        &key,
                        &prior,
                        None,
                        UnitExplanation {
                            overview: "Latest changes".into(),
                            annotations: vec![],
                            deep: None,
                            teaching: None,
                        },
                    )
                    .result_id,
            )
        } else {
            None
        };
        let before = store.inspect().unwrap();
        let mut changed = before.config;
        changed.defaults.detail = Some(crate::learning::Detail::Detailed);
        store.save(&before.revision, changed).unwrap();
        drop(guard);
        let response = request.await.0;
        assert_eq!(response["policy"]["detail"], "balanced");
        assert_eq!(response["prerequisite_id"], serde_json::json!(prior_id));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn file_generation_uses_snapshot_cache_unless_regenerated() {
        let directory = tempdir().unwrap();
        let config = test_config(directory.path());
        let source = crate::change_set::SourceFile {
            path: "lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        };
        let document = crate::semantic::extract_document(&source).unwrap();
        let id = crate::file_analysis::unit_key(&document.units[0]);
        let provider = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let state = Arc::new(FileStateData {
            learning: LearningRuntime::new(
                config.paths.clone(),
                config.reader.clone(),
                Default::default(),
            )
            .unwrap(),
            generation_lock: tokio::sync::Mutex::new(()),
            analysis: Mutex::new(FileAnalysis {
                revision: None,
                documents: vec![document],
                explanations: Default::default(),
                concept_explanations: Default::default(),
                deep_explanations: Default::default(),
            }),
            provider: provider.clone(),
            cache: Some(ExplanationCache::open(directory.path()).unwrap()),
            model: config.model,
            reader: config.reader,
            explanation: config.explanation,
        });

        let first = generate_file(id.clone(), false, false, false, state.clone())
            .await
            .0;
        assert_eq!(first["cache_hit"], false);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

        let cached = generate_file(id.clone(), false, false, false, state.clone())
            .await
            .0;
        assert_eq!(cached["cache_hit"], true);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

        let regenerated = generate_file(id, false, true, false, state).await.0;
        assert_eq!(regenerated["cache_hit"], false);
        assert_eq!(regenerated["overview"], "result 2");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn preserved_diff_unit_cannot_trigger_provider_inference() {
        let directory = tempdir().unwrap();
        let config = test_config(directory.path());
        let source = crate::change_set::SourceFile {
            path: "lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        };
        let document = crate::semantic::extract_document(&source).unwrap();
        let semantic_changes =
            crate::semantic::match_documents(Some(&document), Some(&document), String::new());
        let id = semantic_changes[0].id.clone();
        let provider = Arc::new(CountingProvider {
            calls: AtomicUsize::new(0),
        });
        let state = Arc::new(StateData {
            learning: LearningRuntime::new(
                config.paths.clone(),
                config.reader.clone(),
                Default::default(),
            )
            .unwrap(),
            generation_lock: tokio::sync::Mutex::new(()),
            change_set: Mutex::new(ChangeSet {
                generation: SnapshotGeneration(1),
                identity: crate::snapshot::SnapshotIdentity::WorkingTree {
                    fingerprint: "test".into(),
                },
                context: crate::explain::AnalysisContext::working_tree(),
                files: vec![crate::change_set::FileChange {
                    old_path: Some("lib.rs".into()),
                    new_path: Some("lib.rs".into()),
                    kind: crate::change_set::FileChangeKind::Modified,
                    similarity: None,
                    old_mode: Some("100644".into()),
                    new_mode: Some("100644".into()),
                    binary: false,
                    patch: None,
                    before: Some(source.clone()),
                    after: Some(source),
                    semantic_changes,
                }],
            }),
            provider: provider.clone(),
            cache: None,
            model: config.model,
            reader: config.reader,
            explanation: config.explanation,
        });

        let result = generate(id.0, ExplanationKind::Changes, false, None, state)
            .await
            .0;
        assert_eq!(result["code"], "no_explanation_required");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }
}
