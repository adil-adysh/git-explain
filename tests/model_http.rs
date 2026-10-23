use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
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
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::oneshot;

fn provider(base_url: String, timeout: Duration) -> OpenAiProvider {
    provider_with_limit(base_url, timeout, None)
}

fn provider_with_limit(
    base_url: String,
    timeout: Duration,
    context_window: Option<u32>,
) -> OpenAiProvider {
    OpenAiProvider::from_config_with_timeout(
        ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: Some("llama_cpp".into()),
            base_url,
            model: "unsloth-test".into(),
            api_key_env: None,
            api_key: None,
            context_window,
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

fn ollama_provider(base_url: String, timeout: Duration) -> OpenAiProvider {
    OpenAiProvider::from_config_with_timeout(
        ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: Some("ollama".into()),
            base_url,
            model: "ollama-test".into(),
            api_key_env: None,
            api_key: None,
            context_window: None,
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
        prerequisite_id: None,
        policy: git_explain::learning::LearningContext {
            config: Default::default(),
            repository_defaults: None,
            reader: ReaderConfig {
                experience: "experienced".into(),
                known_languages: vec![],
                learning_languages: vec![],
                known_frameworks: vec![],
                learning_frameworks: vec![],
            },
        }
        .resolve("Rust", &Default::default())
        .unwrap(),
        source_unit: "fn add(a: i32, b: i32) -> i32 { a + b }".into(),
        unit_name: "add".into(),
        unit_kind: "Function".into(),
        diff: "+ a + b".into(),
        language: "Rust".into(),
        git_context: "Change source: test".into(),
        regions: vec![ExplanationRegion {
            id: 1,
            coverage_id: 1,
            start_line: 1,
            end_line: 1,
            source: "a + b".into(),
        }],
        prior_explanation: None,
        kind: git_explain::model::ExplanationKind::Changes,
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
    let (url, shutdown, _) = serve_counted_response(response).await;
    (url, shutdown)
}

async fn serve_counted_response(
    response: serde_json::Value,
) -> (String, oneshot::Sender<()>, Arc<AtomicUsize>) {
    serve_counted_status(response, StatusCode::OK).await
}

async fn serve_counted_status(
    response: serde_json::Value,
    status: StatusCode,
) -> (String, oneshot::Sender<()>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            let response = response.clone();
            async move { (status, axum::Json(response)).into_response() }
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
    (format!("http://{}/v1", address), shutdown, calls)
}

#[tokio::test]
async fn provider_context_failure_records_pressure_after_one_request() {
    let (url, shutdown, calls) = serve_counted_status(
        json!({"error":"maximum context length exceeded"}),
        StatusCode::BAD_REQUEST,
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let history = directory.path().join("context.json");
    let tracker = git_explain::local_context::LocalContextTracker::at(history.clone());
    let error = provider(url, Duration::from_secs(2))
        .with_context_tracker(tracker, "evaluation".into())
        .explain(request(false))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("maximum context length"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(history).expect("provider pressure is recorded"))
            .unwrap();
    assert_eq!(history["records"][0]["success"], false);
    assert_eq!(history["records"][0]["provider_context_overflow"], true);
    assert_eq!(history["records"][0]["attempts"], 1);
    let _ = shutdown.send(());
}

#[tokio::test]
async fn preflight_failure_records_pressure_without_generating_text() {
    let (url, shutdown, calls) = serve_counted_response(json!({})).await;
    let directory = tempfile::tempdir().unwrap();
    let history = directory.path().join("context.json");
    let tracker = git_explain::local_context::LocalContextTracker::at(history.clone());
    let error = provider_with_limit(url, Duration::from_secs(2), Some(1))
        .with_context_tracker(tracker, "evaluation".into())
        .explain(request(false))
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("context requirement"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(history).expect("preflight failure is recorded"))
            .unwrap();
    let records = history["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["success"], false);
    assert_eq!(records[0]["local_context_failure"], true);
    assert!(records[0]["ideal_required_context"].as_u64().unwrap() > 1);
    let _ = shutdown.send(());
}

#[tokio::test]
async fn accepts_structured_content_and_ignores_separate_reasoning() {
    let (base_url, shutdown) = serve_response(json!({
        "choices": [{
            "finish_reason": "stop",
            "message": {
                "content": "{\"overview\":\"Adds two values.\",\"annotations\":[{\"region\":1,\"source_quote\":\"a + b\",\"kind\":\"change\",\"text\":\"The expression is the return value.\"}]}",
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
async fn unloaded_ollama_model_is_warmed_before_the_explanation_payload() {
    let ps_calls = Arc::new(AtomicUsize::new(0));
    let request_bodies = Arc::new(Mutex::new(Vec::new()));
    let ps_seen = ps_calls.clone();
    let bodies_seen = request_bodies.clone();
    let app = Router::new()
        .route(
            "/api/ps",
            get(move || {
                let call = ps_seen.fetch_add(1, Ordering::SeqCst);
                async move {
                    let models = (call > 0).then(|| json!([{
                        "name": "ollama-test",
                        "context_length": 4096,
                    }]));
                    axum::Json(json!({"models": models.unwrap_or_else(|| json!([]))}))
                }
            }),
        )
        .route(
            "/api/show",
            post(|| async { axum::Json(json!({"model_info": {"test.context_length": 32768}})) }),
        )
        .route(
            "/api/chat",
            post(move |Json(body): Json<serde_json::Value>| {
                let bodies_seen = bodies_seen.clone();
                async move {
                    let loading = body["messages"].as_array().is_some_and(|messages| messages.is_empty());
                    bodies_seen.lock().unwrap().push(body);
                    if loading {
                        return axum::Json(json!({"message":{"role":"assistant","content":""},"done_reason":"load"}));
                    }
                    axum::Json(json!({
                        "message": {"role":"assistant", "content": "{\"overview\":\"Adds values.\",\"annotations\":[]}"},
                        "done_reason": "stop",
                        "prompt_eval_count": 20,
                        "eval_count": 10
                    }))
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

    let result = ollama_provider(format!("http://{address}/v1"), Duration::from_secs(2))
        .explain(request(false))
        .await
        .unwrap();
    assert_eq!(result.overview, "Adds values.");
    assert!(ps_calls.load(Ordering::SeqCst) >= 2);
    let bodies = request_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        bodies
            .iter()
            .filter(|body| !body["messages"].as_array().unwrap().is_empty())
            .count(),
        1
    );
    assert_eq!(bodies[0]["messages"], json!([]));
    assert!(bodies[0].get("options").is_none());
    assert!(bodies[1]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("SOURCE UNIT"));
    assert!(bodies[1]["format"].is_object());
    assert_eq!(bodies[1]["stream"], false);
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
async fn malformed_output_returns_error_without_an_automatic_model_retry() {
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
    let directory = tempfile::tempdir().unwrap();
    let history = directory.path().join("context.json");
    let tracker = git_explain::local_context::LocalContextTracker::at(history.clone());
    let result = provider(format!("http://{}/v1", address), Duration::from_secs(2))
        .with_context_tracker(tracker, "evaluation".into())
        .explain(request(false))
        .await
        .unwrap_err();
    assert!(format!("{result:#}").contains("invalid structured model content"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let history: serde_json::Value =
        serde_json::from_slice(&std::fs::read(history).expect("first-attempt failure is recorded"))
            .unwrap();
    let records = history["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["success"], false);
    assert_eq!(records[0]["attempts"], 1);
    assert!(records[0]["latency_ms"].is_number());
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

#[test]
fn local_context_negotiation_failures_have_actionable_ui_errors() {
    for message in [
        "context requirement 13733 tokens exceeds fixed/available context 8192 tokens",
        "context requirement 13733 tokens exceeds configured/model limit 8192 tokens",
        "context requirement 13733 tokens cannot fit within the requested-context limit 8192 tokens",
    ] {
        let error = user_facing_error(&anyhow::anyhow!(message).context("generate Changes"));
        assert_eq!(error.code, "context_exceeded", "{message}");
        assert!(!error.retryable);
        assert!(error.message.contains("context allocation"));
    }
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

#[tokio::test]
async fn shared_provider_keeps_each_requests_learning_policy() {
    use git_explain::learning::{Focus, LanguageHelp, LearningContext};
    let observed = Arc::new(Mutex::new(Vec::new()));
    let capture = observed.clone();
    let app = Router::new().route("/v1/chat/completions", post(move |Json(body): Json<serde_json::Value>| {
        let capture = capture.clone();
        async move {
            let prompt = body["messages"][1]["content"].as_str().unwrap().to_string();
            let overview = if prompt.contains("Skip elementary syntax") { "Traces execution." } else { "Teaches syntax." };
            capture.lock().unwrap().push(prompt);
            Json(json!({"choices":[{"finish_reason":"stop","message":{"content":json!({"overview":overview,"annotations":[]}).to_string()}}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let provider = provider(format!("http://{address}/v1"), Duration::from_secs(2));
    let context = LearningContext {
        config: Default::default(),
        repository_defaults: None,
        reader: ReaderConfig {
            experience: "experienced".into(),
            known_languages: vec![],
            learning_languages: vec![],
            known_frameworks: vec![],
            learning_frameworks: vec![],
        },
    };
    let mut first = request(false);
    let mut policy = context.resolve("Rust", &Default::default()).unwrap();
    policy.language_help = LanguageHelp::Explain;
    policy.focus = Focus::Architecture;
    first.policy = policy.clone();
    assert_eq!(
        provider.explain(first).await.unwrap().overview,
        "Teaches syntax."
    );
    let mut second = request(false);
    policy.language_help = LanguageHelp::Skip;
    policy.focus = Focus::Execution;
    second.policy = policy;
    assert_eq!(
        provider.explain(second).await.unwrap().overview,
        "Traces execution."
    );
    let prompts = observed.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].contains("architectural relationships"));
    assert!(prompts[1].contains("Trace execution in order"));
    server.abort();
}
