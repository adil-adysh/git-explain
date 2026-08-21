use crate::{
    config::ServerConfig,
    explain::ExplainedFunction,
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
    items: Vec<ExplainedFunction>,
    provider: Arc<dyn ExplanationProvider>,
}
pub async fn serve(
    items: Vec<ExplainedFunction>,
    provider: impl ExplanationProvider + 'static,
    config: ServerConfig,
) -> Result<()> {
    let state = Arc::new(State {
        items,
        provider: Arc::new(provider),
    });
    let app = Router::new()
        .route("/", get(index))
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
    Html(web::render(&state.items))
}
async fn deep(
    Path(id): Path<usize>,
    axum::extract::State(state): axum::extract::State<Arc<State>>,
) -> Json<serde_json::Value> {
    let Some(item) = state.items.get(id) else {
        return Json(serde_json::json!({"ok":false,"deep":"Unknown function."}));
    };
    let request = ExplanationRequest {
        function: item.symbol.source.clone(),
        diff: String::new(),
        language: item.language.clone(),
        deep: true,
    };
    match state.provider.explain(request).await {
        Ok(e) => Json(serde_json::json!({"ok":true,"deep":e.deep.unwrap_or(e.overview)})),
        Err(_) => Json(serde_json::json!({"ok":false,"deep":"Detailed explanation unavailable."})),
    }
}
