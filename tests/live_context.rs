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

async fn small_openai_request(client: &Client, base_url: &str, model: &str) -> serde_json::Value {
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
    let body = response
        .json::<serde_json::Value>()
        .await
        .expect("read inference JSON");
    println!(
        "inference: finish_reason={} prompt_tokens={} completion_tokens={}",
        body["choices"][0]["finish_reason"],
        body["usage"]["prompt_tokens"],
        body["usage"]["completion_tokens"],
    );
    body
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
    let show = show
        .json::<serde_json::Value>()
        .await
        .expect("read Ollama /api/show JSON");
    let model_max = show["model_info"].as_object().and_then(|info| {
        info.iter().find_map(|(key, value)| {
            key.ends_with(".context_length")
                .then(|| value.as_u64())
                .flatten()
        })
    });
    println!("ollama: model={model} model_max={model_max:?} context_control=fixed_runtime_v1");

    let ps = client
        .get(format!("{native}/api/ps"))
        .send()
        .await
        .expect("query Ollama runtime models");
    assert_eq!(ps.status(), StatusCode::OK, "Ollama /api/ps must succeed");

    let response = small_openai_request(&client, &base_url, &model).await;
    let ps_after = client
        .get(format!("{native}/api/ps"))
        .send()
        .await
        .expect("query Ollama runtime models after inference")
        .json::<serde_json::Value>()
        .await
        .expect("read Ollama /api/ps JSON");
    let runtime = ps_after["models"].as_array().and_then(|models| {
        models
            .iter()
            .find(|entry| {
                entry["name"].as_str() == Some(&model) || entry["model"].as_str() == Some(&model)
            })
            .and_then(|entry| entry["context_length"].as_u64())
    });
    println!("ollama: runtime_allocated_after_inference={runtime:?}");
    if let (Some(runtime), Some(prompt)) = (runtime, response["usage"]["prompt_tokens"].as_u64()) {
        assert!(
            prompt < runtime,
            "reported prompt plus output reserve must fit runtime context"
        );
    }
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
    println!("llama.cpp: model={model} context_control=fixed_runtime_startup");
    small_openai_request(&client, &base_url, &model).await;
}
