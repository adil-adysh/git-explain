use super::*;
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
}
impl OpenAiProvider {
    pub fn from_env() -> Self {
        Self {
            client: Client::new(),
            base_url: std::env::var("GIT_EXPLAIN_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8000/v1".into()),
            model: std::env::var("GIT_EXPLAIN_MODEL").unwrap_or_else(|_| "local-model".into()),
            api_key: std::env::var("GIT_EXPLAIN_API_KEY").ok(),
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
        let task = if r.deep {
            "Return JSON with a concise field `overview`, an empty `annotations` array, and a `deep` field containing a step-by-step explanation."
        } else {
            "Return JSON with `overview` and 1-4 `annotations`; annotation lines are relative to the supplied function."
        };
        let prompt = format!("You explain changed source code to an experienced software engineer learning this language or technology. Explain, do not review, critique, suggest improvements, or infer intent. Do not generate HTML. Return exactly one JSON object with these fields: overview (string), annotations (array of objects with start_line, end_line, kind, text), and optionally deep (string). Annotation line numbers are relative to the supplied function. {task}\n\nFUNCTION:\n{}\n\nRELEVANT DIFF:\n{}", r.function, r.diff);
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
                temperature: 0.2,
                max_tokens: if r.deep { 1200 } else { 450 },
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
