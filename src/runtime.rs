use crate::{
    cache::ExplanationCache,
    change_set::{ChangeSet, FileChange, SemanticUnitChange},
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    model::{ExplanationRequest, UnitExplanation},
};

pub fn hydrate(
    change_set: &mut ChangeSet,
    cache: &ExplanationCache,
    model: &ResolvedProfile,
    reader: &ReaderConfig,
    explanation: &ExplanationConfig,
) {
    let context = change_set.context.clone();
    for file in &mut change_set.files {
        let metadata = file.clone();
        for change in &mut file.semantic_changes {
            if !change.kind.explainable() {
                continue;
            }
            for deep in [false, true] {
                let request = request_for(&metadata, change, &context, deep);
                let key = ExplanationCache::change_key(&request, model, reader, explanation);
                if let Ok(Some(json)) = cache.get(&key) {
                    if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                        apply(change, result, deep);
                    }
                }
            }
        }
    }
}
pub fn request_for(
    file: &FileChange,
    change: &SemanticUnitChange,
    context: &crate::explain::AnalysisContext,
    deep: bool,
) -> ExplanationRequest {
    let unit = change.active_unit();
    let mut git_context = context.prompt_context();
    git_context.push_str(&format!("\nFile status: {}", file.status_label()));
    if let Some(path) = &file.old_path {
        git_context.push_str(&format!("\nOld path: {}", path.display()));
    }
    if let Some(path) = &file.new_path {
        git_context.push_str(&format!("\nNew path: {}", path.display()));
    }
    git_context.push_str(&format!("\nUnit status: {:?}", change.kind));
    ExplanationRequest {
        source_unit: unit.source.clone(),
        unit_name: unit.identity.qualified_name.clone(),
        unit_kind: format!("{:?}", unit.identity.kind),
        diff: change.diff.clone(),
        language: unit.language.clone(),
        git_context,
        regions: vec![],
        prior_explanation: deep
            .then_some(change.explanation.overview.clone())
            .filter(|value| !value.is_empty()),
        deep,
    }
}
pub fn apply(change: &mut SemanticUnitChange, result: UnitExplanation, deep: bool) {
    if deep {
        change.deep_explanation = result
            .deep
            .or((!result.overview.is_empty()).then_some(result.overview));
    } else {
        change.explanation = result;
    }
}
pub fn result(result: UnitExplanation, cache_hit: bool, deep: bool) -> serde_json::Value {
    serde_json::json!({"ok":true,"cache_hit":cache_hit,"overview":result.overview,"annotations":result.annotations,"deep":result.deep,"mode":if deep {"deep"} else {"normal"}})
}
