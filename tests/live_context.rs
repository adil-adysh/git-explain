//! Opt-in integration tests against real model servers.
//!
//! They intentionally require a running server and a known installed model:
//! `GIT_EXPLAIN_TEST_OLLAMA_URL` / `GIT_EXPLAIN_TEST_OLLAMA_MODEL`, or
//! `GIT_EXPLAIN_TEST_LLAMA_CPP_URL` / `GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL`.

use reqwest::{Client, StatusCode};
use serde_json::json;

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this live test"))
}

fn native_ollama_base(url: &str) -> &str {
    url.trim_end_matches('/').trim_end_matches("/v1")
}

async fn assert_small_openai_request(client: &Client, base_url: &str, model: &str) {
    let response = client
        .post(format!(
            "{}/chat/completions",
            base_url.trim_end_matches('/')
        ))
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "Reply with OK."}],
            "max_tokens": 1,
        }))
        .send()
        .await
        .expect("send source-free OpenAI-compatible request");
    assert!(
        response.status().is_success(),
        "source-free inference failed: HTTP {}: {}",
        response.status(),
        response.text().await.unwrap_or_default()
    );
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_OLLAMA_URL and GIT_EXPLAIN_TEST_OLLAMA_MODEL"]
async fn live_ollama_reports_native_context_and_serves_openai_requests() {
    let base_url = required("GIT_EXPLAIN_TEST_OLLAMA_URL");
    let model = required("GIT_EXPLAIN_TEST_OLLAMA_MODEL");
    let client = Client::new();
    let native = native_ollama_base(&base_url);

    let show = client
        .post(format!("{native}/api/show"))
        .json(&json!({"model": model}))
        .send()
        .await
        .expect("query Ollama model metadata");
    assert!(show.status().is_success(), "Ollama /api/show must succeed");

    let ps = client
        .get(format!("{native}/api/ps"))
        .send()
        .await
        .expect("query Ollama runtime models");
    assert_eq!(ps.status(), StatusCode::OK, "Ollama /api/ps must succeed");

    assert_small_openai_request(&client, &base_url, &model).await;
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_LLAMA_CPP_URL and GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL"]
async fn live_llama_cpp_serves_openai_requests_with_its_startup_context() {
    let base_url = required("GIT_EXPLAIN_TEST_LLAMA_CPP_URL");
    let model = required("GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL");
    let client = Client::new();
    let models = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .send()
        .await
        .expect("query llama.cpp OpenAI model list");
    assert!(
        models.status().is_success(),
        "llama.cpp /v1/models must succeed"
    );
    assert_small_openai_request(&client, &base_url, &model).await;
}
