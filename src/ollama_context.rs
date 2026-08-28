//! Local, metadata-only Ollama workload history and advisory context policy.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const HISTORY_LIMIT: usize = 100;
pub const MIN_SAMPLES: usize = 10;
const TIERS: &[u32] = &[4_096, 8_192, 16_384, 32_768, 65_536, 131_072];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OllamaRequestRecord {
    pub timestamp_ms: u64,
    pub profile: String,
    pub model: String,
    pub deep: bool,
    pub model_max: Option<u32>,
    pub runtime_context: Option<u32>,
    pub profile_limit: Option<u32>,
    pub effective_context: u32,
    pub estimated_input: u32,
    pub output_reserve: u32,
    pub safety_margin: u32,
    pub ideal_required_context: u32,
    pub final_required_context: u32,
    pub compacted: bool,
    pub actual_prompt_tokens: Option<u32>,
    pub actual_completion_tokens: Option<u32>,
    pub latency_ms: Option<u64>,
    pub success: bool,
    pub local_context_failure: bool,
    pub provider_context_overflow: bool,
    pub output_truncated: bool,
    pub concise_retry_used: bool,
    pub attempts: u8,
}

impl OllamaRequestRecord {
    pub fn now(profile: String, model: String, deep: bool) -> Self {
        Self {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            profile,
            model,
            deep,
            model_max: None,
            runtime_context: None,
            profile_limit: None,
            effective_context: 0,
            estimated_input: 0,
            output_reserve: 0,
            safety_margin: 0,
            ideal_required_context: 0,
            final_required_context: 0,
            compacted: false,
            actual_prompt_tokens: None,
            actual_completion_tokens: None,
            latency_ms: None,
            success: false,
            local_context_failure: false,
            provider_context_overflow: false,
            output_truncated: false,
            concise_retry_used: false,
            attempts: 1,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct History {
    records: Vec<OllamaRequestRecord>,
}

#[derive(Clone, Debug)]
pub struct OllamaRequestTracker {
    path: PathBuf,
}

impl OllamaRequestTracker {
    pub fn for_user_config(config_path: &Path) -> Self {
        let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
        Self {
            path: parent.join("state").join("ollama-context-history.json"),
        }
    }
    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn records(&self, profile: &str, deep: bool) -> Vec<OllamaRequestRecord> {
        self.load()
            .records
            .into_iter()
            .filter(|r| r.profile == profile && r.deep == deep)
            .collect()
    }
    pub fn record(&self, record: OllamaRequestRecord) -> Result<()> {
        let mut history = self.load();
        history.records.push(record);
        let mut groups: HashMap<(String, bool), Vec<OllamaRequestRecord>> = HashMap::new();
        for record in history.records {
            groups
                .entry((record.profile.clone(), record.deep))
                .or_default()
                .push(record);
        }
        let mut records = Vec::new();
        for (_, mut group) in groups {
            group.sort_by_key(|record| record.timestamp_ms);
            let start = group.len().saturating_sub(HISTORY_LIMIT);
            records.extend(group.drain(start..));
        }
        records.sort_by_key(|record| record.timestamp_ms);
        self.write(&History { records })
    }
    pub fn reset(&self, profile: &str) -> Result<usize> {
        let mut history = self.load();
        let before = history.records.len();
        history.records.retain(|record| record.profile != profile);
        self.write(&history)?;
        Ok(before - history.records.len())
    }
    fn load(&self) -> History {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
    fn write(&self, history: &History) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let temporary = self.path.with_extension("tmp");
        let content = serde_json::to_vec(history)?;
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        file.write_all(&content)?;
        file.sync_all()?;
        fs::rename(&temporary, &self.path)
            .with_context(|| format!("replace {}", self.path.display()))
    }
}

#[derive(Clone, Debug, Default)]
pub struct OllamaContextStatistics {
    pub count: usize,
    pub required_p50: Option<u32>,
    pub required_p90: Option<u32>,
    pub required_p95: Option<u32>,
    pub required_max: Option<u32>,
    pub estimated_p50: Option<u32>,
    pub estimated_p95: Option<u32>,
    pub compactions: usize,
    pub hard_failures: usize,
    pub overflows: usize,
    pub truncations: usize,
    pub average_latency_ms: Option<u64>,
}
impl OllamaContextStatistics {
    pub fn from_records(records: &[OllamaRequestRecord]) -> Self {
        let mut required = records
            .iter()
            .map(|r| r.ideal_required_context)
            .collect::<Vec<_>>();
        let mut estimated = records
            .iter()
            .map(|r| r.estimated_input)
            .collect::<Vec<_>>();
        required.sort_unstable();
        estimated.sort_unstable();
        let latencies = records
            .iter()
            .filter_map(|r| r.latency_ms)
            .collect::<Vec<_>>();
        Self {
            count: records.len(),
            required_p50: percentile(&required, 50),
            required_p90: percentile(&required, 90),
            required_p95: percentile(&required, 95),
            required_max: required.last().copied(),
            estimated_p50: percentile(&estimated, 50),
            estimated_p95: percentile(&estimated, 95),
            compactions: records.iter().filter(|r| r.compacted).count(),
            hard_failures: records.iter().filter(|r| r.local_context_failure).count(),
            overflows: records
                .iter()
                .filter(|r| r.provider_context_overflow)
                .count(),
            truncations: records.iter().filter(|r| r.output_truncated).count(),
            average_latency_ms: (!latencies.is_empty())
                .then(|| latencies.iter().sum::<u64>() / latencies.len() as u64),
        }
    }
}
fn percentile(values: &[u32], percent: usize) -> Option<u32> {
    (!values.is_empty()).then(|| values[(values.len() * percent).div_ceil(100).saturating_sub(1)])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecommendationState {
    InsufficientHistory,
    KeepCurrent,
    Increase,
    PotentialDecrease,
    AtModelMaximum,
}
#[derive(Clone, Debug)]
pub struct OllamaRecommendation {
    pub state: RecommendationState,
    pub current: Option<u32>,
    pub recommended: Option<u32>,
    pub target: Option<u32>,
    pub reason: String,
}
pub fn recommend(
    records: &[OllamaRequestRecord],
    current: Option<u32>,
    model_max: Option<u32>,
) -> OllamaRecommendation {
    let statistics = OllamaContextStatistics::from_records(records);
    if statistics.count < MIN_SAMPLES {
        return OllamaRecommendation { state: RecommendationState::InsufficientHistory, current, recommended: current, target: None, reason: format!("At least {MIN_SAMPLES} recent requests are required before a stable recommendation.") };
    }
    let p95 = statistics.required_p95.unwrap_or_default();
    let target = p95.saturating_add(p95 / 5);
    let ceiling = model_max.unwrap_or(u32::MAX);
    let tier = TIERS
        .iter()
        .copied()
        .find(|tier| *tier >= target && *tier <= ceiling)
        .or_else(|| (target <= ceiling).then_some(ceiling));
    let recommended = tier.or(model_max);
    let pressure = statistics.hard_failures > 0
        || statistics.overflows > 0
        || current.is_some_and(|value| p95.saturating_mul(10) >= value.saturating_mul(8))
        || statistics.compactions.saturating_mul(10) >= statistics.count;
    let state = match (current, recommended) {
        (_, None) => RecommendationState::AtModelMaximum,
        (Some(now), Some(next)) if next > now && pressure => RecommendationState::Increase,
        (Some(now), Some(next))
            if next < now
                && statistics.hard_failures == 0
                && statistics.compactions.saturating_mul(20) <= statistics.count =>
        {
            RecommendationState::PotentialDecrease
        }
        (Some(_), Some(_)) => RecommendationState::KeepCurrent,
        (None, Some(_)) => RecommendationState::Increase,
    };
    let reason = format!("p95 ideal required context is {p95} tokens; target with 20% headroom is {target} tokens. {} compactions and {} hard failures were recorded.", statistics.compactions, statistics.hard_failures);
    OllamaRecommendation {
        state,
        current,
        recommended,
        target: Some(target),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn record(profile: &str, deep: bool, required: u32) -> OllamaRequestRecord {
        let mut r = OllamaRequestRecord::now(profile.into(), "model".into(), deep);
        r.ideal_required_context = required;
        r.final_required_context = required;
        r
    }
    #[test]
    fn persists_bounds_and_never_stores_source() {
        let dir = tempdir().unwrap();
        let t = OllamaRequestTracker::at(dir.path().join("history.json"));
        for i in 0..101 {
            let mut r = record("p", false, 4096);
            r.timestamp_ms = i;
            t.record(r).unwrap();
        }
        assert_eq!(t.records("p", false).len(), 100);
        assert!(!std::fs::read_to_string(t.path())
            .unwrap()
            .contains("SECRET_SOURCE_SENTINEL_94A1"));
    }
    #[test]
    fn percentiles_are_deterministic() {
        let records = (1..=20)
            .map(|n| record("p", false, n * 100))
            .collect::<Vec<_>>();
        let s = OllamaContextStatistics::from_records(&records);
        assert_eq!(s.required_p50, Some(1000));
        assert_eq!(s.required_p90, Some(1800));
        assert_eq!(s.required_p95, Some(1900));
    }
    #[test]
    fn recommendation_uses_ideal_requirement() {
        let records = (0..10)
            .map(|_| {
                let mut r = record("p", false, 12000);
                r.final_required_context = 6500;
                r.compacted = true;
                r
            })
            .collect::<Vec<_>>();
        let r = recommend(&records, Some(8192), Some(32768));
        assert_eq!(r.recommended, Some(16384));
        assert_eq!(r.state, RecommendationState::Increase);
    }
}
