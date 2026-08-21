use crate::{
    config::ServerConfig,
    explain::ExplainedUnit,
    model::{ExplanationProvider, ExplanationRequest},
    web,
};
use anyhow::Result;
use axum::{
    extract::Path,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use std::sync::Arc;

struct State {
    items: Vec<ExplainedUnit>,
    context: crate::explain::AnalysisContext,
    provider: Arc<dyn ExplanationProvider>,
}
pub async fn serve(
    items: Vec<ExplainedUnit>,
    provider: impl ExplanationProvider + 'static,
    context: crate::explain::AnalysisContext,
    config: ServerConfig,
) -> Result<()> {
    let state = Arc::new(State {
        items,
        context,
        provider: Arc::new(provider),
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/units/{id}/deep", post(deep))
        .route("/api/deep/{id}", post(deep))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    let url = format!("http://{}", listener.local_addr()?);
    println!("git explain: {}", url);
    if config.open_browser {
        let _ = webbrowser::open(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}
async fn index(axum::extract::State(state): axum::extract::State<Arc<State>>) -> Html<String> {
    Html(web::render(&state.items, &state.context))
}
async fn deep(
    Path(id): Path<usize>,
    axum::extract::State(state): axum::extract::State<Arc<State>>,
) -> Json<serde_json::Value> {
    let Some(item) = state.items.get(id) else {
        return Json(serde_json::json!({"ok":false,"deep":"Unknown code unit."}));
    };
    let request = ExplanationRequest {
        source_unit: item.unit.source.clone(),
        unit_name: item.unit.name.clone(),
        unit_kind: format!("{:?}", item.unit.kind),
        diff: item.diff.clone(),
        language: item.language.clone(),
        git_context: state.context.prompt_context(),
        regions: item.regions.clone(),
        prior_explanation: Some(item.explanation.overview.clone()),
        deep: true,
    };
    match state.provider.explain(request).await {
        Ok(e) => Json(serde_json::json!({"ok":true,"deep":e.deep.unwrap_or(e.overview)})),
        Err(error) => {
            eprintln!(
                "deep explanation request failed for {}: {error:#}",
                item.unit.name
            );
            Json(serde_json::json!({"ok":false,"deep":"Detailed explanation unavailable."}))
        }
    }
}
