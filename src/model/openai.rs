use super::*;
use crate::config::{ExplanationConfig, GenerationConfig, ModelConfig, ReaderConfig};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    LlamaCpp,
    OpenAiCompatible,
}

impl ProviderKind {
    fn from_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("llama_cpp") {
            Self::LlamaCpp
        } else {
            Self::OpenAiCompatible
        }
    }
    fn is_llama_cpp(self) -> bool {
        matches!(self, Self::LlamaCpp)
    }
}

#[derive(Clone)]
pub struct OpenAiProvider {
    client: Client,
    kind: ProviderKind,
    base_url: String,
    model: String,
    api_key: Option<String>,
    normal: GenerationConfig,
    deep: GenerationConfig,
    reader: ReaderConfig,
    explanation: ExplanationConfig,
}

impl OpenAiProvider {
    pub fn from_config(
        model: ModelConfig,
        reader: ReaderConfig,
        explanation: ExplanationConfig,
    ) -> Self {
        Self::from_config_with_timeout(model, reader, explanation, Duration::from_secs(120))
    }

    pub fn from_config_with_timeout(
        model: ModelConfig,
        reader: ReaderConfig,
        explanation: ExplanationConfig,
        timeout: Duration,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .expect("valid model HTTP client configuration"),
            kind: ProviderKind::from_name(&model.provider),
            base_url: model.base_url,
            model: model.model,
            api_key: model.api_key,
            normal: model.normal,
            deep: model.deep,
            reader,
            explanation,
        }
    }

    fn build_request(&self, request: &ExplanationRequest) -> Req {
        let generation = if request.deep {
            &self.deep
        } else {
            &self.normal
        };
        let reader_context = if self.reader.experience != "experienced"
            || !self.reader.known_languages.is_empty()
            || !self.reader.learning_languages.is_empty()
            || !self.reader.known_frameworks.is_empty()
            || !self.reader.learning_frameworks.is_empty()
        {
            format!("Reader:\n{}\n\nKnown languages:\n{}\n\nLearning languages:\n{}\n\nKnown frameworks:\n{}\n\nLearning frameworks:\n{}", self.reader.experience, self.reader.known_languages.join(", "), self.reader.learning_languages.join(", "), self.reader.known_frameworks.join(", "), self.reader.learning_frameworks.join(", "))
        } else {
            String::new()
        };
        let regions = request
            .regions
            .iter()
            .map(|region| {
                format!(
                    "[REGION {}] lines {}-{}\n{}",
                    region.id, region.start_line, region.end_line, region.source
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let task = if request.deep {
            "Return exactly one JSON object with one required string field: explanation. Give a focused, step-by-step teaching explanation. Do not review, suggest rewrites, or speculate about intent."
        } else {
            "Return exactly one JSON object with required fields overview and annotations. Keep overview to at most two sentences. Select only the configured number of regions that materially help understanding. Do not provide a tutorial, repeat the overview, explain trivial syntax, review code, suggest rewrites, or speculate about intent."
        };
        let prior = request
            .prior_explanation
            .as_deref()
            .map(|text| format!("\n\nNORMAL OVERVIEW:\n{text}"))
            .unwrap_or_default();
        let prompt = format!("{task}\n\n{}\nProgramming language: {}\n{}\n\nFUNCTION:\n{}\n\nDETERMINISTIC SOURCE REGIONS (the region number is the only location identifier you may return):\n{}\n\nRELEVANT DIFF:\n{}{}\n\nAnnotation limit: {}. Maximum words per annotation: {}. Explain language concepts: {}. Explain framework concepts: {}. Infer intent: {}. Do not calculate or return source line numbers. Do not include fields other than those required by the schema.", request.git_context, request.language, reader_context, request.function, regions, request.diff, prior, self.explanation.max_annotations, self.explanation.max_annotation_words, self.explanation.explain_language_concepts, self.explanation.explain_framework_concepts, self.explanation.infer_intent);
        Req {
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
            response_format: if self.kind.is_llama_cpp() {
                if request.deep {
                    deep_schema()
                } else {
                    normal_schema()
                }
            } else {
                json!({"type": "json_object"})
            },
            chat_template_kwargs: self
                .kind
                .is_llama_cpp()
                .then(|| json!({"enable_thinking": generation.reasoning})),
            reasoning_effort: self.kind.is_llama_cpp().then(|| {
                if generation.reasoning {
                    "high".into()
                } else {
                    "none".into()
                }
            }),
            reasoning_format: self.kind.is_llama_cpp().then(|| "deepseek".into()),
        }
    }

    fn parse_response(
        &self,
        content: &str,
        request: &ExplanationRequest,
    ) -> Result<FunctionExplanation> {
        let raw: RawResponse =
            serde_json::from_str(&clean_content(content)).context("malformed explanation JSON")?;
        if request.deep {
            let explanation = raw
                .explanation
                .or(raw.deep)
                .or(raw.overview)
                .context("deep explanation response omitted explanation")?;
            return Ok(FunctionExplanation {
                overview: String::new(),
                annotations: vec![],
                deep: Some(strip_reasoning(&explanation)),
            });
        }
        let overview = raw.overview.context("normal response omitted overview")?;
        let raw_annotations = raw
            .annotations
            .context("normal response omitted annotations")?;
        let annotations = raw_annotations
            .into_iter()
            .filter_map(|annotation| {
                map_annotation(
                    annotation,
                    &request.regions,
                    self.explanation.max_annotation_words,
                    !self.kind.is_llama_cpp(),
                )
            })
            .take(self.explanation.max_annotations as usize)
            .collect();
        Ok(FunctionExplanation {
            overview: truncate_words(&truncate_sentences(&strip_reasoning(&overview), 2), 80),
            annotations,
            deep: None,
        })
    }
}

#[derive(Serialize, Debug)]
struct Req {
    model: String,
    messages: Vec<Msg>,
    temperature: f32,
    max_tokens: u32,
    response_format: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_format: Option<String>,
}
#[derive(Serialize, Debug)]
struct Msg {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct Resp {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}
#[derive(Deserialize)]
struct Choice {
    message: MsgOut,
    #[serde(default)]
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct MsgOut {
    #[serde(default)]
    content: Option<String>,
    #[serde(default, rename = "reasoning_content")]
    _reasoning_content: Option<String>,
}
#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}
impl Usage {
    fn summary(&self) -> String {
        format!(
            "prompt_tokens={:?}, completion_tokens={:?}",
            self.prompt_tokens, self.completion_tokens
        )
    }
}
#[derive(Deserialize)]
struct RawResponse {
    overview: Option<String>,
    annotations: Option<Vec<RawAnnotation>>,
    explanation: Option<String>,
    deep: Option<String>,
}
#[derive(Deserialize)]
struct RawAnnotation {
    region: Option<usize>,
    kind: Option<String>,
    text: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

fn map_annotation(
    annotation: RawAnnotation,
    regions: &[ExplanationRegion],
    max_words: u32,
    allow_legacy_lines: bool,
) -> Option<Annotation> {
    let kind = annotation.kind.unwrap_or_else(|| "context".into());
    if !matches!(kind.as_str(), "change" | "concept" | "flow" | "context") {
        return None;
    }
    let (start_line, end_line) = if let Some(id) = annotation.region {
        let region = regions.iter().find(|region| region.id == id)?;
        (region.start_line, region.end_line)
    } else if allow_legacy_lines {
        (annotation.start_line?, annotation.end_line?)
    } else {
        return None;
    };
    Some(Annotation {
        start_line,
        end_line,
        kind,
        text: truncate_words(&strip_reasoning(&annotation.text), max_words as usize),
    })
}
fn truncate_words(text: &str, max_words: usize) -> String {
    text.split_whitespace()
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}
fn truncate_sentences(text: &str, max_sentences: usize) -> String {
    let mut sentences = 0;
    for (index, character) in text.char_indices() {
        if matches!(character, '.' | '!' | '?') {
            sentences += 1;
            if sentences >= max_sentences {
                return text[..=index].trim().to_string();
            }
        }
    }
    text.trim().to_string()
}
fn strip_reasoning(text: &str) -> String {
    clean_content(text)
}
fn clean_content(raw: &str) -> String {
    let trimmed = strip_reasoning_sections(raw.trim());
    let after_thinking = trimmed.trim();
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

fn strip_reasoning_sections(raw: &str) -> String {
    let mut result = raw.to_string();
    for (open, close) in [("<think>", "</think>"), ("<|thinking|>", "</|thinking|>")] {
        loop {
            let Some(start) = result.find(open) else {
                break;
            };
            let tail = &result[start + open.len()..];
            if let Some(end) = tail.find(close) {
                result.replace_range(start..start + open.len() + end + close.len(), "");
            } else {
                result.truncate(start);
                break;
            }
        }
    }
    result.trim().to_string()
}

fn normal_schema() -> Value {
    json!({"type":"json_schema","json_schema":{"name":"git_explain_normal","strict":true,"schema":{"type":"object","additionalProperties":false,"required":["overview","annotations"],"properties":{"overview":{"type":"string"},"annotations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["region","kind","text"],"properties":{"region":{"type":"integer","minimum":1},"kind":{"type":"string","enum":["change","concept","flow","context"]},"text":{"type":"string"}}}}}}}})
}
fn deep_schema() -> Value {
    json!({"type":"json_schema","json_schema":{"name":"git_explain_deep","strict":true,"schema":{"type":"object","additionalProperties":false,"required":["explanation"],"properties":{"explanation":{"type":"string"}}}}})
}

#[async_trait]
impl ExplanationProvider for OpenAiProvider {
    async fn explain(&self, request: ExplanationRequest) -> Result<FunctionExplanation> {
        let payload = self.build_request(&request);
        let mut req = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .json(&payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let started = Instant::now();
        let response: Resp = req
            .send()
            .await
            .context("model request")?
            .error_for_status()
            .context("model response")?
            .json()
            .await
            .context("model JSON envelope")?;
        let choice = response
            .choices
            .first()
            .context("model returned no choices")?;
        let usage = response
            .usage
            .as_ref()
            .map(Usage::summary)
            .unwrap_or_else(|| "usage unavailable".into());
        eprintln!(
            "model request: provider={:?} model={} duration_ms={} finish_reason={:?} {}",
            self.kind,
            self.model,
            started.elapsed().as_millis(),
            choice.finish_reason,
            usage
        );
        if choice.finish_reason.as_deref() == Some("length") {
            bail!("model response truncated at token limit ({usage})");
        }
        let content = choice.message.content.clone().unwrap_or_default();
        if content.trim().is_empty() {
            if choice.message._reasoning_content.is_some() {
                bail!("model returned reasoning separately but no visible answer ({usage})");
            }
            bail!("model returned empty content ({usage})");
        }
        self.parse_response(&content, &request).with_context(|| {
            format!(
                "invalid structured model content (finish_reason={:?}; {usage})",
                choice.finish_reason
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExplanationConfig, GenerationConfig, ModelConfig, ReaderConfig};

    fn provider(name: &str) -> OpenAiProvider {
        OpenAiProvider::from_config(
            ModelConfig {
                provider: name.into(),
                base_url: "http://localhost:8083/v1".into(),
                model: "test-model".into(),
                api_key_env: None,
                api_key: None,
                normal: GenerationConfig {
                    reasoning: false,
                    max_tokens: 500,
                    temperature: 0.2,
                },
                deep: GenerationConfig {
                    reasoning: true,
                    max_tokens: 2500,
                    temperature: 0.3,
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
        )
    }
    fn request(deep: bool) -> ExplanationRequest {
        ExplanationRequest {
            function: "fn work() { changed(); }".into(),
            diff: "+changed".into(),
            language: "Rust".into(),
            git_context: "Change source: working tree".into(),
            regions: vec![ExplanationRegion {
                id: 1,
                start_line: 1,
                end_line: 1,
                source: "changed();".into(),
            }],
            prior_explanation: None,
            deep,
        }
    }
    #[test]
    fn llama_normal_request_uses_schema_and_disables_reasoning() {
        let value =
            serde_json::to_value(provider("llama_cpp").build_request(&request(false))).unwrap();
        assert_eq!(value["model"], "test-model");
        assert!((value["temperature"].as_f64().unwrap() - 0.2).abs() < 1e-6);
        assert_eq!(value["max_tokens"], 500);
        assert_eq!(value["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(value["reasoning_effort"], "none");
        assert_eq!(value["reasoning_format"], "deepseek");
        assert_eq!(value["response_format"]["type"], "json_schema");
        assert_eq!(
            value["response_format"]["json_schema"]["schema"]["required"],
            json!(["overview", "annotations"])
        );
        assert_eq!(
            value["response_format"]["json_schema"]["schema"]["properties"]["annotations"]["items"]
                ["required"],
            json!(["region", "kind", "text"])
        );
    }
    #[test]
    fn llama_deep_request_is_separate_and_enables_reasoning() {
        let value =
            serde_json::to_value(provider("llama_cpp").build_request(&request(true))).unwrap();
        assert!((value["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(value["max_tokens"], 2500);
        assert_eq!(value["chat_template_kwargs"]["enable_thinking"], true);
        assert_eq!(value["reasoning_effort"], "high");
        assert_eq!(value["reasoning_format"], "deepseek");
        assert_eq!(
            value["response_format"]["json_schema"]["schema"]["required"],
            json!(["explanation"])
        );
        assert!(
            value["response_format"]["json_schema"]["schema"]["properties"]
                .get("deep_explanation")
                .is_none()
        );
        assert!(value["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("+changed"));
    }
    #[test]
    fn generic_provider_keeps_json_object_fallback() {
        let value =
            serde_json::to_value(provider("openai_compatible").build_request(&request(false)))
                .unwrap();
        assert_eq!(value["response_format"], json!({"type":"json_object"}));
        assert!(value.get("chat_template_kwargs").is_none());
        assert!(value.get("reasoning_effort").is_none());
        assert!(value.get("reasoning_format").is_none());
    }
    #[test]
    fn region_annotations_map_and_reasoning_is_removed() {
        let p = provider("llama_cpp");
        let r = request(false);
        let parsed = p.parse_response(r#"{"overview":"Short.","annotations":[{"region":1,"kind":"change","text":"<think>bad</think>Explain it."}]}"#, &r).unwrap();
        assert_eq!(parsed.annotations[0].start_line, 1);
        assert_eq!(parsed.annotations[0].end_line, 1);
        assert_eq!(parsed.annotations[0].text, "Explain it.");
        let deep = p
            .parse_response(
                r#"<think>hidden</think>{"explanation":"Visible explanation."}"#,
                &ExplanationRequest { deep: true, ..r },
            )
            .unwrap();
        assert_eq!(deep.deep.as_deref(), Some("Visible explanation."));
    }

    #[test]
    fn llama_annotations_must_use_deterministic_regions() {
        let p = provider("llama_cpp");
        let r = request(false);
        let parsed = p
            .parse_response(
                r#"{"overview":"Short.","annotations":[{"start_line":1,"end_line":1,"kind":"change","text":"Wrong contract."}]}"#,
                &r,
            )
            .unwrap();
        assert!(parsed.annotations.is_empty());
    }

    #[test]
    fn reasoning_markers_are_removed_even_when_embedded() {
        let p = provider("llama_cpp");
        let r = request(false);
        let parsed = p
            .parse_response(
                r#"{"overview":"Before <think>secret</think> after.","annotations":[{"region":1,"kind":"context","text":"Visible <|thinking|>hidden</|thinking|> text."}]}"#,
                &r,
            )
            .unwrap();
        assert_eq!(parsed.overview, "Before after.");
        assert_eq!(parsed.annotations[0].text, "Visible text.");
    }
    #[test]
    fn malformed_output_is_rejected() {
        assert!(provider("llama_cpp")
            .parse_response("not json", &request(false))
            .unwrap_err()
            .to_string()
            .contains("malformed explanation JSON"));
    }
}
