use crate::{
    config::{ExplanationConfig, GenerationConfig, ModelConfig, ReaderConfig},
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
const PROMPT: &str = "source-unit-on-demand-v1";

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
        model: &ModelConfig,
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
        model: &ModelConfig,
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
