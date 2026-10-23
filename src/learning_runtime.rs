//! Per-server learning settings and immutable explanation variants.
use crate::{
    config::{ConfigPaths, ReaderConfig},
    learning::{
        LearningContext, LearningSaveConflict, LearningSelection, LearningStore,
        ResolvedExplanationPolicy,
    },
    model::{ExplanationKind, ExplanationRequest, UnitExplanation},
};
use anyhow::Result;
use axum::http::{HeaderMap, StatusCode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Mutex};

pub struct LearningRuntime {
    pub paths: ConfigPaths,
    pub reader: ReaderConfig,
    pub selection: LearningSelection,
    token: String,
    results: Mutex<HashMap<String, LearningResult>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearningResult {
    pub policy: ResolvedExplanationPolicy,
    pub policy_id: String,
    pub result_id: String,
    pub prerequisite_id: Option<String>,
    pub explanation: UnitExplanation,
}
impl LearningResult {
    pub fn response(&self, cache_hit: bool, kind: ExplanationKind) -> Value {
        let mut value = crate::runtime::result(self.explanation.clone(), cache_hit, kind);
        value["policy"] = json!(self.policy);
        value["policy_id"] = json!(self.policy_id);
        value["result_id"] = json!(self.result_id);
        value["prerequisite_id"] = json!(self.prerequisite_id);
        value["teaching"] = json!(self.explanation.teaching);
        value
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningLookup {
    pub language: String,
    pub selection: Option<LearningSelection>,
    pub learning: Option<LearningSelection>,
    pub unit_id: String,
    pub kind: Option<ExplanationKind>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningSave {
    pub revision: String,
    pub config: crate::learning::LearningConfig,
}

impl LearningRuntime {
    pub fn decorate_article(
        html: &mut String,
        id: &str,
        states: &[(ExplanationKind, LearningResult)],
    ) {
        let marker = format!("<article data-unit-id=\"{id}\"");
        let Some(start) = html.find(&marker) else {
            return;
        };
        let Some(relative_end) = html[start..].find("</article>") else {
            return;
        };
        let end = start + relative_end;
        let escape = |value: &str| {
            value
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#39;")
        };
        let templates = states
            .iter()
            .map(|(kind, result)| {
                let name = format!("{kind:?}").to_lowercase();
                format!(
                    "<template data-layer-state=\"{name}\">{}</template>",
                    escape(&serde_json::to_string(&result.response(true, *kind)).unwrap())
                )
            })
            .collect::<String>();
        html.insert_str(end, &templates);
        // Overview is the initial server-rendered view. Other views remain inert.
        if let Some((_, result)) = states
            .iter()
            .find(|(kind, _)| *kind == ExplanationKind::Overview)
        {
            let article_end = html[start..].find("</article>").unwrap() + start;
            let mut article = html[start..article_end].to_string();
            if let Some(offset) = article.find("class=\"layer-summary explanation\">") {
                let content = offset + "class=\"layer-summary explanation\">".len();
                if let Some(end) = article[content..].find("</p>") {
                    article.replace_range(
                        content..content + end,
                        &escape(&result.explanation.overview),
                    );
                }
            }
            article = article.replace(
                "data-generated=\"false\">Generate selected explanation",
                "data-generated=\"true\">Regenerate explanation",
            );
            html.replace_range(start..article_end, &article);
        }
        if let Some((_, result)) = states
            .iter()
            .find(|(kind, _)| *kind == ExplanationKind::Deep)
        {
            if let Some(teaching) = &result.explanation.teaching {
                let article_end = html[start..].find("</article>").unwrap() + start;
                let mut article = html[start..article_end].to_string();
                if let Some(offset) = article.find("<div class=\"deep\">") {
                    let content = offset + "<div class=\"deep\">".len();
                    if let Some(end) = article[content..].find("</div>") {
                        article.replace_range(
                            content..content + end,
                            &crate::web::render_teaching(teaching),
                        );
                    }
                }
                html.replace_range(start..article_end, &article);
            }
        }
    }
    pub fn new(
        paths: ConfigPaths,
        reader: ReaderConfig,
        selection: LearningSelection,
    ) -> Result<Self> {
        LearningContext::load(&paths, &reader)?;
        let mut bytes = [0; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Ok(Self {
            paths,
            reader,
            selection,
            token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
            results: Mutex::new(HashMap::new()),
        })
    }
    pub fn inspect(&self) -> Result<Value> {
        let inspection = LearningStore::new(self.paths.user.clone()).inspect()?;
        let context = LearningContext::load(&self.paths, &self.reader)?;
        Ok(
            json!({"ok":true,"revision":inspection.revision,"config":inspection.config,"selection":self.selection,"token":self.token,"repository_available":context.repository_defaults.is_some()}),
        )
    }
    pub fn resolve(
        &self,
        language: &str,
        selection: Option<&LearningSelection>,
    ) -> Result<ResolvedExplanationPolicy> {
        LearningContext::load(&self.paths, &self.reader)?
            .resolve(language, selection.unwrap_or(&self.selection))
    }
    pub fn resolve_response(&self, body: &LearningLookup) -> Result<Value> {
        let context = LearningContext::load(&self.paths, &self.reader)?;
        let selection = body
            .selection
            .as_ref()
            .or(body.learning.as_ref())
            .unwrap_or(&self.selection);
        let policy = context.resolve(&body.language, selection)?;
        let provenance = context.provenance(&body.language, selection)?;
        Ok(json!({"ok":true,"policy_id":policy.id(),"policy":policy,"provenance":provenance}))
    }
    pub fn prepare(
        &self,
        request: &mut ExplanationRequest,
        selection: Option<&LearningSelection>,
    ) -> Result<()> {
        request.policy = self.resolve(&request.language, selection)?;
        Ok(())
    }
    pub fn cached(&self, key: &str) -> Option<LearningResult> {
        self.results.lock().unwrap().get(key).cloned()
    }
    pub fn restore(
        &self,
        key: &str,
        request: &ExplanationRequest,
        prerequisite_id: Option<String>,
        cache: Option<&crate::cache::ExplanationCache>,
    ) -> Option<LearningResult> {
        if let Some(value) = self.cached(key) {
            return Some(value);
        }
        let stored = cache?.get(key).ok()??;
        let result: LearningResult = serde_json::from_str(&stored).ok()?;
        if result.policy != request.policy || result.prerequisite_id != prerequisite_id {
            return None;
        }
        self.results
            .lock()
            .unwrap()
            .insert(key.into(), result.clone());
        Some(result)
    }
    pub fn remember(
        &self,
        key: &str,
        request: &ExplanationRequest,
        prerequisite_id: Option<String>,
        explanation: UnitExplanation,
    ) -> LearningResult {
        let policy = request.policy.clone();
        let mut generation_nonce = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut generation_nonce);
        let bytes = serde_json::to_vec(&(key, &explanation, &prerequisite_id, generation_nonce))
            .expect("result serialization");
        let result = LearningResult {
            policy_id: policy.id(),
            policy,
            result_id: format!("{:x}", Sha256::digest(bytes)),
            prerequisite_id,
            explanation,
        };
        self.results
            .lock()
            .unwrap()
            .insert(key.into(), result.clone());
        result
    }
    pub fn save(&self, headers: &HeaderMap, body: LearningSave) -> (StatusCode, Value) {
        if !self.authorized(headers) {
            return (
                StatusCode::FORBIDDEN,
                json!({"ok":false,"error":"Learning settings require this page's settings token and origin."}),
            );
        }
        match LearningStore::new(self.paths.user.clone()).save(&body.revision, body.config) {
            Ok(value) => (
                StatusCode::OK,
                json!({"ok":true,"revision":value.revision,"config":value.config}),
            ),
            Err(error) => {
                let conflict = error.downcast_ref::<LearningSaveConflict>().is_some();
                (
                    if conflict {
                        StatusCode::CONFLICT
                    } else {
                        StatusCode::BAD_REQUEST
                    },
                    json!({"ok":false,"error":error.to_string()}),
                )
            }
        }
    }
    fn authorized(&self, headers: &HeaderMap) -> bool {
        let text = |key: &str| headers.get(key).and_then(|v| v.to_str().ok()).unwrap_or("");
        if text("x-git-explain-learning-token") != self.token
            || !text("content-type")
                .split(';')
                .next()
                .is_some_and(|v| v.trim() == "application/json")
        {
            return false;
        }
        let host = text("host");
        let Ok(url) = reqwest::Url::parse(&format!("http://{host}")) else {
            return false;
        };
        matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            && text("origin") == format!("http://{host}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime() -> (tempfile::TempDir, LearningRuntime) {
        let d = tempfile::tempdir().unwrap();
        let reader = ReaderConfig {
            experience: "experienced".into(),
            known_languages: vec![],
            learning_languages: vec![],
            known_frameworks: vec![],
            learning_frameworks: vec![],
        };
        let r = LearningRuntime::new(
            ConfigPaths {
                user: d.path().join("config.toml"),
                repository: None,
            },
            reader,
            Default::default(),
        )
        .unwrap();
        (d, r)
    }
    fn auth_headers(r: &LearningRuntime) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("host", "127.0.0.1:8192".parse().unwrap());
        h.insert("origin", "http://127.0.0.1:8192".parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        h.insert("x-git-explain-learning-token", r.token.parse().unwrap());
        h
    }
    #[test]
    fn settings_writes_require_distinct_token_and_same_origin() {
        let (_d, r) = runtime();
        let mut h = HeaderMap::new();
        h.insert("host", "127.0.0.1:8192".parse().unwrap());
        h.insert("origin", "http://127.0.0.1:8192".parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        assert!(!r.authorized(&h));
        h.insert("x-git-explain-learning-token", r.token.parse().unwrap());
        assert!(r.authorized(&h));
        h.insert("origin", "http://other.test".parse().unwrap());
        assert!(!r.authorized(&h));
    }
    #[test]
    fn resolve_does_not_create_results_or_modify_preferences() {
        let (_d, r) = runtime();
        let before = r.inspect().unwrap();
        let policy = r.resolve("Rust", None).unwrap();
        assert_eq!(policy.language, "rust");
        assert_eq!(before["revision"], r.inspect().unwrap()["revision"]);
        assert!(r.cached("missing").is_none());
    }
    #[test]
    fn save_contention_returns_conflict_and_allows_retry_and_detects_stale_revision() {
        use fs2::FileExt;
        let (_d, r) = runtime();
        let headers = auth_headers(&r);
        let before = r.inspect().unwrap();
        let before_revision = before["revision"].as_str().unwrap().to_string();
        let mut new_config = crate::learning::LearningConfig::default();
        new_config.defaults.detail = Some(crate::learning::Detail::Detailed);
        let payload = LearningSave {
            revision: before_revision.clone(),
            config: new_config,
        };

        let lock_path = r.paths.user.with_extension("learning.lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let lock_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        lock_file.lock_exclusive().unwrap();

        let (status, resp) = r.save(&headers, payload.clone());
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(resp["ok"], false);
        assert_eq!(
            resp["error"],
            "learning preferences are being edited; retry after the other save completes"
        );
        let unchanged = r.inspect().unwrap();
        assert_eq!(unchanged["revision"], before_revision);
        assert_eq!(
            unchanged["config"]["defaults"]["detail"],
            serde_json::Value::Null
        );

        lock_file.unlock().unwrap();
        drop(lock_file);

        let (status, resp) = r.save(&headers, payload.clone());
        assert_eq!(status, StatusCode::OK);
        assert_eq!(resp["ok"], true);
        assert_ne!(resp["revision"], before_revision);

        let (status, stale_resp) = r.save(&headers, payload);
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(stale_resp["ok"], false);
        assert_eq!(
            stale_resp["error"],
            "learning preferences changed; reload before saving"
        );
    }
    #[test]
    fn save_validation_error_for_profile_named_changed_returns_bad_request() {
        let (_d, r) = runtime();
        let headers = auth_headers(&r);
        let revision = r.inspect().unwrap()["revision"]
            .as_str()
            .unwrap()
            .to_string();
        let mut invalid_config = crate::learning::LearningConfig::default();
        invalid_config.profile = Some("changed".into());
        let payload = LearningSave {
            revision,
            config: invalid_config,
        };
        let (status, resp) = r.save(&headers, payload);
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(resp["ok"], false);
        assert_eq!(resp["error"], "unknown learning profile 'changed'");
    }
}
