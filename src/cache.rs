use crate::{
    config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile},
    context::PLAN_VERSION,
    model::ExplanationRequest,
};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const SCHEMA: &str = "1";
const PROMPT: &str = PLAN_VERSION;

#[derive(Clone)]
pub struct ExplanationCache {
    path: PathBuf,
    db: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug, Serialize)]
struct Identity<'a> {
    schema: &'static str,
    prompt: &'static str,
    provider: &'a str,
    model: &'a str,
    mode: &'static str,
    generation: &'a GenerationConfig,
    reader: &'a ReaderConfig,
    explanation: &'a ExplanationConfig,
    git_context: &'a str,
    language: &'a str,
    kind: &'a str,
    name: &'a str,
    source: &'a str,
    diff: &'a str,
    prior: Option<&'a str>,
    regions: Vec<RegionIdentity>,
}
#[derive(Clone, Debug, Serialize)]
struct RegionIdentity {
    id: usize,
    start: usize,
    end: usize,
    source: String,
}

impl ExplanationCache {
    pub fn open(git_dir: &Path) -> Result<Self> {
        let directory = git_dir.join("git-explain");
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("create {}", directory.display()))?;
        let path = directory.join("cache.sqlite");
        let db = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS explanations (key TEXT PRIMARY KEY, mode TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, language TEXT NOT NULL, unit_kind TEXT NOT NULL, unit_name TEXT NOT NULL, prompt_version TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, response_json TEXT NOT NULL, duration_ms INTEGER, prompt_tokens INTEGER, completion_tokens INTEGER);")?;
        Ok(Self {
            path,
            db: Arc::new(Mutex::new(db)),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn key(
        request: &ExplanationRequest,
        model: &ResolvedProfile,
        reader: &ReaderConfig,
        explanation: &ExplanationConfig,
    ) -> String {
        let generation = if request.deep {
            &model.deep
        } else {
            &model.normal
        };
        let identity = Identity {
            schema: SCHEMA,
            prompt: PROMPT,
            provider: &model.provider,
            model: &model.model,
            mode: if request.deep { "deep" } else { "normal" },
            generation,
            reader,
            explanation,
            git_context: &request.git_context,
            language: &request.language,
            kind: &request.unit_kind,
            name: &request.unit_name,
            source: &request.source_unit,
            diff: &request.diff,
            prior: request.prior_explanation.as_deref(),
            regions: request
                .regions
                .iter()
                .map(|r| RegionIdentity {
                    id: r.id,
                    start: r.start_line,
                    end: r.end_line,
                    source: r.source.clone(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&identity).expect("cache identity serializes");
        format!("{:x}", Sha256::digest(bytes))
    }
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let db = self.db.lock().unwrap();
        Ok(db
            .query_row(
                "SELECT response_json FROM explanations WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn put(
        &self,
        key: &str,
        request: &ExplanationRequest,
        model: &ResolvedProfile,
        response: &str,
    ) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.execute("INSERT INTO explanations(key,mode,provider,model,language,unit_kind,unit_name,prompt_version,response_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(key) DO UPDATE SET response_json=excluded.response_json,created_at=CURRENT_TIMESTAMP", params![key, if request.deep {"deep"} else {"normal"}, model.provider, model.model, request.language, request.unit_kind, request.unit_name, PROMPT, response])?;
        Ok(())
    }
    pub fn count(&self) -> Result<u64> {
        let db = self.db.lock().unwrap();
        Ok(db.query_row("SELECT COUNT(*) FROM explanations", [], |r| {
            r.get::<_, u64>(0)
        })?)
    }
    pub fn clear(&self) -> Result<u64> {
        let db = self.db.lock().unwrap();
        Ok(db.execute("DELETE FROM explanations", [])? as u64)
    }
}
trait OptionalRow<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}
impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExplanationConfig, GenerationConfig, ReaderConfig, ResolvedProfile};
    use crate::model::ExplanationRegion;
    use tempfile::tempdir;

    fn setup() -> (
        ExplanationCache,
        ResolvedProfile,
        ReaderConfig,
        ExplanationConfig,
        ExplanationRequest,
    ) {
        let dir = tempdir().unwrap();
        let cache = ExplanationCache::open(dir.path()).unwrap();
        std::mem::forget(dir);
        let model = ResolvedProfile {
            provider: "openai_compatible".into(),
            preset: Some("llama_cpp".into()),
            base_url: "http://localhost".into(),
            model: "m".into(),
            api_key_env: None,
            api_key: None,
            context_window: None,
            normal: GenerationConfig {
                reasoning: Some(false),
                max_tokens: Some(10),
                temperature: Some(0.2),
            },
            deep: GenerationConfig {
                reasoning: Some(true),
                max_tokens: Some(20),
                temperature: Some(0.3),
            },
        };
        let reader = ReaderConfig {
            experience: "experienced".into(),
            known_languages: vec![],
            learning_languages: vec![],
            known_frameworks: vec![],
            learning_frameworks: vec![],
        };
        let explanation = ExplanationConfig {
            default_depth: "normal".into(),
            max_annotations: 3,
            max_annotation_words: 60,
            explain_language_concepts: true,
            explain_framework_concepts: true,
            infer_intent: false,
        };
        let request = ExplanationRequest {
            source_unit: "fn load() {}".into(),
            unit_name: "load".into(),
            unit_kind: "Function".into(),
            diff: "+load".into(),
            language: "Rust".into(),
            git_context: "working tree".into(),
            regions: vec![ExplanationRegion {
                id: 1,
                start_line: 1,
                end_line: 1,
                source: "fn load() {}".into(),
            }],
            prior_explanation: None,
            deep: false,
        };
        (cache, model, reader, explanation, request)
    }

    #[test]
    fn key_changes_for_explanation_inputs_and_modes() {
        let (_, model, reader, explanation, request) = setup();
        let base = ExplanationCache::key(&request, &model, &reader, &explanation);
        let mut changed = request.clone();
        changed.source_unit.push(' ');
        assert_ne!(
            base,
            ExplanationCache::key(&changed, &model, &reader, &explanation)
        );
        let mut changed = request.clone();
        changed.diff.push('x');
        assert_ne!(
            base,
            ExplanationCache::key(&changed, &model, &reader, &explanation)
        );
        let mut changed = request.clone();
        changed.git_context.push('x');
        assert_ne!(
            base,
            ExplanationCache::key(&changed, &model, &reader, &explanation)
        );
        let mut deep = request;
        deep.deep = true;
        assert_ne!(
            base,
            ExplanationCache::key(&deep, &model, &reader, &explanation)
        );
    }

    #[test]
    fn normal_and_deep_entries_are_independent() {
        let (cache, model, reader, explanation, normal) = setup();
        let normal_key = ExplanationCache::key(&normal, &model, &reader, &explanation);
        cache
            .put(
                &normal_key,
                &normal,
                &model,
                r#"{"overview":"normal","annotations":[],"deep":null}"#,
            )
            .unwrap();
        let mut deep = normal;
        deep.deep = true;
        let deep_key = ExplanationCache::key(&deep, &model, &reader, &explanation);
        assert_ne!(normal_key, deep_key);
        assert!(cache.get(&deep_key).unwrap().is_none());
        assert_eq!(cache.count().unwrap(), 1);
    }
}
