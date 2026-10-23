use super::*;
use crate::{
    config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile},
    context::{
        calculate_context_requirement, negotiate_context, ConservativeTokenEstimator,
        ContextBudget, ContextCapabilities, ContextCapacity, ContextControl, ContextNegotiation,
        PromptPlan,
    },
    local_context::{workload_key, LocalContextRecord, LocalContextTracker},
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Discover backend-specific capacity information without changing inference
/// transport. Failures are deliberately non-fatal: compatible endpoints need
/// not expose diagnostic routes.
pub async fn discover_context_capacity(model: &ResolvedProfile) -> ContextCapacity {
    discover_context_capabilities(model).await.capacity
}

/// Discover capacity separately from whether the selected transport can alter
/// it. Ollama uses its native API, including its verified request-scoped
/// `options.num_ctx` control; generic compatible endpoints remain untouched.
pub async fn discover_context_capabilities(model: &ResolvedProfile) -> ContextCapabilities {
    let kind = provider_kind_for_profile(model);
    let capacity = kind
        .discover_context_capacity(
            &Client::new(),
            &model.base_url,
            &model.model,
            model.context_window,
        )
        .await;
    ContextCapabilities {
        capacity,
        control: kind.context_control(),
    }
}

pub fn context_control_for_profile(model: &ResolvedProfile) -> ContextControl {
    provider_kind_for_profile(model).context_control()
}

pub fn context_backend_key_for_profile(model: &ResolvedProfile) -> &'static str {
    provider_kind_for_profile(model).telemetry_key()
}

pub fn context_backend_name_for_profile(model: &ResolvedProfile) -> &'static str {
    provider_kind_for_profile(model).display_name()
}

pub fn context_remediation_instruction_for_profile(
    model: &ResolvedProfile,
    profile: &str,
    target: u32,
) -> String {
    provider_kind_for_profile(model).remediation_instruction(profile, target)
}

/// Telemetry is local-only: never persist workload metadata for remote hosts.
pub fn is_local_profile(model: &ResolvedProfile) -> bool {
    reqwest::Url::parse(&model.base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]"))
}

fn provider_kind_for_profile(model: &ResolvedProfile) -> ProviderKind {
    ProviderKind::from_preset(model.preset.as_deref())
}

impl ProviderKind {
    async fn discover_context_capacity(
        self,
        client: &Client,
        base_url: &str,
        model: &str,
        profile_limit: Option<u32>,
    ) -> ContextCapacity {
        let mut capacity = ContextCapacity {
            profile_limit,
            ..ContextCapacity::default()
        };
        match self {
            ProviderKind::Ollama => {
                let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
                if let Ok(response) = client.get(format!("{base}/api/ps")).send().await {
                    if let Ok(value) = response.json::<Value>().await {
                        capacity.runtime_allocated = ollama_runtime_context(&value, model);
                    }
                }
                if let Ok(response) = client
                    .post(format!("{base}/api/show"))
                    .json(&json!({"model": model}))
                    .send()
                    .await
                {
                    if let Ok(value) = response.json::<Value>().await {
                        capacity.model_max = ollama_model_context(&value);
                    }
                }
            }
            ProviderKind::LlamaCpp => {
                if let Ok(response) = client
                    .get(format!("{}/models", base_url.trim_end_matches('/')))
                    .send()
                    .await
                {
                    if let Ok(value) = response.json::<Value>().await {
                        capacity.runtime_allocated = llama_cpp_configured_context(&value, model);
                        capacity.model_max = llama_cpp_model_context(&value, model);
                    }
                }
            }
            ProviderKind::OpenAiCompatible => {}
        }
        capacity
    }
}

fn ollama_runtime_context(value: &Value, model: &str) -> Option<u32> {
    value
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| {
            models.iter().find(|entry| {
                ["name", "model"].iter().any(|key| {
                    entry.get(*key).and_then(Value::as_str).is_some_and(|name| {
                        name == model || name.strip_suffix(":latest") == Some(model)
                    })
                })
            })
        })
        .and_then(|entry| entry.get("context_length"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn ollama_model_context(value: &Value) -> Option<u32> {
    value
        .get("model_info")
        .and_then(Value::as_object)
        .and_then(|info| {
            info.iter().find_map(|(key, value)| {
                key.ends_with(".context_length")
                    .then(|| value.as_u64())
                    .flatten()
            })
        })
        .and_then(|value| u32::try_from(value).ok())
}

fn llama_cpp_configured_context(value: &Value, model: &str) -> Option<u32> {
    llama_cpp_model_entry(value, model)
        .and_then(|entry| entry.pointer("/status/args").and_then(Value::as_array))
        .and_then(|args| {
            args.windows(2).find_map(|pair| {
                (pair[0].as_str() == Some("--ctx-size"))
                    .then(|| pair[1].as_str())
                    .flatten()
                    .and_then(|value| value.parse::<u32>().ok())
            })
        })
}

fn llama_cpp_model_context(value: &Value, model: &str) -> Option<u32> {
    llama_cpp_model_entry(value, model)
        .and_then(|entry| entry.pointer("/meta/n_ctx_train").and_then(Value::as_u64))
        .and_then(|value| u32::try_from(value).ok())
}

fn llama_cpp_model_entry<'a>(value: &'a Value, model: &str) -> Option<&'a Value> {
    value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|models| {
            models.iter().find(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(model)
                    || entry
                        .get("aliases")
                        .and_then(Value::as_array)
                        .is_some_and(|aliases| {
                            aliases.iter().any(|alias| alias.as_str() == Some(model))
                        })
            })
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    LlamaCpp,
    Ollama,
    OpenAiCompatible,
}

impl ProviderKind {
    fn from_preset(preset: Option<&str>) -> Self {
        if preset.is_some_and(|name| name.eq_ignore_ascii_case("llama_cpp")) {
            Self::LlamaCpp
        } else if preset.is_some_and(|name| name.eq_ignore_ascii_case("ollama")) {
            Self::Ollama
        } else {
            Self::OpenAiCompatible
        }
    }
    fn is_llama_cpp(self) -> bool {
        matches!(self, Self::LlamaCpp)
    }
    pub const fn context_control(self) -> ContextControl {
        match self {
            // llama.cpp configures --ctx-size when its server starts. Ollama's
            // native chat API accepts a request-scoped options.num_ctx value.
            Self::LlamaCpp => ContextControl::FixedRuntime,
            Self::Ollama => ContextControl::PerRequest,
            Self::OpenAiCompatible => ContextControl::Unknown,
        }
    }
    pub const fn telemetry_key(self) -> &'static str {
        match self {
            Self::LlamaCpp => "llama_cpp",
            Self::Ollama => "ollama",
            Self::OpenAiCompatible => "openai_compatible",
        }
    }
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::LlamaCpp => "llama.cpp",
            Self::Ollama => "Ollama",
            Self::OpenAiCompatible => "the local OpenAI-compatible backend",
        }
    }
    fn remediation_instruction(self, profile: &str, target: u32) -> String {
        match self {
            Self::Ollama => format!(
                "\n\nAllow git-explain to request this context by setting the profile cap, then run:\ngit explain profile edit {profile} --context-window {target}\ngit explain profile test {profile}"
            ),
            Self::LlamaCpp => format!(
                "\n\nRestart llama.cpp with `--ctx-size {target}`, then run:\ngit explain profile test {profile}"
            ),
            Self::OpenAiCompatible => format!(
                "\n\nThis local OpenAI-compatible backend has no verified context-size control. The recommendation is advisory; consult its server documentation or startup configuration, then run:\ngit explain profile test {profile}"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InferenceRequestOptions {
    requested_context: Option<u32>,
}

impl InferenceRequestOptions {
    fn for_negotiation(negotiation: &ContextNegotiation) -> Self {
        // This is the sole adapter boundary for verified request-scoped
        // context controls. Generic OpenAI-compatible adapters never serialize
        // a non-standard context field.
        Self {
            requested_context: matches!(negotiation.control, ContextControl::PerRequest)
                .then_some(negotiation.requested_context)
                .flatten(),
        }
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
    explanation: ExplanationConfig,
    context_window: Option<u32>,
    context_tracker: Option<(LocalContextTracker, String)>,
}

impl OpenAiProvider {
    #[cfg(test)]
    fn build_request(&self, request: &ExplanationRequest) -> Req {
        self.build_request_with_retry(request, false)
    }
    pub fn from_config(
        model: ResolvedProfile,
        reader: ReaderConfig,
        explanation: ExplanationConfig,
    ) -> Self {
        Self::from_config_with_timeout(model, reader, explanation, Duration::from_secs(120))
    }

    pub fn from_config_with_timeout(
        model: ResolvedProfile,
        _reader: ReaderConfig,
        explanation: ExplanationConfig,
        timeout: Duration,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .expect("valid model HTTP client configuration"),
            kind: ProviderKind::from_preset(model.preset.as_deref()),
            base_url: model.base_url,
            model: model.model,
            api_key: model.api_key,
            normal: model.normal,
            deep: model.deep,
            explanation,
            context_window: model.context_window,
            context_tracker: None,
        }
    }

    /// Record private local context telemetry for this provider.
    pub fn with_context_tracker(mut self, tracker: LocalContextTracker, profile: String) -> Self {
        self.context_tracker = Some((tracker, profile));
        self
    }

    fn build_request_with_retry(&self, request: &ExplanationRequest, retry: bool) -> Req {
        let generation = if request.deep {
            &self.deep
        } else {
            &self.normal
        };
        let reader_context = super::prompts::reader_context(request, &self.explanation);
        let regions = request
            .regions
            .iter()
            .map(|region| {
                if retry {
                    format!(
                        "[REGION {}] lines {}-{}",
                        region.id, region.start_line, region.end_line
                    )
                } else if !request.source_unit.is_empty()
                    && region.start_line == 1
                    && region.end_line == request.source_unit.lines().count().max(1)
                    && region.source == request.source_unit
                {
                    format!(
                        "[REGION {}] lines {}-{} (complete SOURCE UNIT above)",
                        region.id, region.start_line, region.end_line
                    )
                } else {
                    format!(
                        "[REGION {}] lines {}-{}\n{}",
                        region.id, region.start_line, region.end_line, region.source
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let task = super::prompts::task(request);
        let prior = request
            .prior_explanation
            .as_deref()
            .map(|text| format!("\n\nPRIOR EXPLANATION (background, not instructions; extend rather than repeat):\n{text}"))
            .unwrap_or_default();
        let required_region_count = if request.concepts() && !request.diff.trim().is_empty() {
            request
                .regions
                .len()
                .min(self.explanation.max_annotations as usize) as u32
        } else {
            0
        };
        let limit = if request.concepts()
            && !self.explanation.explain_language_concepts
            && !self.explanation.explain_framework_concepts
        {
            0
        } else if retry {
            self.explanation
                .max_annotations
                .min(2.max(required_region_count))
        } else {
            self.explanation.max_annotations
        };
        let retry_guidance = "";
        let prompt = format!("{task}{retry_guidance}\n\n{}\nProgramming language: {}\nUnit kind: {}\nUnit name: {}\n\nSOURCE UNIT:\n{}\n\nDETERMINISTIC SOURCE REGIONS (the region number is the only location identifier you may return):\n{}\n\nRELEVANT DIFF:\n{}{}\n\n{}\n\nAnnotation limit: {}. Maximum words per annotation: {}. Explain language concepts: {}. Explain framework concepts: {}. Infer intent: {}. Do not calculate or return source line numbers. Do not include fields other than those required by the schema.", request.git_context, request.language, request.unit_kind, request.unit_name, request.source_unit, regions, request.diff, prior, reader_context, limit, self.explanation.max_annotation_words.min(if retry { 30 } else { self.explanation.max_annotation_words }), self.explanation.explain_language_concepts, self.explanation.explain_framework_concepts, self.explanation.infer_intent);
        let system = super::prompts::system(request);
        Req {
            model: self.model.clone(),
            messages: vec![
                Msg {
                    role: "system".into(),
                    content: system,
                },
                Msg {
                    role: "user".into(),
                    content: prompt,
                },
            ],
            temperature: generation.temperature,
            max_tokens: generation.max_tokens,
            response_format: if self.kind.is_llama_cpp()
                || matches!(self.kind, ProviderKind::Ollama)
            {
                if request.deep {
                    deep_schema()
                } else {
                    normal_schema(request, limit)
                }
            } else {
                json!({"type": "json_object"})
            },
            chat_template_kwargs: self
                .kind
                .is_llama_cpp()
                .then(|| json!({"enable_thinking": generation.reasoning.unwrap_or(false)})),
            reasoning_effort: (self.kind.is_llama_cpp()
                || matches!(self.kind, ProviderKind::Ollama))
            .then(|| {
                if generation.reasoning.unwrap_or(false) {
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
    ) -> Result<UnitExplanation> {
        let value: Value =
            serde_json::from_str(&clean_content(content)).context("malformed explanation JSON")?;
        let allowed: &[&str] = if request.deep {
            &["purpose", "walkthrough", "boundaries", "example"]
        } else {
            &["overview", "annotations"]
        };
        if value
            .as_object()
            .is_none_or(|object| object.keys().any(|key| !allowed.contains(&key.as_str())))
        {
            bail!("structured explanation contains unexpected fields");
        }
        let mut raw: RawResponse =
            serde_json::from_value(value.clone()).context("malformed explanation JSON")?;
        if value.get("purpose").is_some() {
            if !["purpose", "walkthrough", "boundaries", "example"]
                .iter()
                .all(|key| value.get(*key).is_some())
            {
                bail!("deep teaching omitted required fields");
            }
            raw.teaching = Some(serde_json::from_value(value).context("malformed deep teaching")?);
        }
        if request.deep {
            if let Some(mut teaching) = raw.teaching {
                teaching.purpose = strip_reasoning(&teaching.purpose);
                teaching.walkthrough = teaching
                    .walkthrough
                    .iter()
                    .map(|s| strip_reasoning(s))
                    .collect();
                teaching.boundaries = teaching
                    .boundaries
                    .iter()
                    .map(|s| strip_reasoning(s))
                    .collect();
                teaching.example = teaching.example.map(|s| strip_reasoning(&s));
                if teaching.purpose.trim().is_empty()
                    || teaching.walkthrough.is_empty()
                    || teaching
                        .walkthrough
                        .iter()
                        .chain(&teaching.boundaries)
                        .any(|s| s.trim().is_empty())
                    || teaching
                        .example
                        .as_ref()
                        .is_some_and(|s| s.trim().is_empty())
                {
                    bail!("deep teaching contains blank or missing teaching content");
                }
                let mut paragraphs = vec![teaching.purpose.clone()];
                paragraphs.extend(teaching.walkthrough.clone());
                paragraphs.extend(teaching.boundaries.clone());
                if let Some(example) = &teaching.example {
                    paragraphs.push(format!("Example: {example}"));
                }
                return Ok(UnitExplanation {
                    overview: String::new(),
                    annotations: vec![],
                    deep: Some(paragraphs.join("\n\n")),
                    teaching: Some(teaching),
                });
            }
            bail!("deep explanation response omitted structured teaching");
        }
        let overview = raw.overview.context("normal response omitted overview")?;
        let raw_annotations = raw
            .annotations
            .context("normal response omitted annotations")?;
        let raw_annotation_count = raw_annotations.len();
        let mut annotations = raw_annotations
            .into_iter()
            .filter_map(|annotation| {
                map_annotation(
                    annotation,
                    &request.regions,
                    self.explanation.max_annotation_words,
                )
            })
            .filter(|annotation| match request.kind {
                ExplanationKind::Concepts => annotation.kind == "concept",
                ExplanationKind::Changes => annotation.kind == "change",
                _ => true,
            })
            .collect::<Vec<_>>();
        if annotations.len() != raw_annotation_count
            || annotations
                .iter()
                .any(|annotation| annotation.text.trim().is_empty())
            || (request.kind == ExplanationKind::Overview && raw_annotation_count > 0)
        {
            bail!("response contains invalid annotations or deterministic regions");
        }
        if request.concepts()
            && (annotations.len() != raw_annotation_count
                || annotations.iter().any(|annotation| {
                    annotation.text.is_empty()
                        || annotation.title.as_deref().is_some_and(|title| {
                            title.trim().eq_ignore_ascii_case(annotation.text.trim())
                        })
                        || annotation
                            .title
                            .as_ref()
                            .is_some_and(|s| s.trim().is_empty())
                        || (annotation
                            .title
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty()))
                }))
        {
            bail!("concept response contains invalid concept annotations");
        }
        annotations.sort_by(|left, right| {
            (left.start_line, left.end_line, &left.kind, &left.text).cmp(&(
                right.start_line,
                right.end_line,
                &right.kind,
                &right.text,
            ))
        });
        annotations.dedup_by(|left, right| {
            left.start_line == right.start_line
                && left.end_line == right.end_line
                && left.kind == right.kind
                && left.title == right.title
                && left.text == right.text
        });
        let annotation_limit = if request.concepts()
            && !self.explanation.explain_language_concepts
            && !self.explanation.explain_framework_concepts
        {
            0
        } else {
            self.explanation.max_annotations as usize
        };
        if request.concepts() && annotation_limit == 0 && !annotations.is_empty() {
            bail!("concept response contains disabled teaching categories");
        }
        annotations.truncate(annotation_limit);
        let annotations = if request.kind == ExplanationKind::Overview {
            vec![]
        } else {
            annotations
        };
        if request.concepts()
            && !annotations.is_empty()
            && !request.diff.trim().is_empty()
            && request.regions.len() <= self.explanation.max_annotations as usize
        {
            let missing = request.regions.iter().any(|region| {
                !annotations.iter().any(|annotation| {
                    annotation.start_line == region.start_line
                        && annotation.end_line == region.end_line
                })
            });
            if missing {
                bail!("concept response did not cover every changed region");
            }
        }
        Ok(UnitExplanation {
            overview: truncate_words(&truncate_sentences(&strip_reasoning(&overview), 2), 80),
            annotations,
            deep: None,
            teaching: None,
        })
    }
}

#[derive(Serialize, Debug)]
struct Req {
    model: String,
    messages: Vec<Msg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    response_format: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_format: Option<String>,
}
#[derive(Clone, Serialize, Debug)]
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
struct OllamaChatResponse {
    message: MsgOut,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    eval_duration: Option<u64>,
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
    #[serde(skip)]
    generation_duration_ms: Option<u64>,
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
    #[serde(skip)]
    teaching: Option<DeepTeaching>,
    overview: Option<String>,
    annotations: Option<Vec<RawAnnotation>>,
}
#[derive(Deserialize)]
struct RawAnnotation {
    title: Option<String>,
    region: Option<usize>,
    kind: Option<String>,
    text: String,
}

fn map_annotation(
    annotation: RawAnnotation,
    regions: &[ExplanationRegion],
    max_words: u32,
) -> Option<Annotation> {
    let kind = annotation.kind.unwrap_or_else(|| "context".into());
    if !matches!(kind.as_str(), "change" | "concept" | "flow" | "context") {
        return None;
    }
    let (start_line, end_line) = if let Some(id) = annotation.region {
        let region = regions.iter().find(|region| region.id == id)?;
        (region.start_line, region.end_line)
    } else {
        return None;
    };
    Some(Annotation {
        title: annotation.title.map(|s| strip_reasoning(&s)),
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
        let next = text[index + character.len_utf8()..].chars().next();
        let boundary = matches!(character, '!' | '?')
            || (character == '.' && next.is_none_or(char::is_whitespace));
        if boundary {
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
        while let Some(start) = result.find(open) {
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

fn normal_schema(request: &ExplanationRequest, annotation_limit: u32) -> Value {
    let region_ids = request
        .regions
        .iter()
        .map(|region| region.id)
        .collect::<Vec<_>>();
    let kinds = match request.kind {
        ExplanationKind::Concepts => json!(["concept"]),
        ExplanationKind::Changes => json!(["change"]),
        _ => json!(["change", "concept", "flow", "context"]),
    };
    let (minimum, maximum) = if request.kind == ExplanationKind::Overview {
        (0, 0)
    } else {
        (0, annotation_limit)
    };
    let mut schema = json!({"type":"json_schema","json_schema":{"name":"git_explain_normal","strict":true,"schema":{"type":"object","additionalProperties":false,"required":["overview","annotations"],"properties":{"overview":{"type":"string"},"annotations":{"type":"array","minItems":minimum,"maxItems":maximum,"items":{"type":"object","additionalProperties":false,"required":["region","kind","text"],"properties":{"region":{"type":"integer","enum":region_ids},"kind":{"type":"string","enum":kinds},"text":{"type":"string","minLength":1}}}}}}}});
    if request.concepts() {
        let item = &mut schema["json_schema"]["schema"]["properties"]["annotations"]["items"];
        item["required"] = json!(["region", "kind", "title", "text"]);
        item["properties"]["title"] = json!({"type":"string", "minLength":1});
    }
    schema
}
fn deep_schema() -> Value {
    json!({"type":"json_schema","json_schema":{"name":"git_explain_deep","strict":true,"schema":{"type":"object","additionalProperties":false,"required":["purpose","walkthrough","boundaries","example"],"properties":{"purpose":{"type":"string","minLength":1},"walkthrough":{"type":"array","minItems":1,"items":{"type":"string","minLength":1}},"boundaries":{"type":"array","items":{"type":"string","minLength":1}},"example":{"type":["string","null"]}}}}})
}

#[async_trait]
impl ExplanationProvider for OpenAiProvider {
    async fn explain(&self, request: ExplanationRequest) -> Result<UnitExplanation> {
        if self.base_url.trim().is_empty() || self.model.trim().is_empty() {
            bail!("model configuration is incomplete: base URL and model name are required");
        }
        if !crate::language::contains_meaningful_source(&request.source_unit) {
            bail!("refusing to explain whitespace-only source");
        }
        match self.explain_once(&request, false, false).await {
            Ok(result) => Ok(result),
            Err(error) if is_local_context_error(&error) => {
                eprintln!("context requirement cannot fit; retrying once with a concise request");
                self.explain_once(&request, true, false).await
            }
            Err(error) if is_provider_context_error(&error) => {
                eprintln!(
                    "provider reported context overflow; retrying once with a concise request"
                );
                self.explain_once(&request, true, true).await
            }
            Err(error) if is_retryable_structured_error(&error) => {
                eprintln!("structured model output invalid; retrying once with a concise request");
                self.explain_once(&request, true, false).await
            }
            Err(error) => Err(error),
        }
    }
}
impl OpenAiProvider {
    async fn explain_once(
        &self,
        request: &ExplanationRequest,
        retry: bool,
        previous_provider_overflow: bool,
    ) -> Result<UnitExplanation> {
        let payload = self.build_request_with_retry(request, retry);
        let generation = if request.deep {
            &self.deep
        } else {
            &self.normal
        };
        // Estimate the complete serialized inference payload rather than just
        // source/user text: this includes the system message, JSON schema,
        // reasoning/template options, roles, and message framing.
        let serialized_payload = self.serialize_inference_payload(&payload)?;
        let requirement = calculate_context_requirement(
            &serialized_payload,
            generation,
            request.deep,
            &ConservativeTokenEstimator,
        );
        let ideal_required_context = if retry {
            let original = self.build_request_with_retry(request, false);
            let original_payload = self.serialize_inference_payload(&original)?;
            calculate_context_requirement(
                &original_payload,
                generation,
                request.deep,
                &ConservativeTokenEstimator,
            )
            .minimum_required_context
        } else {
            requirement.minimum_required_context
        };
        let capabilities = self.discover_context_capabilities().await?;
        let negotiation = match negotiate_context(&capabilities, requirement.clone()) {
            Ok(negotiation) => negotiation,
            Err(error) => {
                if retry {
                    self.track_context_failure(
                        request,
                        &capabilities,
                        &requirement,
                        ideal_required_context,
                        retry,
                        previous_provider_overflow,
                        true,
                        false,
                        false,
                        None,
                        None,
                    );
                }
                return Err(error);
            }
        };
        let request_options = InferenceRequestOptions::for_negotiation(&negotiation);
        let budget = ContextBudget::from_negotiation(&negotiation);
        let plan = PromptPlan::check(&serialized_payload, budget, &ConservativeTokenEstimator)?;
        eprintln!(
            "context plan: control={} total={} output_reserve={} safety_margin={} input_budget={} estimated_input={} requested_context={:?}",
            negotiation.control.description(),
            plan.budget.total, plan.budget.output_reserve, plan.budget.safety_margin,
            plan.budget.input_budget, plan.estimated_input, request_options.requested_context,
        );
        let started = Instant::now();
        let response = self.send_inference(&payload, request_options).await?;
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
            self.track_context_failure(
                request,
                &capabilities,
                &negotiation.requirement,
                ideal_required_context,
                retry,
                previous_provider_overflow,
                false,
                false,
                true,
                response.usage.as_ref(),
                Some(started.elapsed().as_millis() as u64),
            );
            bail!("model response truncated at token limit ({usage})");
        }
        let content = choice.message.content.clone().unwrap_or_default();
        if content.trim().is_empty() {
            if choice.message._reasoning_content.is_some() {
                bail!("model returned reasoning separately but no visible answer ({usage})");
            }
            bail!("model returned empty content ({usage})");
        }
        let explanation = match self.parse_response(&content, request).with_context(|| {
            format!(
                "invalid structured model content (finish_reason={:?}; {usage})",
                choice.finish_reason
            )
        }) {
            Ok(explanation) => explanation,
            Err(error) => {
                if retry {
                    self.track_context_failure(
                        request,
                        &capabilities,
                        &negotiation.requirement,
                        ideal_required_context,
                        retry,
                        previous_provider_overflow,
                        false,
                        false,
                        false,
                        response.usage.as_ref(),
                        Some(started.elapsed().as_millis() as u64),
                    );
                }
                return Err(error);
            }
        };
        self.track_context_success(
            request,
            retry,
            previous_provider_overflow,
            &capabilities,
            &negotiation,
            ideal_required_context,
            plan.estimated_input,
            response.usage.as_ref(),
            started.elapsed().as_millis() as u64,
        );
        Ok(explanation)
    }

    async fn discover_context_capabilities(&self) -> Result<ContextCapabilities> {
        let mut capacity = self
            .kind
            .discover_context_capacity(
                &self.client,
                &self.base_url,
                &self.model,
                self.context_window,
            )
            .await;
        if matches!(self.kind, ProviderKind::Ollama) && capacity.runtime_allocated.is_none() {
            // Ollama's /api/ps only reports a loaded model. Establish the
            // runtime allocation with a source-free request before budgeting
            // an explanation against a fixed OpenAI-compatible transport.
            self.warm_ollama_model().await?;
            capacity = self
                .kind
                .discover_context_capacity(
                    &self.client,
                    &self.base_url,
                    &self.model,
                    self.context_window,
                )
                .await;
        }
        Ok(ContextCapabilities {
            capacity,
            control: self.kind.context_control(),
        })
    }

    async fn warm_ollama_model(&self) -> Result<()> {
        let mut request = self
            .client
            .post(format!("{}/api/chat", ollama_native_base(&self.base_url)))
            .json(&json!({
                "model": self.model,
                "messages": [{"role": "user", "content": "Reply with OK."}],
                "stream": false,
                "think": false,
                "options": {"num_predict": 1},
            }));
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request
            .send()
            .await
            .context("load Ollama model for context discovery")?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!(
            "could not load Ollama model before context planning (HTTP {status}): {}",
            body.trim().chars().take(300).collect::<String>()
        );
    }

    async fn send_inference(
        &self,
        payload: &Req,
        request_options: InferenceRequestOptions,
    ) -> Result<Resp> {
        if matches!(self.kind, ProviderKind::Ollama) {
            return self
                .send_ollama_json_request(payload, request_options)
                .await;
        }
        let mut request = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .json(payload);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.context("model request")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "model response (HTTP {status}): {}",
                body.trim().chars().take(300).collect::<String>()
            );
        }
        response.json().await.context("model JSON envelope")
    }

    fn serialize_inference_payload(&self, payload: &Req) -> Result<String> {
        if matches!(self.kind, ProviderKind::Ollama) {
            return serde_json::to_string(&self.ollama_json_request(payload, None)?)
                .context("serialize Ollama JSON request");
        }
        serde_json::to_string(payload).context("serialize model request")
    }

    fn ollama_json_request(&self, payload: &Req, requested_context: Option<u32>) -> Result<Value> {
        let mut options = serde_json::Map::new();
        if let Some(temperature) = payload.temperature {
            options.insert("temperature".into(), json!(temperature));
        }
        if let Some(max_tokens) = payload.max_tokens {
            options.insert("num_predict".into(), json!(max_tokens));
        }
        if let Some(context) = requested_context {
            options.insert("num_ctx".into(), json!(context));
        }
        let format = payload.response_format["json_schema"]["schema"].clone();
        let schema_instruction = format!(
            "Return only a JSON object that conforms to this schema: {}",
            serde_json::to_string(&format).context("serialize Ollama JSON schema")?
        );
        let mut messages = payload.messages.clone();
        messages.push(Msg {
            role: "system".into(),
            content: schema_instruction,
        });
        Ok(json!({
            "model": payload.model,
            "messages": messages,
            "stream": false,
            "format": format,
            "think": payload.reasoning_effort.as_deref() != Some("none"),
            "options": options,
        }))
    }

    async fn send_ollama_json_request(
        &self,
        payload: &Req,
        request_options: InferenceRequestOptions,
    ) -> Result<Resp> {
        let request_body = self.ollama_json_request(payload, request_options.requested_context)?;
        let mut request = self
            .client
            .post(format!("{}/api/chat", ollama_native_base(&self.base_url)))
            .json(&request_body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.context("Ollama JSON model request")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "model response (HTTP {status}): {}",
                body.trim().chars().take(300).collect::<String>()
            );
        }
        let response: OllamaChatResponse = response.json().await.context("Ollama JSON envelope")?;
        Ok(Resp {
            choices: vec![Choice {
                message: response.message,
                finish_reason: response.done_reason,
            }],
            usage: Some(Usage {
                prompt_tokens: response.prompt_eval_count,
                completion_tokens: response.eval_count,
                generation_duration_ms: response.eval_duration.map(|duration| duration / 1_000_000),
            }),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn track_context_success(
        &self,
        request: &ExplanationRequest,
        concise_retry_used: bool,
        provider_overflow_seen: bool,
        capabilities: &ContextCapabilities,
        negotiation: &ContextNegotiation,
        ideal_required_context: u32,
        _estimated_input: u32,
        usage: Option<&Usage>,
        latency_ms: u64,
    ) {
        let Some(mut record) = self.new_context_record(
            request,
            capabilities,
            &negotiation.requirement,
            ideal_required_context,
            negotiation.available_context,
            usage,
        ) else {
            return;
        };
        record.latency_ms = Some(latency_ms);
        record.success = true;
        record.provider_context_overflow = provider_overflow_seen;
        record.concise_retry_used = concise_retry_used;
        record.attempts = if concise_retry_used { 2 } else { 1 };
        self.persist_context_record(record);
    }

    #[allow(clippy::too_many_arguments)]
    fn track_context_failure(
        &self,
        request: &ExplanationRequest,
        capabilities: &ContextCapabilities,
        requirement: &crate::context::ContextRequirement,
        ideal_required_context: u32,
        concise_retry_used: bool,
        previous_provider_overflow: bool,
        local_context_failure: bool,
        provider_context_overflow: bool,
        output_truncated: bool,
        usage: Option<&Usage>,
        latency_ms: Option<u64>,
    ) {
        let Some(mut record) = self.new_context_record(
            request,
            capabilities,
            requirement,
            ideal_required_context,
            capabilities.capacity.effective().tokens,
            usage,
        ) else {
            return;
        };
        record.latency_ms = latency_ms;
        record.local_context_failure = local_context_failure;
        record.provider_context_overflow = previous_provider_overflow || provider_context_overflow;
        record.output_truncated = output_truncated;
        record.concise_retry_used = concise_retry_used;
        record.attempts = if concise_retry_used { 2 } else { 1 };
        self.persist_context_record(record);
    }

    fn new_context_record(
        &self,
        request: &ExplanationRequest,
        capabilities: &ContextCapabilities,
        requirement: &crate::context::ContextRequirement,
        ideal_required_context: u32,
        effective_context: u32,
        usage: Option<&Usage>,
    ) -> Option<LocalContextRecord> {
        let (_, profile) = self.context_tracker.as_ref()?;
        let generation = if request.deep {
            &self.deep
        } else {
            &self.normal
        };
        let mut record = LocalContextRecord::now(
            profile.clone(),
            self.kind.telemetry_key().into(),
            self.model.clone(),
            request.deep,
        );
        record.workload_key = workload_key(
            &self.model,
            generation.reasoning,
            generation.max_tokens,
            generation.temperature,
        );
        record.model_max = capabilities.capacity.model_max;
        record.runtime_context = capabilities.capacity.runtime_allocated;
        record.profile_limit = self.context_window;
        record.effective_context = effective_context;
        record.estimated_input = requirement.estimated_input;
        record.output_reserve = requirement.output_reserve;
        record.safety_margin = requirement.protocol_overhead + requirement.safety_margin;
        record.ideal_required_context = ideal_required_context;
        record.final_required_context = requirement.minimum_required_context;
        record.actual_prompt_tokens = usage.and_then(|value| value.prompt_tokens);
        record.actual_completion_tokens = usage.and_then(|value| value.completion_tokens);
        record.actual_total_tokens = usage.and_then(|value| {
            value
                .prompt_tokens
                .zip(value.completion_tokens)
                .map(|(prompt, completion)| prompt.saturating_add(completion))
        });
        record.generation_duration_ms = usage.and_then(|value| value.generation_duration_ms);
        record.generation_tokens_per_second = usage.and_then(|value| {
            value
                .completion_tokens
                .zip(value.generation_duration_ms)
                .and_then(|(tokens, ms)| {
                    (ms > 0).then(|| u64::from(tokens).saturating_mul(1_000) / ms)
                })
        });
        Some(record)
    }

    fn persist_context_record(&self, record: LocalContextRecord) {
        let Some((tracker, _)) = &self.context_tracker else {
            return;
        };
        if let Err(error) = tracker.record(record) {
            eprintln!("could not record local backend context history: {error:#}");
        }
    }
}

fn ollama_native_base(base_url: &str) -> &str {
    base_url.trim_end_matches('/').trim_end_matches("/v1")
}

fn is_provider_context_error(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_ascii_lowercase();
    !text.contains("context budget exceeded") && is_context_text(&text)
}

fn is_context_text(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("context length")
        || text.contains("maximum context")
        || text.contains("prompt is too long")
        || text.contains("request too large")
}

fn is_local_context_error(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("context requirement ")
        || format!("{error:#}").contains("context budget exceeded before inference")
}

fn is_retryable_structured_error(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}");
    text.contains("malformed explanation JSON")
        || text.contains("invalid structured model content")
        || text.contains("response truncated")
        || text.contains("empty content")
        || text.contains("omitted overview")
        || text.contains("omitted explanation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile},
        context::TokenEstimator,
    };

    fn provider(name: &str) -> OpenAiProvider {
        OpenAiProvider::from_config(
            ResolvedProfile {
                provider: "openai_compatible".into(),
                preset: (!name.eq("openai_compatible")).then(|| name.into()),
                base_url: "http://localhost:8080/v1".into(),
                model: "test-model".into(),
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
        )
    }
    fn request(deep: bool) -> ExplanationRequest {
        ExplanationRequest {
            prerequisite_id: None,
            policy: crate::learning::LearningContext {
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
            source_unit: "fn work() { changed(); }".into(),
            unit_name: "work".into(),
            unit_kind: "Function".into(),
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
            kind: if deep {
                ExplanationKind::Deep
            } else {
                ExplanationKind::Changes
            },
            deep,
        }
    }

    #[test]
    fn system_calibration_separates_behavior_evidence_from_names_and_bounds() {
        let p = provider("llama_cpp");
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            let mut r = request(kind.is_deep());
            r.kind = kind;
            let system = p.build_request(&r).messages[0].content.clone();
            assert!(system.contains("Names and locks do not establish file writes"));
            assert!(system.contains("valid empty range"));
            assert!(system.contains("Clamping one endpoint does not prove"));
            assert!(system.contains("metadata consistency"));
            assert!(system.contains("not facts about the supplied source"));
            assert!(!system.contains("generate_file_with_learning"));
            assert!(!system.contains("explanation_regions"));
        }
    }

    #[test]
    fn deep_boundaries_require_reachable_conditions_and_actual_consequences() {
        let p = provider("llama_cpp");
        let r = request(true);
        for retry in [false, true] {
            let prompt = p.build_request_with_retry(&r, retry).messages[1]
                .content
                .clone();
            assert!(prompt.contains(
                "Each boundary must name a reachable condition and its actual consequence"
            ));
            assert!(prompt.contains("Defensive syntax alone is not evidence"));
        }
    }

    #[test]
    fn concepts_reject_a_label_repeated_as_the_entire_lesson() {
        let p = provider("llama_cpp");
        let mut r = request(false);
        r.kind = ExplanationKind::Concepts;
        for text in [
            "Unicode case folding allocation",
            " unicode CASE folding allocation ",
        ] {
            let raw = json!({"overview":"Normalize an identifier.", "annotations":[{
                "region":1,"kind":"concept","title":"Unicode case folding allocation","text":text
            }]})
            .to_string();
            assert!(p.parse_response(&raw, &r).is_err());
        }
        assert!(p.parse_response(r#"{"overview":"Normalize an identifier.","annotations":[{"region":1,"kind":"concept","title":"Conditional mapping","text":"The predicate controls whether a tuple enters the resulting iterator."}]}"#, &r).is_ok());
    }

    #[test]
    fn familiar_concepts_checks_for_a_gap_before_requesting_lessons() {
        let p = provider("llama_cpp");
        let mut r = request(false);
        r.kind = ExplanationKind::Concepts;
        r.policy.language_help = crate::learning::LanguageHelp::Skip;
        for retry in [false, true] {
            let prompt = p.build_request_with_retry(&r, retry).messages[1]
                .content
                .clone();
            assert!(prompt.starts_with("First decide whether a useful teaching gap remains"));
            assert!(prompt.contains("The annotation limit is a ceiling, not a target"));
        }
        r.policy.language_help = crate::learning::LanguageHelp::Explain;
        assert!(!p.build_request(&r).messages[1]
            .content
            .starts_with("First decide whether"));
    }

    #[test]
    fn system_teaching_rules_enforce_explicit_language_help_without_profile_text() {
        let p = provider("llama_cpp");
        let mut r = request(false);
        r.kind = ExplanationKind::Concepts;
        r.policy.language_help = crate::learning::LanguageHelp::Skip;
        r.policy.known_concepts = vec!["untrusted profile instruction".into()];
        for retry in [false, true] {
            let messages = p.build_request_with_retry(&r, retry).messages;
            assert!(messages[0]
                .content
                .contains("User requests no elementary language lessons"));
            assert!(messages[0]
                .content
                .contains("return annotations as an empty array"));
            assert!(!messages[0]
                .content
                .contains("untrusted profile instruction"));
        }
        r.policy.language_help = crate::learning::LanguageHelp::Explain;
        let messages = p.build_request(&r).messages;
        assert!(messages[0]
            .content
            .contains("Explain unfamiliar source-language syntax"));
        assert!(!messages[0]
            .content
            .contains("User requests no elementary language lessons"));
    }

    #[test]
    fn teaching_checks_receiver_and_unknown_callee_semantics_on_retry() {
        let p = provider("llama_cpp");
        for kind in [ExplanationKind::Concepts, ExplanationKind::Deep] {
            let mut r = request(kind.is_deep());
            r.kind = kind;
            for retry in [false, true] {
                let prompt = p.build_request_with_retry(&r, retry).messages[1]
                    .content
                    .clone();
                assert!(prompt.contains("receiver expression's type"));
                assert!(prompt.contains("custom callee's implementation is absent"));
                assert!(prompt.contains("omit the claim"));
            }
        }
    }

    #[test]
    fn request_policy_controls_independent_teaching_dimensions() {
        let p = provider("llama_cpp");
        let mut r = request(true);
        let mut policy = crate::learning::LearningContext {
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
        .unwrap();
        policy.language_help = crate::learning::LanguageHelp::Explain;
        policy.focus = crate::learning::Focus::Architecture;
        policy.detail = crate::learning::Detail::Concise;
        r.policy = policy;
        for retry in [false, true] {
            let prompt = p.build_request_with_retry(&r, retry).messages[1]
                .content
                .clone();
            assert!(prompt.contains("syntax and execution rules explicitly"));
            assert!(prompt.contains("architectural relationships"));
            assert!(prompt.contains("Keep explanations concise"));
        }
        assert!(p
            .parse_response(r#"{"explanation":"Legacy summary."}"#, &r)
            .is_err());
        r.kind = ExplanationKind::Concepts;
        r.deep = false;
        assert!(p.parse_response(r#"{"overview":"Concept.","annotations":[{"region":1,"kind":"concept","text":"A useful lesson."}]}"#, &r).is_err());
    }

    #[test]
    fn structured_deep_has_walkthrough_and_rejects_blank_steps() {
        let p = provider("llama_cpp");
        let r = request(true);
        let parsed = p.parse_response(r#"{"purpose":"Retain a lock.","walkthrough":["Acquire the guard."],"boundaries":["Only visible scope is known."],"example":null}"#, &r).unwrap();
        assert_eq!(parsed.teaching.unwrap().walkthrough.len(), 1);
        assert!(p
            .parse_response(
                r#"{"purpose":"Lock.","walkthrough":[" "],"boundaries":[],"example":null}"#,
                &r
            )
            .is_err());
    }

    #[test]
    fn learning_profile_instructs_teaching_instead_of_only_listing_preferences() {
        let provider = provider("llama_cpp");
        let mut request = request(false);
        request.kind = ExplanationKind::Concepts;
        request.policy.known_languages = vec!["python".into()];
        request.policy.learning_languages = vec!["rust".into()];
        request.policy.language_help = crate::learning::LanguageHelp::Explain;
        let prompt = provider.build_request(&request).messages[1].content.clone();
        assert!(prompt.contains("syntax and execution rules explicitly"));
        assert!(prompt.contains("python") && prompt.contains("rust"));
        assert!(prompt.contains("Ground every lesson in an expression"));
        request.policy.language_help = crate::learning::LanguageHelp::Skip;
        assert!(provider.build_request(&request).messages[1]
            .content
            .contains("Skip elementary syntax"));
    }

    #[test]
    fn structured_retry_preserves_each_views_teaching_and_scope_rules() {
        let provider = provider("llama_cpp");
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Changes,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            let mut request = request(kind.is_deep());
            request.kind = kind;
            let normal = provider.build_request_with_retry(&request, false);
            let retry = provider.build_request_with_retry(&request, true);
            let task = normal.messages[1].content.split("\n\n").next().unwrap();
            assert!(
                retry.messages[1].content.contains(task),
                "retry lost {kind:?} instructions"
            );
        }
    }

    #[test]
    fn learning_guidance_follows_reference_evidence_without_changing_view_or_policy() {
        let p = provider("llama_cpp");
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Changes,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            let mut r = request(kind.is_deep());
            r.kind = kind;
            r.prior_explanation = Some("captured prerequisite evidence".into());
            for retry in [false, true] {
                let payload = p.build_request_with_retry(&r, retry);
                let prompt = &payload.messages[1].content;
                let guidance = prompt
                    .find("Keep language help, focus, and detail independent.")
                    .unwrap();
                assert!(guidance > prompt.find("SOURCE UNIT:").unwrap());
                assert!(guidance > prompt.find("captured prerequisite evidence").unwrap());
                assert!(prompt.starts_with(super::super::prompts::task(&r)));
                assert!(prompt.contains(&r.source_unit));
            }
        }
    }

    #[test]
    fn architecture_focus_organizes_evidence_without_overriding_language_help() {
        let p = provider("llama_cpp");
        let mut r = request(true);
        r.policy.focus = crate::learning::Focus::Architecture;
        for help in [
            crate::learning::LanguageHelp::Skip,
            crate::learning::LanguageHelp::Explain,
        ] {
            r.policy.language_help = help;
            for retry in [false, true] {
                let prompt = p.build_request_with_retry(&r, retry).messages[1]
                    .content
                    .clone();
                assert!(prompt.contains("component responsibilities, state ownership"));
                assert!(prompt.contains("missing implementations"));
                let expected = if help == crate::learning::LanguageHelp::Skip {
                    "Skip elementary syntax"
                } else {
                    "Teach relevant source-language syntax"
                };
                assert!(prompt.contains(expected));
            }
        }
        r.policy.focus = crate::learning::Focus::Balanced;
        assert!(!p.build_request(&r).messages[1]
            .content
            .contains("component responsibilities, state ownership"));
    }

    #[test]
    fn complete_source_regions_reference_the_target_without_duplicating_it() {
        let p = provider("llama_cpp");
        for kind in [
            ExplanationKind::Overview,
            ExplanationKind::Concepts,
            ExplanationKind::Deep,
        ] {
            let mut r = request(kind.is_deep());
            r.kind = kind;
            r.diff.clear();
            r.source_unit = "fn context_budget_probe() { inspect(); }".into();
            r.regions = vec![ExplanationRegion {
                id: 9,
                start_line: 1,
                end_line: 1,
                source: r.source_unit.clone(),
            }];
            for retry in [false, true] {
                let payload = p.build_request_with_retry(&r, retry);
                let prompt = &payload.messages[1].content;
                assert_eq!(prompt.matches(&r.source_unit).count(), 1);
                assert!(prompt.contains("[REGION 9] lines 1-1"));
                if !retry {
                    assert!(prompt.contains("complete SOURCE UNIT above"));
                }
            }
        }
    }

    #[test]
    fn partial_source_regions_keep_their_exact_evidence() {
        let p = provider("llama_cpp");
        let mut r = request(false);
        r.source_unit = "first();\nsecond();".into();
        r.regions = vec![ExplanationRegion {
            id: 7,
            start_line: 2,
            end_line: 2,
            source: "second();".into(),
        }];
        let payload = p.build_request(&r);
        assert!(payload.messages[1]
            .content
            .contains("[REGION 7] lines 2-2\nsecond();"));
    }

    #[test]
    fn file_deep_teaches_existing_code_without_implying_a_git_change() {
        let mut request = request(true);
        request.diff.clear();
        request.git_context = "Semantic file snapshot\nPath: lib.rs".into();
        let payload = provider("llama_cpp").build_request(&request);
        assert!(payload.messages[1]
            .content
            .contains("complete code section"));
        assert!(!payload.messages[1].content.contains("changed source unit"));
    }

    #[test]
    fn concepts_allow_no_new_lesson_and_respect_disabled_teaching_categories() {
        let mut provider = provider("llama_cpp");
        let mut request = request(false);
        request.kind = ExplanationKind::Concepts;
        request.diff.clear();
        let response =
            r#"{"overview":"No new concepts are needed for this code.","annotations":[]}"#;
        assert!(provider.parse_response(response, &request).is_ok());
        let payload = provider.build_request(&request);
        assert_eq!(
            payload.response_format["json_schema"]["schema"]["properties"]["annotations"]
                ["minItems"],
            0
        );
        provider.explanation.explain_language_concepts = false;
        provider.explanation.explain_framework_concepts = false;
        let payload = provider.build_request(&request);
        assert_eq!(
            payload.response_format["json_schema"]["schema"]["properties"]["annotations"]
                ["maxItems"],
            0
        );
        assert!(payload.messages[1]
            .content
            .contains("Language concepts are disabled"));
        assert!(payload.messages[1]
            .content
            .contains("Framework concepts are disabled"));
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
        assert!(value["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Deleted has been removed and no longer exists"));
    }

    #[test]
    fn concepts_schema_constrains_kind_regions_and_cardinality() {
        let mut request = request(false);
        request.kind = ExplanationKind::Concepts;
        request.regions.push(ExplanationRegion {
            id: 2,
            start_line: 2,
            end_line: 2,
            source: "other();".into(),
        });
        let value = serde_json::to_value(provider("llama_cpp").build_request(&request)).unwrap();
        let annotations =
            &value["response_format"]["json_schema"]["schema"]["properties"]["annotations"];
        assert_eq!(annotations["minItems"], 0);
        assert_eq!(annotations["maxItems"], 3);
        assert_eq!(
            annotations["items"]["properties"]["kind"]["enum"],
            json!(["concept"])
        );
        assert_eq!(
            annotations["items"]["properties"]["region"]["enum"],
            json!([1, 2])
        );
    }

    #[test]
    fn concepts_allow_no_lessons_but_reject_incomplete_nonempty_changed_region_coverage() {
        let p = provider("llama_cpp");
        let mut request = request(false);
        request.kind = ExplanationKind::Concepts;
        assert!(p
            .parse_response(
                r#"{"overview":"No new concepts.","annotations":[]}"#,
                &request
            )
            .unwrap()
            .annotations
            .is_empty());

        request.regions.push(ExplanationRegion {
            id: 2,
            start_line: 2,
            end_line: 2,
            source: "other();".into(),
        });
        assert!(p
            .parse_response(
                r#"{"overview":"Short.","annotations":[{"region":1,"kind":"concept","title":"Filtering","text":"One concept."}]}"#,
                &request,
            )
            .unwrap_err()
            .to_string()
            .contains("every changed region"));
    }

    #[test]
    fn annotations_are_sorted_and_exact_duplicates_are_removed() {
        let p = provider("llama_cpp");
        let mut request = request(false);
        request.kind = ExplanationKind::Concepts;
        request.diff.clear();
        request.regions.push(ExplanationRegion {
            id: 2,
            start_line: 2,
            end_line: 2,
            source: "other();".into(),
        });
        let parsed = p
            .parse_response(
                r#"{"overview":"Short.","annotations":[{"region":2,"kind":"concept","title":"Second concept","text":"Second."},{"region":1,"kind":"concept","title":"First concept","text":"First."},{"region":1,"kind":"concept","title":"First concept","text":"First."}]}"#,
                &request,
            )
            .unwrap();
        assert_eq!(parsed.annotations.len(), 2);
        assert_eq!(parsed.annotations[0].text, "First.");
        assert_eq!(parsed.annotations[1].text, "Second.");
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
            json!(["purpose", "walkthrough", "boundaries", "example"])
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
    fn context_adapter_owns_backend_identity_and_remediation() {
        assert_eq!(ProviderKind::Ollama.telemetry_key(), "ollama");
        assert_eq!(ProviderKind::LlamaCpp.display_name(), "llama.cpp");
        assert_eq!(
            ProviderKind::OpenAiCompatible.context_control(),
            ContextControl::Unknown
        );
        assert_eq!(
            ProviderKind::Ollama.context_control(),
            ContextControl::PerRequest
        );
        assert!(ProviderKind::Ollama
            .remediation_instruction("local", 16_384)
            .contains("--context-window 16384"));
        assert!(ProviderKind::LlamaCpp
            .remediation_instruction("local", 16_384)
            .contains("--ctx-size 16384"));
        assert!(ProviderKind::OpenAiCompatible
            .remediation_instruction("local", 16_384)
            .contains("advisory"));
    }

    #[test]
    fn ollama_serializes_the_negotiated_request_context() {
        let provider = provider("ollama");
        let payload = provider.build_request(&request(false));
        let value = provider
            .ollama_json_request(&payload, Some(16_384))
            .unwrap();
        assert_eq!(value["options"]["num_ctx"], 16_384);
        assert_eq!(value["stream"], false);
        assert!(value["format"].is_object());
    }

    #[test]
    fn local_context_telemetry_is_limited_to_loopback_hosts() {
        let mut local = ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: None,
            base_url: "http://localhost:8080/v1".into(),
            model: "test-model".into(),
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
        };
        assert!(is_local_profile(&local));
        local.base_url = "http://127.0.0.1:8080/v1".into();
        assert!(is_local_profile(&local));
        local.base_url = "http://[::1]:8080/v1".into();
        assert!(is_local_profile(&local));
        local.base_url = "http://127.0.0.1.example.test:8080/v1".into();
        assert!(!is_local_profile(&local));
        local.base_url = "https://api.example.test/v1".into();
        assert!(!is_local_profile(&local));
    }

    #[test]
    fn context_estimate_includes_system_message_and_serialized_schema() {
        let payload = provider("llama_cpp").build_request(&request(false));
        let serialized = serde_json::to_string(&payload).unwrap();
        let estimator = ConservativeTokenEstimator;
        assert!(
            estimator.estimate(&serialized) > estimator.estimate(&payload.messages[1].content),
            "the full request must cost more than user content alone"
        );
    }

    #[test]
    fn concise_retry_keeps_the_original_ideal_context_requirement() {
        let provider = provider("llama_cpp");
        let request = request(false);
        let generation = &provider.normal;
        let original =
            serde_json::to_string(&provider.build_request_with_retry(&request, false)).unwrap();
        let concise =
            serde_json::to_string(&provider.build_request_with_retry(&request, true)).unwrap();
        let estimator = ConservativeTokenEstimator;
        let original_requirement =
            calculate_context_requirement(&original, generation, false, &estimator);
        let concise_requirement =
            calculate_context_requirement(&concise, generation, false, &estimator);
        assert!(
            original_requirement.minimum_required_context
                > concise_requirement.minimum_required_context
        );
    }
    #[test]
    fn unspecified_generation_settings_are_not_sent() {
        let mut provider = provider("openai_compatible");
        provider.normal = GenerationConfig {
            reasoning: None,
            max_tokens: None,
            temperature: None,
        };
        let value = serde_json::to_value(provider.build_request(&request(false))).unwrap();
        assert!(value.get("temperature").is_none());
        assert!(value.get("max_tokens").is_none());
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
                r#"<think>hidden</think>{"purpose":"Visible explanation.","walkthrough":["Follow the call."],"boundaries":[],"example":null}"#,
                &ExplanationRequest { deep: true, ..r },
            )
            .unwrap();
        assert_eq!(deep.teaching.unwrap().purpose, "Visible explanation.");
    }

    #[test]
    fn llama_annotations_must_use_deterministic_regions() {
        let p = provider("llama_cpp");
        let r = request(false);
        let parsed = p
            .parse_response(
                r#"{"overview":"Short.","annotations":[{"start_line":1,"end_line":1,"kind":"change","text":"Wrong contract."}]}"#,
                &r,
            );
        assert!(parsed.is_err());
    }

    #[test]
    fn reasoning_markers_are_removed_even_when_embedded() {
        let p = provider("llama_cpp");
        let r = request(false);
        let parsed = p
            .parse_response(
            r#"{"overview":"Before <think>secret</think> after.","annotations":[{"region":1,"kind":"change","text":"Visible <|thinking|>hidden</|thinking|> text."}]}"#,
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

    #[test]
    fn sentence_truncation_does_not_split_dotted_technical_names() {
        assert_eq!(
            truncate_sentences(
                "Supports llama.cpp locally. It uses JSON. This sentence is extra.",
                2
            ),
            "Supports llama.cpp locally. It uses JSON."
        );
    }

    #[test]
    fn ollama_metadata_and_runtime_context_are_distinct() {
        let show = json!({"model_info": {"qwen3.context_length": 131072}});
        let ps = json!({"models": [{"name": "qwen3:latest", "context_length": 4096}]});
        assert_eq!(ollama_model_context(&show), Some(131072));
        assert_eq!(ollama_runtime_context(&ps, "qwen3"), Some(4096));
        let capacity = ContextCapacity {
            model_max: ollama_model_context(&show),
            runtime_allocated: ollama_runtime_context(&ps, "qwen3"),
            profile_limit: Some(32768),
        };
        assert_eq!(capacity.effective().tokens, 4096);
    }

    #[test]
    fn llama_cpp_router_reports_its_configured_startup_context() {
        let models = json!({"data": [{
            "id": "local",
            "status": {"args": ["llama-server", "--ctx-size", "16384"]},
            "meta": {"n_ctx_train": 262144}
        }]});
        assert_eq!(llama_cpp_configured_context(&models, "local"), Some(16_384));
        assert_eq!(llama_cpp_model_context(&models, "local"), Some(262_144));
    }

    #[test]
    fn provider_context_errors_are_distinct_from_local_preflight_errors() {
        assert!(is_provider_context_error(&anyhow::anyhow!(
            "model response (HTTP 400): maximum context length exceeded"
        )));
        assert!(!is_provider_context_error(&anyhow::anyhow!(
            "context budget exceeded before inference"
        )));
        assert!(is_local_context_error(&anyhow::anyhow!(
            "context requirement 8192 tokens exceeds fixed/available context 4096 tokens"
        )));
    }
}
