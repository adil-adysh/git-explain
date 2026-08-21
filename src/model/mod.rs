pub mod openai;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct ExplanationRequest {
    pub function: String,
    pub diff: String,
    pub language: String,
    pub git_context: String,
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
pub struct FunctionExplanation {
    pub overview: String,
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub deep: Option<String>,
}
#[async_trait]
pub trait ExplanationProvider: Send + Sync {
    async fn explain(&self, request: ExplanationRequest) -> Result<FunctionExplanation>;
}
