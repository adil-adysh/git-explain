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
    Html(web::render_file_analysis(&state.analysis.lock().unwrap()))
}

async fn file_explain(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, false, false, state).await
}
async fn file_concepts(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, false, true, state).await
}
async fn file_concepts_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, true, true, state).await
}

async fn file_deep(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, true, false, false, state).await
}

async fn file_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, false, true, false, state).await
}

async fn file_deep_regenerate(
    Path(id): Path<String>,
    State(state): State<Arc<FileStateData>>,
) -> Json<serde_json::Value> {
    generate_file(id, true, true, false, state).await
}

async fn generate_file(
    id: String,
    deep: bool,
    regenerate: bool,
    concepts: bool,
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
        if concepts {
            file_analysis::concept_request_for(unit, analysis.revision.as_deref())
        } else {
            file_analysis::request_for(unit, analysis.revision.as_deref(), deep, prior)
        }
    };
    let key =
        ExplanationCache::snapshot_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    if concepts {
                        state
                            .analysis
                            .lock()
                            .unwrap()
                            .apply_concepts(&id, result.clone());
                    } else {
                        state
                            .analysis
                            .lock()
                            .unwrap()
                            .apply(&id, result.clone(), deep);
                    }
                    return Json(runtime::result(result, true, request.kind));
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
            if concepts {
                state
                    .analysis
                    .lock()
                    .unwrap()
                    .apply_concepts(&id, result.clone());
            } else {
                state
                    .analysis
                    .lock()
                    .unwrap()
                    .apply(&id, result.clone(), deep);
            }
            Json(runtime::result(result, false, request.kind))
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
        if !change.kind.explainable() {
            return Json(
                serde_json::json!({"ok":false,"code":"no_explanation_required","error":"This semantic unit is unchanged and requires no model explanation.","retryable":false}),
            );
        }
        (
            changes.generation,
            runtime::request_for_kind(file, change, &changes.context, kind),
        )
    };
    let key =
        ExplanationCache::change_key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    if apply(&state, &id, generation, result.clone(), kind) {
                        return Json(runtime::result(result, true, kind));
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
            if apply(&state, &id, generation, result.clone(), kind) {
                Json(runtime::result(result, false, kind))
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
    kind: ExplanationKind,
) -> bool {
    let mut changes = state.change_set.lock().unwrap();
    if changes.generation != generation {
        return false;
    }
    let Some(change) = changes.find_change_mut(id) else {
        return false;
    };
    runtime::apply_kind(change, result, kind);
    true
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
                overview: format!("result {call}"),
                annotations: vec![crate::model::Annotation {
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
