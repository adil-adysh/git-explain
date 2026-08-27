use crate::{
    cache::ExplanationCache,
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    explain::{AnalysisContext, ExplainedUnit},
    model::{ExplanationRequest, UnitExplanation},
    snapshot::{AnalysisSnapshot, UnitId},
};
use std::collections::HashMap;

pub fn hydrate(
    items: &mut HashMap<UnitId, ExplainedUnit>,
    cache: &ExplanationCache,
    snapshot: &AnalysisSnapshot,
    model: &ResolvedProfile,
    reader: &ReaderConfig,
    explanation: &ExplanationConfig,
) {
    for item in items.values_mut() {
        for deep in [false, true] {
            let request = request_for(item, &snapshot.context, deep);
            let key = ExplanationCache::key(&request, model, reader, explanation);
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    apply(item, result, deep);
                }
            }
        }
    }
}

pub fn request_for(
    item: &ExplainedUnit,
    context: &AnalysisContext,
    deep: bool,
) -> ExplanationRequest {
    ExplanationRequest {
        source_unit: item.unit.source.clone(),
        unit_name: item
            .unit
            .qualified_name
            .clone()
            .unwrap_or_else(|| item.unit.name.clone()),
        unit_kind: format!("{:?}", item.unit.kind),
        diff: item.diff.clone(),
        language: item.language.clone(),
        git_context: context.prompt_context(),
        regions: item.regions.clone(),
        prior_explanation: deep
            .then_some(item.explanation.overview.clone())
            .filter(|s| !s.is_empty()),
        deep,
    }
}

pub fn apply(item: &mut ExplainedUnit, result: UnitExplanation, deep: bool) {
    if deep {
        item.deep_explanation = result
            .deep
            .or((!result.overview.is_empty()).then_some(result.overview));
    } else {
        item.explanation = result;
    }
}

pub fn result(result: UnitExplanation, cache_hit: bool, deep: bool) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "cache_hit": cache_hit,
        "overview": result.overview,
        "annotations": result.annotations,
        "deep": result.deep,
        "mode": if deep { "deep" } else { "normal" }
    })
}
