pub mod openai;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct ExplanationRegion {
    pub id: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct ExplanationRequest {
    pub source_unit: String,
    pub unit_name: String,
    pub unit_kind: String,
    pub diff: String,
    pub language: String,
    pub git_context: String,
    pub regions: Vec<ExplanationRegion>,
    pub prior_explanation: Option<String>,
    pub deep: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Annotation {
    pub start_line: usize,
    pub end_line: usize,
    pub kind: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitExplanation {
    pub overview: String,
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub deep: Option<String>,
}

/// Safe, stable categories for the local HTTP API. Provider details stay in logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserFacingError {
    pub code: &'static str,
    pub message: &'static str,
    pub retryable: bool,
}

pub fn user_facing_error(error: &anyhow::Error) -> UserFacingError {
    let text = format!("{error:#}").to_ascii_lowercase();
    let request_error = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<reqwest::Error>());
    if request_error.is_some_and(reqwest::Error::is_timeout)
        || text.contains("timed out")
        || text.contains("deadline has elapsed")
    {
        return UserFacingError {
            code: "timeout",
            message:
                "The model took too long to respond. Try again or reduce the requested detail.",
            retryable: true,
        };
    }
    if request_error.is_some_and(reqwest::Error::is_connect)
        || text.contains("connection refused")
        || text.contains("failed to connect")
    {
        return UserFacingError {
            code: "model_unavailable",
            message:
                "The configured model server is unavailable. Start it and check the model URL.",
            retryable: true,
        };
    }
    if text.contains("http 401") || text.contains("http 403") || text.contains("unauthorized") {
        return UserFacingError {
            code: "model_auth",
            message: "The model server rejected the credentials. Check the configured API key.",
            retryable: false,
        };
    }
    if text.contains("http 429") || text.contains("rate limit") {
        return UserFacingError {
            code: "model_busy",
            message: "The model server is busy or rate-limited. Wait a moment and try again.",
            retryable: true,
        };
    }
    if text.contains("invalid url")
        || text.contains("relative url")
        || text.contains("builder error")
        || text.contains("base URL")
        || text.contains("base url")
    {
        return UserFacingError {
            code: "model_configuration",
            message: "The model URL configuration is invalid. Check the configured model endpoint.",
            retryable: false,
        };
    }
    if text.contains("model not found")
        || text.contains("no such model")
        || text.contains("unknown model")
        || text.contains("model unavailable")
        || text.contains("service unavailable")
        || text.contains("temporarily unavailable")
        || text.contains("does not exist")
        || text.contains("http 404")
        || text.contains("http 502")
        || text.contains("http 503")
        || text.contains("http 504")
    {
        return UserFacingError {
            code: "model_unavailable",
            message:
                "The configured model is not available on the model server. Check the model name.",
            retryable: false,
        };
    }
    UserFacingError { code: "model_failed", message: "The model could not generate an explanation. Check the model server logs and try again.", retryable: true }
}

#[async_trait]
pub trait ExplanationProvider: Send + Sync {
    async fn explain(&self, request: ExplanationRequest) -> Result<UnitExplanation>;
}
