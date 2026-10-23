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
pub const PROMPT: &str = "explanation-prompt-v20";
pub const RESPONSE_CONTRACT: &str = "structured-response-v9";

#[derive(Clone)]
pub struct ExplanationCache {
    path: PathBuf,
    db: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug, Serialize)]
struct Identity<'a> {
    prerequisite_id: Option<&'a str>,
    learning: &'a crate::learning::ResolvedExplanationPolicy,
    schema: &'static str,
    prompt: &'static str,
    response_contract: &'static str,
    context_plan: &'static str,
    scope: &'static str,
    provider: &'a str,
    model: &'a str,
    mode: &'static str,
    explanation_kind: crate::model::ExplanationKind,
    generation: &'a GenerationConfig,
    explanation: &'a ExplanationConfig,
    git_context: &'a str,
    language: &'a str,
    kind: &'a str,
    name: &'a str,
    source: &'a str,
    diff: &'a str,
    prior: Option<&'a str>,
    regions: Vec<RegionIdentity>,
    evidence_version: &'a str,
    evidence: &'a [crate::language::facts::SourceFact],
}
#[derive(Clone, Debug, Serialize)]
struct RegionIdentity {
    id: usize,
    coverage_id: usize,
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
    pub fn change_key(
        request: &ExplanationRequest,
        model: &ResolvedProfile,
        reader: &ReaderConfig,
        explanation: &ExplanationConfig,
    ) -> String {
        Self::key_for_scope("change", request, model, reader, explanation)
    }

    pub fn snapshot_key(
        request: &ExplanationRequest,
        model: &ResolvedProfile,
        reader: &ReaderConfig,
        explanation: &ExplanationConfig,
    ) -> String {
        Self::key_for_scope("snapshot", request, model, reader, explanation)
    }

    fn key_for_scope(
        scope: &'static str,
        request: &ExplanationRequest,
        model: &ResolvedProfile,
        _reader: &ReaderConfig,
        explanation: &ExplanationConfig,
    ) -> String {
        let generation = if request.kind.is_deep() {
            &model.deep
        } else {
            &model.normal
        };
        let evidence = request.source_evidence();
        let identity = Identity {
            prerequisite_id: request.prerequisite_id.as_deref(),
            learning: &request.policy,
            schema: SCHEMA,
            prompt: PROMPT,
            response_contract: RESPONSE_CONTRACT,
            context_plan: PLAN_VERSION,
            scope,
            provider: &model.provider,
            model: &model.model,
            mode: if request.kind.is_deep() {
                "deep"
            } else {
                "normal"
            },
            explanation_kind: request.kind,
            generation,
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
                    coverage_id: r.coverage_id,
                    start: r.start_line,
                    end: r.end_line,
                    source: r.source.clone(),
                })
                .collect(),
            evidence_version: &evidence.version,
            evidence: &evidence.facts,
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
        db.execute("INSERT INTO explanations(key,mode,provider,model,language,unit_kind,unit_name,prompt_version,response_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(key) DO UPDATE SET response_json=excluded.response_json,created_at=CURRENT_TIMESTAMP", params![key, format!("{:?}", request.kind).to_lowercase(), model.provider, model.model, request.language, request.unit_kind, request.unit_name, PROMPT, response])?;
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
            prerequisite_id: None,
            policy: Default::default(),
            source_unit: "fn load() {}".into(),
            unit_name: "load".into(),
            unit_kind: "Function".into(),
            diff: "+load".into(),
            language: "Rust".into(),
            git_context: "working tree".into(),
            regions: vec![ExplanationRegion {
                id: 1,
                coverage_id: 1,
                start_line: 1,
                end_line: 1,
                source: "fn load() {}".into(),
            }],
            prior_explanation: None,
            kind: crate::model::ExplanationKind::Changes,
            deep: false,
            evidence: Default::default(),
        };
        (cache, model, reader, explanation, request)
    }

    #[test]
    fn default_and_precomputed_evidence_have_the_same_effective_cache_identity() {
        let (_, model, reader, explanation, mut request) = setup();
        request.source_unit = "fn load() { run(); }".into();
        let missing = ExplanationCache::change_key(&request, &model, &reader, &explanation);
        request.evidence =
            crate::language::facts::extract_evidence(&request.language, &request.source_unit);
        assert_eq!(
            missing,
            ExplanationCache::change_key(&request, &model, &reader, &explanation)
        );
    }

    #[test]
    fn cache_key_uses_effective_content_and_rejects_stale_evidence_policy() {
        let (_, model, reader, explanation, mut request) = setup();
        request.source_unit = "fn load() { run(); }".into();
        request.evidence =
            crate::language::facts::extract_evidence(&request.language, &request.source_unit);
        let base = ExplanationCache::change_key(&request, &model, &reader, &explanation);
        let mut changed_facts = request.clone();
        changed_facts.evidence.facts.clear();
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed_facts, &model, &reader, &explanation)
        );
        let mut changed_version = request.clone();
        changed_version.evidence.version = "outdated-evidence-policy".into();
        changed_version.evidence.facts.clear();
        assert_eq!(
            base,
            ExplanationCache::change_key(&changed_version, &model, &reader, &explanation)
        );
    }

    #[test]
    fn key_changes_for_explanation_inputs_and_modes() {
        let (_, model, reader, explanation, request) = setup();
        let base = ExplanationCache::change_key(&request, &model, &reader, &explanation);
        let mut changed = request.clone();
        changed.source_unit.push(' ');
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed, &model, &reader, &explanation)
        );
        let mut changed = request.clone();
        changed.diff.push('x');
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed, &model, &reader, &explanation)
        );
        let mut changed = request.clone();
        changed.git_context.push('x');
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed, &model, &reader, &explanation)
        );
        let mut deep = request;
        deep.kind = crate::model::ExplanationKind::Deep;
        deep.deep = true;
        assert_ne!(
            base,
            ExplanationCache::change_key(&deep, &model, &reader, &explanation)
        );
    }

    #[test]
    fn normal_and_deep_entries_are_independent() {
        let (cache, model, reader, explanation, normal) = setup();
        let normal_key = ExplanationCache::change_key(&normal, &model, &reader, &explanation);
        cache
            .put(
                &normal_key,
                &normal,
                &model,
                r#"{"overview":"normal","annotations":[],"deep":null}"#,
            )
            .unwrap();
        let mut deep = normal;
        deep.kind = crate::model::ExplanationKind::Deep;
        deep.deep = true;
        let deep_key = ExplanationCache::change_key(&deep, &model, &reader, &explanation);
        assert_ne!(normal_key, deep_key);
        assert!(cache.get(&deep_key).unwrap().is_none());
        assert_eq!(cache.count().unwrap(), 1);
    }

    #[test]
    fn cache_key_changes_for_region_coverage_id_and_target_bounds() {
        let (_, model, reader, explanation, request) = setup();
        let base = ExplanationCache::change_key(&request, &model, &reader, &explanation);

        let mut changed_coverage = request.clone();
        changed_coverage.regions[0].coverage_id = 99;
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed_coverage, &model, &reader, &explanation)
        );

        let mut changed_bounds = request.clone();
        changed_bounds.regions[0].end_line = 2;
        assert_ne!(
            base,
            ExplanationCache::change_key(&changed_bounds, &model, &reader, &explanation)
        );
    }

    #[test]
    fn every_explanation_kind_has_an_independent_cache_key() {
        let (_, model, reader, explanation, mut request) = setup();
        let mut keys = std::collections::BTreeSet::new();
        for kind in [
            crate::model::ExplanationKind::Overview,
            crate::model::ExplanationKind::Changes,
            crate::model::ExplanationKind::Concepts,
            crate::model::ExplanationKind::Deep,
        ] {
            request.kind = kind;
            request.deep = kind.is_deep();
            keys.insert(ExplanationCache::change_key(
                &request,
                &model,
                &reader,
                &explanation,
            ));
        }
        assert_eq!(keys.len(), 4);
    }

    #[test]
    fn effective_learning_policy_is_part_of_both_cache_namespaces() {
        let (_, model, reader, explanation, request) = setup();
        let first = ExplanationCache::change_key(&request, &model, &reader, &explanation);
        let snapshot = ExplanationCache::snapshot_key(&request, &model, &reader, &explanation);
        let mut changed = request.clone();
        changed.policy.focus = crate::learning::Focus::Reasoning;
        assert_ne!(
            first,
            ExplanationCache::change_key(&changed, &model, &reader, &explanation)
        );
        assert_ne!(
            snapshot,
            ExplanationCache::snapshot_key(&changed, &model, &reader, &explanation)
        );
        // Obsolete reader scaffolding cannot split identical resolved policies.
        let mut unused_reader = reader.clone();
        unused_reader.known_languages = vec!["Python".into()];
        assert_eq!(
            first,
            ExplanationCache::change_key(&request, &model, &unused_reader, &explanation)
        );
    }

    #[test]
    fn deep_cache_tracks_prerequisite_identity_even_when_text_is_unchanged() {
        let (_, model, reader, explanation, mut request) = setup();
        request.kind = crate::model::ExplanationKind::Deep;
        request.deep = true;
        request.prior_explanation = Some("Identical overview text.".into());
        request.prerequisite_id = Some("first-result".into());
        let first_change = ExplanationCache::change_key(&request, &model, &reader, &explanation);
        let first_snapshot =
            ExplanationCache::snapshot_key(&request, &model, &reader, &explanation);
        request.prerequisite_id = Some("regenerated-result".into());
        assert_ne!(
            first_change,
            ExplanationCache::change_key(&request, &model, &reader, &explanation)
        );
        assert_ne!(
            first_snapshot,
            ExplanationCache::snapshot_key(&request, &model, &reader, &explanation)
        );
    }

    #[test]
    fn snapshot_and_change_cache_namespaces_never_collide() {
        let (_, model, reader, explanation, request) = setup();
        assert_ne!(
            ExplanationCache::snapshot_key(&request, &model, &reader, &explanation),
            ExplanationCache::change_key(&request, &model, &reader, &explanation)
        );
    }
}
