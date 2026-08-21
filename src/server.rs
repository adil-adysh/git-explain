use crate::{
    cache::ExplanationCache,
    config::{ExplanationConfig, ModelConfig, ReaderConfig, ServerConfig},
    explain::{AnalysisContext, ExplainedUnit},
    model::{ExplanationProvider, ExplanationRequest, UnitExplanation},
    web,
};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use std::sync::{Arc, Mutex};

struct StateData {
    items: Mutex<Vec<ExplainedUnit>>,
    context: AnalysisContext,
    provider: Arc<dyn ExplanationProvider>,
    cache: Option<ExplanationCache>,
    model: ModelConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
pub async fn serve(
    items: Vec<ExplainedUnit>,
    provider: impl ExplanationProvider + 'static,
    context: AnalysisContext,
    config: ServerConfig,
    cache: Option<ExplanationCache>,
    model: ModelConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
) -> Result<()> {
    let mut items = items;
    if let Some(cache_ref) = &cache {
        hydrate(
            &mut items,
            cache_ref,
            &context,
            &model,
            &reader,
            &explanation,
        );
    }
    let state = Arc::new(StateData {
        items: Mutex::new(items),
        context,
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
fn hydrate(
    items: &mut [ExplainedUnit],
    cache: &ExplanationCache,
    context: &AnalysisContext,
    model: &ModelConfig,
    reader: &ReaderConfig,
    explanation: &ExplanationConfig,
) {
    for item in items {
        for deep in [false, true] {
            let request = ExplanationRequest {
                source_unit: item.unit.source.clone(),
                unit_name: item
                    .unit
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| item.unit.name.clone()),
                unit_kind: format!("{:?}", item.unit.kind),
                diff: item.diff.clone(),
                language: item.language.clone(),
                git_context: context.prompt_context(),
                regions: item.regions.clone(),
                prior_explanation: deep
                    .then_some(item.explanation.overview.clone())
                    .filter(|s| !s.is_empty()),
                deep,
            };
            let key = ExplanationCache::key(&request, model, reader, explanation);
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(e) = serde_json::from_str::<UnitExplanation>(&json) {
                    if deep {
                        item.deep_explanation =
                            e.deep.or((!e.overview.is_empty()).then_some(e.overview));
                    } else {
                        item.explanation = e;
                    }
                }
            }
        }
    }
}
async fn index(State(state): State<Arc<StateData>>) -> Html<String> {
    Html(web::render(&state.items.lock().unwrap(), &state.context))
}
async fn explain(
    Path(id): Path<usize>,
    State(state): State<Arc<StateData>>,
) -> Json<serde_json::Value> {
    generate(id, false, false, state).await
}
async fn deep(
    Path(id): Path<usize>,
    State(state): State<Arc<StateData>>,
) -> Json<serde_json::Value> {
    generate(id, true, false, state).await
}
async fn deep_regenerate(
    Path(id): Path<usize>,
    State(state): State<Arc<StateData>>,
) -> Json<serde_json::Value> {
    generate(id, true, true, state).await
}
async fn regenerate(
    Path(id): Path<usize>,
    State(state): State<Arc<StateData>>,
) -> Json<serde_json::Value> {
    generate(id, false, true, state).await
}
async fn generate(
    id: usize,
    deep: bool,
    regenerate: bool,
    state: Arc<StateData>,
) -> Json<serde_json::Value> {
    let item = { state.items.lock().unwrap().get(id).cloned() };
    let Some(item) = item else {
        return Json(serde_json::json!({"ok":false,"error":"Unknown code unit."}));
    };
    let request = ExplanationRequest {
        source_unit: item.unit.source.clone(),
        unit_name: item
            .unit
            .qualified_name
            .clone()
            .unwrap_or_else(|| item.unit.name.clone()),
        unit_kind: format!("{:?}", item.unit.kind),
        diff: item.diff.clone(),
        language: item.language.clone(),
        git_context: state.context.prompt_context(),
        regions: item.regions.clone(),
        prior_explanation: deep
            .then_some(item.explanation.overview.clone())
            .filter(|s| !s.is_empty()),
        deep,
    };
    let key = ExplanationCache::key(&request, &state.model, &state.reader, &state.explanation);
    if !regenerate {
        if let Some(cache) = &state.cache {
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(e) = serde_json::from_str::<UnitExplanation>(&json) {
                    update(&state, id, e.clone(), deep);
                    return Json(result(e, true, deep));
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
            update(&state, id, e.clone(), deep);
            Json(result(e, false, deep))
        }
        Err(error) => {
            eprintln!("unit {} explanation failed: {error:#}", id);
            Json(serde_json::json!({"ok":false,"error":"Explanation unavailable."}))
        }
    }
}
fn update(state: &StateData, id: usize, e: UnitExplanation, deep: bool) {
    if let Some(item) = state.items.lock().unwrap().get_mut(id) {
        if deep {
            item.deep_explanation = e.deep.or((!e.overview.is_empty()).then_some(e.overview));
        } else {
            item.explanation = e;
        }
    }
}
fn result(e: UnitExplanation, hit: bool, deep: bool) -> serde_json::Value {
    serde_json::json!({"ok":true,"cache_hit":hit,"overview":e.overview,"annotations":e.annotations,"deep":e.deep,"mode":if deep{"deep"}else{"normal"}})
}
