//! Opt-in integration tests against real model servers.
//!
//! They intentionally require a running server and a known installed model:
//! `GIT_EXPLAIN_TEST_OLLAMA_URL` / `GIT_EXPLAIN_TEST_OLLAMA_MODEL`, or
//! `GIT_EXPLAIN_TEST_LLAMA_CPP_URL` / `GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL`.

use git_explain::{
    config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile},
    context::ContextControl,
    local_context::{
        recommend, workload_key, LocalContextStatistics, LocalContextTracker, MIN_SAMPLES,
    },
    model::{
        openai::{discover_context_capabilities, OpenAiProvider},
        ExplanationProvider, ExplanationRegion, ExplanationRequest,
    },
};
use reqwest::{Client, StatusCode};
use serde_json::json;
use tempfile::tempdir;

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this live test"))
}

fn native_ollama_base(url: &str) -> &str {
    url.trim_end_matches('/').trim_end_matches("/v1")
}

fn profile(base_url: String, model: String, preset: &str) -> ResolvedProfile {
    ResolvedProfile {
        provider: "openai_compatible".into(),
        preset: Some(preset.into()),
        base_url,
        model,
        api_key_env: None,
        api_key: None,
        context_window: None,
        normal: GenerationConfig {
            reasoning: None,
            max_tokens: None,
            temperature: None,
        },
        deep: GenerationConfig {
            reasoning: None,
            max_tokens: None,
            temperature: None,
        },
    }
}

fn production_provider(base_url: String, model: String, preset: &str) -> OpenAiProvider {
    OpenAiProvider::from_config(
        ResolvedProfile {
            context_window: None,
            normal: GenerationConfig {
                reasoning: None,
                max_tokens: Some(96),
                temperature: Some(0.0),
            },
            deep: GenerationConfig {
                reasoning: None,
                max_tokens: Some(96),
                temperature: Some(0.0),
            },
            ..profile(base_url, model, preset)
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
            max_annotations: 1,
            max_annotation_words: 20,
            explain_language_concepts: false,
            explain_framework_concepts: false,
            infer_intent: false,
        },
    )
}

async fn unload_ollama_model_if_requested(client: &Client, native_base: &str, model: &str) {
    if !std::env::var("GIT_EXPLAIN_TEST_OLLAMA_UNLOAD_AFTER")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE"))
    {
        return;
    }
    let response = client
        .post(format!("{native_base}/api/generate"))
        .json(&json!({"model": model, "keep_alive": 0}))
        .send()
        .await
        .expect("ask Ollama to unload the test model");
    assert!(
        response.status().is_success(),
        "Ollama unload request failed with HTTP {}",
        response.status()
    );
    let ps = client
        .get(format!("{native_base}/api/ps"))
        .send()
        .await
        .expect("query Ollama after unload")
        .json::<serde_json::Value>()
        .await
        .expect("read Ollama runtime models after unload");
    let still_loaded = ps["models"].as_array().is_some_and(|models| {
        models.iter().any(|entry| {
            entry["name"].as_str() == Some(model) || entry["model"].as_str() == Some(model)
        })
    });
    assert!(
        !still_loaded,
        "Ollama kept the test model loaded after keep_alive=0"
    );
    println!("ollama: unloaded test model {model}");
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

fn tracker_request(source: &str) -> ExplanationRequest {
    ExplanationRequest {
        source_unit: source.into(),
        unit_name: "local_unit".into(),
        unit_kind: "function".into(),
        diff: "+ local change".into(),
        language: "Rust".into(),
        git_context: "Change source: live Ollama tracker test".into(),
        regions: vec![ExplanationRegion {
            id: 1,
            start_line: 1,
            end_line: 1,
            source: "value + 1".into(),
        }],
        prior_explanation: None,
        deep: false,
    }
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_OLLAMA_URL and GIT_EXPLAIN_TEST_OLLAMA_MODEL"]
async fn live_ollama_reports_native_context_and_serves_openai_requests() {
    let base_url = required("GIT_EXPLAIN_TEST_OLLAMA_URL");
    let model = required("GIT_EXPLAIN_TEST_OLLAMA_MODEL");
    let client = Client::new();
    let native = native_ollama_base(&base_url);

    let capabilities =
        discover_context_capabilities(&profile(base_url.clone(), model.clone(), "ollama")).await;
    assert_eq!(capabilities.control, ContextControl::FixedRuntime);
    println!(
        "git-explain Ollama discovery: model_max={:?} runtime={:?}",
        capabilities.capacity.model_max, capabilities.capacity.runtime_allocated
    );

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
    unload_ollama_model_if_requested(&client, native, &model).await;
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_OLLAMA_URL and GIT_EXPLAIN_TEST_OLLAMA_MODEL"]
async fn live_ollama_explanation_records_private_context_metadata() {
    let base_url = required("GIT_EXPLAIN_TEST_OLLAMA_URL");
    let model = required("GIT_EXPLAIN_TEST_OLLAMA_MODEL");
    let dir = tempdir().expect("create tracker state directory");
    let tracker = LocalContextTracker::for_user_config(&dir.path().join("config.toml"));
    let sentinel = "SECRET_SOURCE_SENTINEL_94A1";
    let provider = production_provider(base_url.clone(), model.clone(), "ollama")
        .with_context_tracker(tracker.clone(), "live-ollama".into());
    let result = provider
        .explain(tracker_request(&format!(
            "fn local_unit() {{ {sentinel}; }}"
        )))
        .await;
    unload_ollama_model_if_requested(&Client::new(), native_ollama_base(&base_url), &model).await;
    let identity = workload_key(&model, None, Some(96), Some(0.0));
    let records = tracker.records("live-ollama", "ollama", &identity, false);
    assert_eq!(
        records.len(),
        1,
        "one logical explanation must yield one record"
    );
    let record = &records[0];
    assert_eq!(record.success, result.is_ok());
    assert_eq!(record.model, model);
    assert!(record.model_max.is_some());
    assert!(record.runtime_context.is_some());
    assert!(record.ideal_required_context >= record.final_required_context);
    assert!(record.latency_ms.is_some());
    assert!(record.actual_prompt_tokens.is_some());
    assert!(record.generation_duration_ms.is_some());
    let stored = std::fs::read_to_string(tracker.path()).expect("read local tracker state");
    assert!(!stored.contains(sentinel));
    assert!(!stored.contains("live Ollama tracker test"));
    println!(
        "ollama tracker: success={} model_max={:?} runtime={:?} required={} prompt={:?} completion={:?} generation_ms={:?} tokens_per_second={:?}",
        record.success,
        record.model_max,
        record.runtime_context,
        record.ideal_required_context,
        record.actual_prompt_tokens,
        record.actual_completion_tokens
        ,record.generation_duration_ms
        ,record.generation_tokens_per_second
    );
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_OLLAMA_URL and GIT_EXPLAIN_TEST_OLLAMA_MODEL"]
async fn live_ollama_context_recommendation_uses_real_recent_workload() {
    let base_url = required("GIT_EXPLAIN_TEST_OLLAMA_URL");
    let model = required("GIT_EXPLAIN_TEST_OLLAMA_MODEL");
    let dir = tempdir().expect("create tracker state directory");
    let tracker = LocalContextTracker::for_user_config(&dir.path().join("config.toml"));
    let provider = production_provider(base_url.clone(), model.clone(), "ollama")
        .with_context_tracker(tracker.clone(), "live-policy".into());
    let sources = [
        "fn size_small(value: i32) -> i32 { value + 1 }".to_string(),
        format!("fn size_medium() {{\n{}\n}}", "let value = 1;\n".repeat(16)),
        format!("fn size_large() {{\n{}\n}}", "let value = 1;\n".repeat(64)),
    ];
    for source in sources.iter().cycle().take(MIN_SAMPLES) {
        let _ = provider.explain(tracker_request(source)).await;
    }
    let capabilities =
        discover_context_capabilities(&profile(base_url.clone(), model.clone(), "ollama")).await;
    let identity = workload_key(&model, None, Some(96), Some(0.0));
    let records = tracker.records("live-policy", "ollama", &identity, false);
    unload_ollama_model_if_requested(&Client::new(), native_ollama_base(&base_url), &model).await;
    assert_eq!(records.len(), MIN_SAMPLES);
    assert!(records
        .windows(2)
        .any(|pair| pair[0].ideal_required_context != pair[1].ideal_required_context));
    let recommendation = recommend(
        &records,
        capabilities.capacity.runtime_allocated,
        capabilities.capacity.model_max,
    );
    assert!(recommendation.recommended.is_some());
    assert!(recommendation.target.is_some());
    let statistics = LocalContextStatistics::from_records(&records);
    println!("ollama policy: model_max={:?} runtime={:?} requests={} required_p50={} required_p95={} recommended={:?}", capabilities.capacity.model_max, capabilities.capacity.runtime_allocated, records.len(), statistics.required.p50.unwrap_or_default(), statistics.required.p95.unwrap_or_default(), recommendation.recommended);
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_LLAMA_CPP_URL and GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL"]
async fn live_llama_cpp_serves_openai_requests_with_its_startup_context() {
    let base_url = required("GIT_EXPLAIN_TEST_LLAMA_CPP_URL");
    let model = required("GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL");
    let client = Client::new();
    let capabilities =
        discover_context_capabilities(&profile(base_url.clone(), model.clone(), "llama_cpp")).await;
    assert_eq!(capabilities.control, ContextControl::FixedRuntime);
    println!(
        "git-explain llama.cpp discovery: model_max={:?} runtime={:?}",
        capabilities.capacity.model_max, capabilities.capacity.runtime_allocated
    );
    let models = client
        .get(format!("{}/models", base_url.trim_end_matches('/')))
        .send()
        .await
        .expect("query llama.cpp OpenAI model list");
    let status = models.status();
    assert!(
        status.is_success()
            || matches!(
                status,
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
        "llama.cpp /v1/models returned unexpected HTTP {}",
        status
    );
    println!("llama.cpp: model-list status={status}");
    if status.is_success() {
        let models = models
            .json::<serde_json::Value>()
            .await
            .expect("read llama.cpp model list JSON");
        let entry = models["data"].as_array().and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["id"].as_str() == Some(&model))
        });
        println!(
            "llama.cpp: model_max={:?} runtime_configured={:?}",
            entry.and_then(|entry| entry["meta"]["n_ctx_train"].as_u64()),
            entry.and_then(|entry| {
                entry["status"]["args"].as_array().and_then(|args| {
                    args.windows(2).find_map(|pair| {
                        (pair[0].as_str() == Some("--ctx-size"))
                            .then(|| pair[1].as_str())
                            .flatten()
                            .and_then(|value| value.parse::<u64>().ok())
                    })
                })
            })
        );
    }
    println!("llama.cpp: model={model} context_control=fixed_runtime_startup");
    small_openai_request(&client, &base_url, &model).await;
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_LLAMA_CPP_URL and GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL"]
async fn live_llama_cpp_rejects_an_explanation_larger_than_its_fixed_context() {
    let base_url = required("GIT_EXPLAIN_TEST_LLAMA_CPP_URL");
    let model = required("GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL");
    let large_body = "let deliberately_long_identifier_for_context_budget = 1;\n".repeat(1_200);
    let error = production_provider(base_url, model, "llama_cpp")
        .explain(ExplanationRequest {
            source_unit: format!("fn oversized() {{\n{large_body}}}"),
            unit_name: "oversized".into(),
            unit_kind: "function".into(),
            diff: "+ oversized function".into(),
            language: "Rust".into(),
            git_context: "Change source: live fixed-runtime test".into(),
            regions: vec![ExplanationRegion {
                id: 1,
                start_line: 1,
                end_line: 1_200,
                source: large_body,
            }],
            prior_explanation: None,
            deep: false,
        })
        .await
        .expect_err("fixed runtime must reject an oversized explanation locally");
    assert!(format!("{error:#}").contains("fixed/available context"));
}

#[tokio::test]
#[ignore = "requires GIT_EXPLAIN_TEST_LLAMA_CPP_URL and GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL"]
async fn live_llama_cpp_runs_a_small_git_explain_request() {
    let base_url = required("GIT_EXPLAIN_TEST_LLAMA_CPP_URL");
    let model = required("GIT_EXPLAIN_TEST_LLAMA_CPP_MODEL");
    let explanation = production_provider(base_url, model, "llama_cpp")
        .explain(ExplanationRequest {
            source_unit: "fn add_one(value: i32) -> i32 { value + 1 }".into(),
            unit_name: "add_one".into(),
            unit_kind: "function".into(),
            diff: "+ value + 1".into(),
            language: "Rust".into(),
            git_context: "Change source: live provider test".into(),
            regions: vec![ExplanationRegion {
                id: 1,
                start_line: 1,
                end_line: 1,
                source: "value + 1".into(),
            }],
            prior_explanation: None,
            deep: false,
        })
        .await
        .expect("small git-explain request must produce structured output");
    assert!(!explanation.overview.trim().is_empty());
    println!(
        "git-explain llama.cpp result: overview={:?}",
        explanation.overview
    );
}
