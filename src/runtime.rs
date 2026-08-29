use crate::{
    cache::ExplanationCache,
    change_set::{ChangeSet, FileChange, SemanticUnitChange},
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    model::{ExplanationRegion, ExplanationRequest, UnitExplanation},
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
        regions: explanation_regions(
            unit,
            change.after.is_some().then(|| {
                file.patch
                    .as_ref()
                    .map(|patch| patch.ranges.as_slice())
                    .unwrap_or(&[])
            }),
        ),
        prior_explanation: deep
            .then_some(change.explanation.overview.clone())
            .filter(|value| !value.is_empty()),
        deep,
    }
}

pub(crate) fn explanation_regions(
    unit: &crate::change_set::SemanticUnit,
    changed_ranges: Option<&[crate::change_set::LineRange]>,
) -> Vec<ExplanationRegion> {
    let source_lines: Vec<_> = unit.source.lines().collect();
    let mut regions = changed_ranges
        .unwrap_or_default()
        .iter()
        .filter_map(|range| {
            let start = range.start.max(unit.location.start_line);
            let end = range.end.min(unit.location.end_line);
            (start <= end).then_some((start, end))
        })
        .enumerate()
        .map(|(index, (start, end))| {
            let relative_start = start - unit.location.start_line + 1;
            let relative_end = end - unit.location.start_line + 1;
            ExplanationRegion {
                id: index + 1,
                start_line: relative_start,
                end_line: relative_end,
                source: source_lines
                    .get(relative_start.saturating_sub(1)..relative_end.min(source_lines.len()))
                    .unwrap_or(&[])
                    .join("\n"),
            }
        })
        .collect::<Vec<_>>();
    if regions.is_empty() {
        regions.push(ExplanationRegion {
            id: 1,
            start_line: 1,
            end_line: source_lines.len().max(1),
            source: unit.source.clone(),
        });
    }
    regions
}
pub fn apply(change: &mut SemanticUnitChange, result: UnitExplanation, deep: bool) {
    if deep {
        change.deep_explanation = result
            .deep
            .or((!result.overview.is_empty()).then_some(result.overview));
    } else {
        change.explanation = result;
        change.deep_explanation = None;
    }
}
pub fn result(result: UnitExplanation, cache_hit: bool, deep: bool) -> serde_json::Value {
    serde_json::json!({"ok":true,"cache_hit":cache_hit,"overview":result.overview,"annotations":result.annotations,"deep":result.deep,"mode":if deep {"deep"} else {"normal"}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        change_set::{
            SemanticChangeId, SemanticIdentity, SemanticUnit, SemanticUnitChangeKind,
            SourceLocation,
        },
        language::SourceUnitKind,
    };

    #[test]
    fn replacing_normal_explanation_invalidates_deep_result() {
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "inspect".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 1,
                end_line: 1,
            },
            source: "fn inspect() {}".into(),
            language: "Rust".into(),
        };
        let mut change = SemanticUnitChange {
            id: SemanticChangeId("id".into()),
            kind: SemanticUnitChangeKind::Added,
            before: None,
            after: Some(unit),
            diff: String::new(),
            explanation: UnitExplanation {
                overview: "Old overview".into(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: Some("Old detail".into()),
        };

        apply(
            &mut change,
            UnitExplanation {
                overview: "New overview".into(),
                annotations: vec![],
                deep: None,
            },
            false,
        );
        assert_eq!(change.explanation.overview, "New overview");
        assert!(change.deep_explanation.is_none());
    }

    #[test]
    fn changed_file_lines_become_unit_relative_annotation_regions() {
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "inspect".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 10,
                end_line: 12,
            },
            source: "fn inspect() {\n    changed();\n}".into(),
            language: "Rust".into(),
        };

        let regions = explanation_regions(
            &unit,
            Some(&[crate::change_set::LineRange { start: 11, end: 11 }]),
        );

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 2);
        assert_eq!(regions[0].end_line, 2);
        assert_eq!(regions[0].source, "    changed();");
    }

    #[test]
    fn units_without_changed_line_ranges_remain_annotatable() {
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "removed".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 20,
                end_line: 21,
            },
            source: "fn removed() {\n}".into(),
            language: "Rust".into(),
        };

        let regions = explanation_regions(&unit, None);

        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].start_line, regions[0].end_line), (1, 2));
    }
}
