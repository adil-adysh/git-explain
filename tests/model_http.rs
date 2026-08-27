use axum::{http::StatusCode, response::IntoResponse, routing::post, Router};
use git_explain::config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile};
use git_explain::model::openai::OpenAiProvider;
use git_explain::model::{
    user_facing_error, ExplanationProvider, ExplanationRegion, ExplanationRequest,
};
use serde_json::json;
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::oneshot;

fn provider(base_url: String, timeout: Duration) -> OpenAiProvider {
    OpenAiProvider::from_config_with_timeout(
        ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: Some("llama_cpp".into()),
            base_url,
            model: "unsloth-test".into(),
            api_key_env: None,
            api_key: None,
            normal: GenerationConfig {
                reasoning: Some(false),
                max_tokens: Some(500),
                temperature: Some(0.2),
            },
            deep: GenerationConfig {
                reasoning: Some(true),
                max_tokens: Some(2500),
                temperature: Some(0.3),
            },
        },
        ReaderConfig {
            experience: "experienced".into(),
            known_languages: vec![],
            learning_languages: vec![],
            known_frameworks: vec![],
            learning_frameworks: vec![],
        },
        ExplanationConfig {
            default_depth: "normal".into(),
            max_annotations: 3,
            max_annotation_words: 60,
            explain_language_concepts: true,
            explain_framework_concepts: true,
            infer_intent: false,
        },
        timeout,
    )
}

fn request(deep: bool) -> ExplanationRequest {
    ExplanationRequest {
        source_unit: "fn add(a: i32, b: i32) -> i32 { a + b }".into(),
        unit_name: "add".into(),
        unit_kind: "Function".into(),
        diff: "+ a + b".into(),
        language: "Rust".into(),
        git_context: "Change source: test".into(),
        regions: vec![ExplanationRegion {
            id: 1,
            start_line: 1,
            end_line: 1,
            source: "a + b".into(),
        }],
        prior_explanation: None,
        deep,
    }
}

#[tokio::test]
async fn whitespace_only_source_is_rejected_before_http_request() {
    let provider = provider("http://127.0.0.1:1/v1".into(), Duration::from_millis(50));
    let mut request = request(false);
    request.source_unit = "\r\n\t  ".into();
    let error = provider.explain(request).await.unwrap_err();
    assert!(format!("{error:#}").contains("whitespace-only source"));
}

async fn serve_response(response: serde_json::Value) -> (String, oneshot::Sender<()>) {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let response = response.clone();
            async move { axum::Json(response) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, signal) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = signal.await;
            })
            .await
            .unwrap();
    });
    (format!("http://{}/v1", address), shutdown)
}

#[tokio::test]
async fn accepts_structured_content_and_ignores_separate_reasoning() {
    let (base_url, shutdown) = serve_response(json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {
                "content": "{\"overview\":\"Adds two values.\",\"annotations\":[{\"region\":1,\"kind\":\"change\",\"text\":\"The expression is the return value.\"}]}",
                "reasoning_content": "private reasoning"
            }
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 15}
    }))
    .await;
    let result = provider(base_url, Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap();
    assert_eq!(result.overview, "Adds two values.");
    assert_eq!(result.annotations.len(), 1);
    let _ = shutdown.send(());
}

#[tokio::test]
async fn rejects_truncated_structured_response_with_usage() {
    let (base_url, shutdown) = serve_response(json!({
        "choices": [{
            "finish_reason": "length",
            "message": {"content": "{\"overview\":\"incomplete"}
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 500}
    }))
    .await;
    let error = provider(base_url, Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("truncated at token limit"));
    assert!(error.contains("completion_tokens=Some(500)"));
    let _ = shutdown.send(());
}

#[tokio::test]
async fn reports_invalid_json_with_finish_and_usage_metadata() {
    let (base_url, shutdown) = serve_response(json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {"content": "{\"overview\":\"unfinished"}
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 42}
    }))
    .await;
    let error = provider(base_url, Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid structured model content"));
    assert!(error.contains("finish_reason=Some(\"stop\")"));
    assert!(error.contains("completion_tokens=Some(42)"));
    let _ = shutdown.send(());
}

#[tokio::test]
async fn retries_malformed_structured_output_once_then_accepts_valid_output() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let attempt = seen.fetch_add(1, Ordering::SeqCst);
            async move {
                let content = if attempt == 0 {
                    "not json"
                } else {
                    r#"{"overview":"Recovered.","annotations":[]}"#
                };
                axum::Json(
                    json!({"choices":[{"finish_reason":"stop","message":{"content":content}}]}),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, signal) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = signal.await;
            })
            .await
            .unwrap();
    });
    let result = provider(format!("http://{}/v1", address), Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap();
    assert_eq!(result.overview, "Recovered.");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let _ = shutdown.send(());
}

#[tokio::test]
async fn rejects_empty_content_when_only_reasoning_is_returned() {
    let response = json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {"content": "", "reasoning_content": "still thinking"}
        }]
    });
    let (base_url, shutdown) = serve_response(response).await;
    let error = provider(base_url, Duration::from_secs(2))
        .explain(request(true))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("reasoning separately but no visible answer"));
    let _ = shutdown.send(());
}

#[tokio::test]
async fn reports_http_errors() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "server error").into_response() }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, signal) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = signal.await;
            })
            .await
            .unwrap();
    });
    let error = provider(format!("http://{}/v1", address), Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("model response"));
    let _ = shutdown.send(());
}

#[test]
fn categorizes_model_failures_for_the_ui() {
    let unavailable = user_facing_error(&anyhow::anyhow!(
        "model response (HTTP 404): model not found"
    ));
    assert_eq!(unavailable.code, "model_unavailable");
    assert!(!unavailable.retryable);

    let timeout = user_facing_error(&anyhow::anyhow!("model request: operation timed out"));
    assert_eq!(timeout.code, "timeout");
    assert!(timeout.retryable);

    let auth = user_facing_error(&anyhow::anyhow!("model response (HTTP 401): unauthorized"));
    assert_eq!(auth.code, "model_auth");
    assert!(!auth.retryable);

    let busy = user_facing_error(&anyhow::anyhow!("model response (HTTP 429): rate limit"));
    assert_eq!(busy.code, "model_busy");
    assert!(busy.retryable);
}

#[tokio::test]
async fn reports_timeout() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, Infallible>(axum::Json(json!({})))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, signal) = oneshot::channel();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = signal.await;
            })
            .await
            .unwrap();
    });
    let error = provider(format!("http://{}/v1", address), Duration::from_millis(10))
        .explain(request(false))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("model request"));
    let _ = shutdown.send(());
}
