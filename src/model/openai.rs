use super::*;
use crate::config::{ExplanationConfig, ModelConfig, ReaderConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone)]
pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    normal: crate::config::GenerationConfig,
    deep: crate::config::GenerationConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}
impl OpenAiProvider {
    pub fn from_config(
        model: ModelConfig,
        reader: ReaderConfig,
        explanation: ExplanationConfig,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: model.base_url,
            model: model.model,
            api_key: model.api_key,
            normal: model.normal,
            deep: model.deep,
            reader,
            explanation,
        }
    }
}
#[derive(Serialize)]
struct Req {
    model: String,
    messages: Vec<Msg>,
    temperature: f32,
    max_tokens: u32,
    response_format: ResponseFormat,
    chat_template_kwargs: Value,
}
#[derive(Serialize)]
struct Msg {
    role: String,
    content: String,
}
#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}
#[derive(Deserialize)]
struct Resp {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: MsgOut,
}
#[derive(Deserialize)]
struct MsgOut {
    content: String,
}
#[async_trait]
impl ExplanationProvider for OpenAiProvider {
    async fn explain(&self, r: ExplanationRequest) -> Result<FunctionExplanation> {
        let generation = if r.deep { &self.deep } else { &self.normal };
        let task = if r.deep {
            "Return JSON with a concise field `overview`, an empty `annotations` array, and a `deep` field containing a step-by-step explanation."
        } else {
            "Return JSON with `overview` and annotations; annotation lines are relative to the supplied function."
        };
        let reader_context = if self.reader.experience != "experienced"
            || !self.reader.known_languages.is_empty()
            || !self.reader.learning_languages.is_empty()
            || !self.reader.known_frameworks.is_empty()
            || !self.reader.learning_frameworks.is_empty()
        {
            format!("Reader context: experience {}. Known languages: {}. Learning languages: {}. Known frameworks: {}. Learning frameworks: {}.", self.reader.experience, self.reader.known_languages.join(", "), self.reader.learning_languages.join(", "), self.reader.known_frameworks.join(", "), self.reader.learning_frameworks.join(", "))
        } else {
            String::new()
        };
        let prompt = format!("You explain changed source code to an experienced software engineer learning this language or technology. {}. The programming language is {}. {} Explain, do not review, critique, suggest improvements, or infer intent. A commit subject is evidence only; do not infer developer intent from it. Do not generate HTML. Return exactly one JSON object with these fields: overview (string), annotations (array of objects with start_line, end_line, kind, text), and optionally deep (string). Annotation lines are relative to the supplied function. Use at most {} annotations, each at most {} words. Explain language concepts: {}. Explain framework concepts: {}. Infer intent: {}. {task}\n\nFUNCTION:\n{}\n\nRELEVANT DIFF:\n{}", r.git_context, r.language, reader_context, self.explanation.max_annotations, self.explanation.max_annotation_words, self.explanation.explain_language_concepts, self.explanation.explain_framework_concepts, self.explanation.infer_intent, r.function, r.diff);
        let mut req = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .json(&Req {
                model: self.model.clone(),
                messages: vec![
                    Msg {
                        role: "system".into(),
                        content: "Return structured JSON only.".into(),
                    },
                    Msg {
                        role: "user".into(),
                        content: prompt,
                    },
                ],
                temperature: generation.temperature,
                max_tokens: generation.max_tokens,
                response_format: ResponseFormat {
                    kind: "json_object",
                },
                chat_template_kwargs: serde_json::json!({"enable_thinking": false}),
            });
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        let resp: Resp = req
            .send()
            .await
            .context("model request")?
            .error_for_status()
            .context("model response")?
            .json()
            .await
            .context("model JSON envelope")?;
        let content = clean_content(
            &resp
                .choices
                .first()
                .context("model returned no choices")?
                .message
                .content,
        );
        let parsed: FunctionExplanation =
            serde_json::from_str(&content).context("malformed explanation JSON")?;
        Ok(parsed)
    }
}

fn clean_content(raw: &str) -> String {
    let after_thinking = raw
        .rfind("</think>")
        .map(|index| &raw[index + "</think>".len()..])
        .unwrap_or(raw)
        .trim();
    let without_fence = after_thinking
        .strip_prefix("```json")
        .or_else(|| after_thinking.strip_prefix("```"))
        .unwrap_or(after_thinking)
        .trim();
    without_fence
        .strip_suffix("```")
        .unwrap_or(without_fence)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::clean_content;
    #[test]
    fn removes_reasoning_wrapper_and_fence() {
        assert_eq!(
            clean_content("<think>hidden</think>\n```json\n{\"ok\":true}\n```"),
            "{\"ok\":true}"
        );
    }
}
