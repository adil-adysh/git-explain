use crate::{
    cache::ExplanationCache,
    change_set::{ChangeSet, FileChange, SemanticUnitChange},
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    model::{ExplanationKind, ExplanationRegion, ExplanationRequest, UnitExplanation},
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
            for kind in [
                ExplanationKind::Overview,
                ExplanationKind::Changes,
                ExplanationKind::Deep,
            ] {
                let request = request_for_kind(&metadata, change, &context, kind);
                let key = ExplanationCache::change_key(&request, model, reader, explanation);
                if let Ok(Some(json)) = cache.get(&key) {
                    if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                        apply_kind(change, result, kind);
                    }
                }
            }
            let request = concept_request_for(&metadata, change, &context);
            let key = ExplanationCache::change_key(&request, model, reader, explanation);
            if let Ok(Some(json)) = cache.get(&key) {
                if let Ok(result) = serde_json::from_str::<UnitExplanation>(&json) {
                    change.concept_explanation = result;
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
    request_for_kind(
        file,
        change,
        context,
        if deep {
            ExplanationKind::Deep
        } else {
            ExplanationKind::Changes
        },
    )
}

pub fn request_for_kind(
    file: &FileChange,
    change: &SemanticUnitChange,
    context: &crate::explain::AnalysisContext,
    kind: ExplanationKind,
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
        prerequisite_id: None,
        policy: Default::default(),
        source_unit: unit.source.clone(),
        unit_name: unit.identity.qualified_name.clone(),
        unit_kind: format!("{:?}", unit.identity.kind),
        // A file-wide patch can contain unrelated additions/deletions. A section
        // with only one revision has no replacement relationship to those units.
        diff: match (change.before.as_ref(), change.after.as_ref()) {
            (None, Some(after)) => after
                .source
                .lines()
                .map(|line| format!("+{line}\n"))
                .collect(),
            (Some(before), None) => before
                .source
                .lines()
                .map(|line| format!("-{line}\n"))
                .collect(),
            _ => change.diff.clone(),
        },
        language: unit.language.clone(),
        git_context,
        regions: if matches!(kind, ExplanationKind::Changes | ExplanationKind::Concepts) {
            annotation_regions(
                unit,
                change.after.as_ref().map(|_| {
                    file.patch
                        .as_ref()
                        .map(|patch| patch.ranges.as_slice())
                        .unwrap_or(&[])
                }),
            )
        } else {
            explanation_regions(
                unit,
                (kind != ExplanationKind::Overview && change.after.is_some()).then(|| {
                    file.patch
                        .as_ref()
                        .map(|patch| patch.ranges.as_slice())
                        .unwrap_or(&[])
                }),
            )
        },
        prior_explanation: (kind == ExplanationKind::Deep)
            .then_some(change.explanation.overview.clone())
            .filter(|value| !value.is_empty()),
        kind,
        deep: kind == ExplanationKind::Deep,
        evidence: crate::language::facts::extract_evidence(&unit.language, &unit.source),
    }
}

/// Build the source-oriented request used by the Concepts action. In diff
/// mode, concepts use the same deterministic changed regions as the change
/// explanation, with the complete active unit still available as context.
pub fn concept_request_for(
    file: &FileChange,
    change: &SemanticUnitChange,
    context: &crate::explain::AnalysisContext,
) -> ExplanationRequest {
    let mut request = request_for_kind(file, change, context, ExplanationKind::Concepts);
    request
        .git_context
        .push_str("\nExplanation focus: concepts");
    request
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
                coverage_id: index + 1,
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
            coverage_id: 1,
            start_line: 1,
            end_line: source_lines.len().max(1),
            source: unit.source.clone(),
        });
    }
    regions
}

fn parser_for_language(language: &str) -> Option<tree_sitter::Parser> {
    let mut parser = tree_sitter::Parser::new();
    let lang = crate::language::grammar::grammar_for_language(language)?;
    parser.set_language(&lang).ok()?;
    Some(parser)
}

fn collect_statement_starts(node: tree_sitter::Node, out: &mut Vec<usize>, infra: bool) {
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    let is_container = matches!(
        parent_kind,
        "block"
            | "compound_statement"
            | "statement_block"
            | "statement_list"
            | "declaration_list"
            | "class_body"
            | "field_declaration_list"
    ) || parent_kind.ends_with("_block")
        || parent_kind.ends_with("_body")
        || (infra
            && (matches!(
                parent_kind,
                "body" | "block_mapping" | "object" | "statements" | "source_file" | "program"
            ) || parent_kind.ends_with("_list")));

    if is_container && node.is_named() && !node.kind().contains("comment") {
        let start_line = node.start_position().row + 1;
        out.push(start_line);
    }

    // Keep multiline expressions intact, including closures nested inside them,
    // block scalars, sequences, heredocs, and here-strings.
    if matches!(
        node.kind(),
        "array_expression"
            | "call_expression"
            | "arguments"
            | "closure_expression"
            | "arrow_function"
            | "lambda"
            | "lambda_expression"
            | "array"
            | "list"
            | "tuple"
            | "object"
            | "argument_list"
            | "initializer_list"
            | "block_scalar"
            | "block_sequence"
            | "flow_sequence"
            | "heredoc_body"
            | "heredoc_redirect"
            | "here_string"
            | "here_string_literal"
            | "string_literal"
            | "interpolated_string_expression"
            | "template_string"
            | "string_lit"
    ) || node.kind().contains("string")
        || node.kind().contains("heredoc")
    {
        return;
    }
    for child in node.children(&mut node.walk()) {
        collect_statement_starts(child, out, infra);
    }
}

pub(crate) fn annotation_regions(
    unit: &crate::change_set::SemanticUnit,
    changed_ranges: Option<&[crate::change_set::LineRange]>,
) -> Vec<ExplanationRegion> {
    let mut coarse_regions = explanation_regions(unit, changed_ranges);
    coarse_regions.sort_by_key(|region| (region.start_line, region.end_line));
    let mut normalized: Vec<ExplanationRegion> = Vec::new();
    for region in coarse_regions {
        if let Some(previous) = normalized.last_mut() {
            if region.start_line <= previous.end_line {
                previous.end_line = previous.end_line.max(region.end_line);
                continue;
            }
        }
        normalized.push(region);
    }
    let lines: Vec<_> = unit.source.lines().collect();
    for (index, region) in normalized.iter_mut().enumerate() {
        region.id = index + 1;
        region.coverage_id = index + 1;
        region.source = lines
            .get(region.start_line.saturating_sub(1)..region.end_line.min(lines.len()))
            .unwrap_or(&[])
            .join("\n");
    }
    let coarse_regions = normalized;
    let Some(mut parser) = parser_for_language(&unit.language) else {
        return coarse_regions;
    };
    let mut tree = if unit.language.eq_ignore_ascii_case("jsonc") {
        crate::language::infra::prepare_json_source(&unit.source, true)
            .and_then(|prep| parser.parse(&prep, None))
    } else {
        parser.parse(&unit.source, None)
    };
    if matches!(unit.language.to_lowercase().as_str(), "typescript" | "ts")
        && tree.as_ref().is_some_and(|t| t.root_node().has_error())
        && parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .is_ok()
    {
        if let Some(jsx_tree) = parser.parse(&unit.source, None) {
            if !jsx_tree.root_node().has_error() {
                tree = Some(jsx_tree);
            }
        }
    }
    let Some(tree) = tree else {
        return coarse_regions;
    };

    if tree.root_node().has_error() {
        return coarse_regions;
    }
    let mut statement_starts = Vec::new();
    collect_statement_starts(
        tree.root_node(),
        &mut statement_starts,
        crate::language::infra_facts::is_infra_or_kotlin_language(&unit.language),
    );
    statement_starts.sort_unstable();
    statement_starts.dedup();

    let source_lines: Vec<_> = unit.source.lines().collect();
    let mut fine_regions = Vec::new();
    let mut next_id = 1;

    for coarse in coarse_regions {
        let in_range: Vec<usize> = statement_starts
            .iter()
            .copied()
            .filter(|&l| l >= coarse.start_line && l <= coarse.end_line)
            .collect();

        if in_range.is_empty() {
            fine_regions.push(ExplanationRegion {
                id: next_id,
                coverage_id: coarse.coverage_id,
                start_line: coarse.start_line,
                end_line: coarse.end_line,
                source: coarse.source,
            });
            next_id += 1;
            continue;
        }

        let first_start = coarse.start_line;
        let mut cut_starts = vec![first_start];
        for &s in &in_range[1..] {
            if s > first_start && s <= coarse.end_line {
                cut_starts.push(s);
            }
        }
        cut_starts.dedup();

        let mut slices = Vec::new();
        for i in 0..cut_starts.len() {
            let start = cut_starts[i];
            let end = if i + 1 < cut_starts.len() {
                cut_starts[i + 1] - 1
            } else {
                coarse.end_line
            };
            if start <= end {
                slices.push((start, end));
            }
        }

        while slices.len() > 16 {
            let mut min_idx = 0;
            let mut min_span = usize::MAX;
            for i in 0..slices.len() - 1 {
                let span = slices[i + 1].1 - slices[i].0;
                if span < min_span {
                    min_span = span;
                    min_idx = i;
                }
            }
            let merged = (slices[min_idx].0, slices[min_idx + 1].1);
            slices.remove(min_idx + 1);
            slices[min_idx] = merged;
        }

        for (start, end) in slices {
            fine_regions.push(ExplanationRegion {
                id: next_id,
                coverage_id: coarse.coverage_id,
                start_line: start,
                end_line: end,
                source: source_lines
                    .get(start.saturating_sub(1)..end.min(source_lines.len()))
                    .unwrap_or(&[])
                    .join("\n"),
            });
            next_id += 1;
        }
    }

    if fine_regions.is_empty() {
        explanation_regions(unit, changed_ranges)
    } else {
        fine_regions
    }
}
#[cfg(test)]
pub fn apply(change: &mut SemanticUnitChange, result: UnitExplanation, deep: bool) {
    apply_kind(
        change,
        result,
        if deep {
            ExplanationKind::Deep
        } else {
            ExplanationKind::Changes
        },
    );
}
pub fn apply_kind(change: &mut SemanticUnitChange, result: UnitExplanation, kind: ExplanationKind) {
    if kind == ExplanationKind::Deep {
        change.deep_explanation = result
            .deep
            .or((!result.overview.is_empty()).then_some(result.overview));
    } else if kind == ExplanationKind::Concepts {
        change.concept_explanation = result;
    } else if kind == ExplanationKind::Overview {
        change.overview_explanation = result;
    } else {
        change.explanation = result;
        change.deep_explanation = None;
    }
}
pub fn result(
    result: UnitExplanation,
    cache_hit: bool,
    kind: ExplanationKind,
) -> serde_json::Value {
    serde_json::json!({"ok":true,"cache_hit":cache_hit,"overview":result.overview,"annotations":result.annotations,"deep":result.deep,"mode":format!("{kind:?}").to_lowercase()})
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

    fn annotation_test_unit(source: &str) -> SemanticUnit {
        SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "fixture".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "fixture.rs".into(),
                start_line: 1,
                end_line: source.lines().count(),
            },
            source: source.into(),
            language: "Rust".into(),
        }
    }

    #[test]
    fn annotation_regions_fall_back_for_syntax_errors() {
        let unit = annotation_test_unit("fn incomplete( {\n    first();\n    second();\n}");
        let regions = annotation_regions(&unit, None);
        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].start_line, regions[0].end_line), (1, 4));
        assert_eq!(regions[0].source, unit.source);
    }

    #[test]
    fn annotation_regions_normalize_overlaps_and_input_order() {
        let mut unit = annotation_test_unit(
            "fn stable() {\n    first();\n    second();\n    third();\n    fourth();\n}",
        );
        unit.location.start_line = 10;
        unit.location.end_line = 15;
        let ranges = [
            crate::change_set::LineRange { start: 10, end: 13 },
            crate::change_set::LineRange { start: 12, end: 15 },
        ];
        let regions = annotation_regions(&unit, Some(&ranges));
        let reversed = annotation_regions(&unit, Some(&[ranges[1].clone(), ranges[0].clone()]));
        assert_eq!(regions, reversed);
        assert_eq!(
            regions
                .iter()
                .map(|r| (r.id, r.coverage_id, r.start_line, r.end_line))
                .collect::<Vec<_>>(),
            vec![(1, 1, 1, 2), (2, 1, 3, 3), (3, 1, 4, 4), (4, 1, 5, 6)]
        );
        assert_eq!(
            regions
                .iter()
                .map(|r| r.source.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            unit.source
        );
    }

    #[test]
    fn annotation_regions_preserve_multiline_expressions_with_nested_blocks() {
        for source in [
            "fn nested_array() {\n    let callbacks = [\n        || {\n            first();\n            second();\n        },\n    ];\n    consume(callbacks);\n}",
            "fn nested_call() {\n    consume(\n        || {\n            first();\n            second();\n        },\n    );\n    finish();\n}",
        ] {
            let regions = annotation_regions(&annotation_test_unit(source), None);
            assert_eq!(regions.iter().map(|r| (r.start_line, r.end_line)).collect::<Vec<_>>(),
                vec![(1, 7), (8, 9)], "{source}");
        }
    }

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
            overview_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            explanation: UnitExplanation {
                teaching: None,
                overview: "Old overview".into(),
                annotations: vec![],
                deep: None,
            },
            concept_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: Some("Old detail".into()),
        };

        apply(
            &mut change,
            UnitExplanation {
                teaching: None,
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

    #[test]
    fn whole_added_function_has_distinct_statement_bounded_targets() {
        let source = "fn test_synthetic() {\n    for message in [\n        \"first\",\n        \"second\",\n        \"third\",\n    ] {\n        let error = construct_error(message);\n        assert_eq!(error.code, \"failure\");\n        assert!(!error.retryable);\n        assert!(error.message.contains(\"detail\"));\n    }\n}";
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "test_synthetic".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 1,
                end_line: 12,
            },
            source: source.into(),
            language: "Rust".into(),
        };
        let change = SemanticUnitChange {
            id: SemanticChangeId("id".into()),
            kind: SemanticUnitChangeKind::Added,
            before: None,
            after: Some(unit.clone()),
            diff: format!("-fn unrelated_deleted() {{}}\n+{source}"),
            overview_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            concept_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: None,
        };
        let file = FileChange {
            old_path: None,
            new_path: Some(std::path::PathBuf::from("lib.rs")),
            kind: crate::change_set::FileChangeKind::Added,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: Some(crate::change_set::FilePatch {
                ranges: vec![crate::change_set::LineRange { start: 1, end: 12 }],
                text: source.into(),
            }),
            before: None,
            after: None,
            semantic_changes: vec![],
        };
        let context = crate::explain::AnalysisContext::working_tree();
        let request = request_for_kind(&file, &change, &context, ExplanationKind::Changes);
        assert!(!request.diff.contains("unrelated_deleted"));
        assert!(request.diff.contains("+fn test_synthetic"));
        let bounds: Vec<_> = request
            .regions
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect();
        assert_eq!(bounds, vec![(1, 6), (7, 7), (8, 8), (9, 9), (10, 12)]);
    }

    #[test]
    fn partial_hunk_never_includes_unchanged_lines() {
        let source = "fn test_synthetic() {\n    for message in [\n        \"first\",\n        \"second\",\n        \"third\",\n    ] {\n        let error = construct_error(message);\n        assert_eq!(error.code, \"failure\");\n        assert!(!error.retryable);\n        assert!(error.message.contains(\"detail\"));\n    }\n}";
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "test_synthetic".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 1,
                end_line: 12,
            },
            source: source.into(),
            language: "Rust".into(),
        };
        let change = SemanticUnitChange {
            id: SemanticChangeId("id".into()),
            kind: SemanticUnitChangeKind::Modified,
            before: Some(unit.clone()),
            after: Some(unit.clone()),
            diff: String::new(),
            overview_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            concept_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: None,
        };
        let file = FileChange {
            old_path: Some(std::path::PathBuf::from("lib.rs")),
            new_path: Some(std::path::PathBuf::from("lib.rs")),
            kind: crate::change_set::FileChangeKind::Modified,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: Some(crate::change_set::FilePatch {
                ranges: vec![crate::change_set::LineRange { start: 7, end: 9 }],
                text: String::new(),
            }),
            before: None,
            after: None,
            semantic_changes: vec![],
        };
        let context = crate::explain::AnalysisContext::working_tree();
        let request = request_for_kind(&file, &change, &context, ExplanationKind::Changes);
        for region in &request.regions {
            assert!(
                region.start_line >= 7,
                "start_line {} < 7",
                region.start_line
            );
            assert!(region.end_line <= 9, "end_line {} > 9", region.end_line);
        }
    }

    #[test]
    fn file_concepts_full_source_has_fine_targets() {
        let source = "fn test_synthetic() {\n    for message in [\n        \"first\",\n        \"second\",\n        \"third\",\n    ] {\n        let error = construct_error(message);\n        assert_eq!(error.code, \"failure\");\n        assert!(!error.retryable);\n        assert!(error.message.contains(\"detail\"));\n    }\n}";
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "test_synthetic".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 1,
                end_line: 12,
            },
            source: source.into(),
            language: "Rust".into(),
        };
        let request = crate::file_analysis::concept_request_for(&unit, None);
        assert_eq!(request.kind, ExplanationKind::Concepts);
        let bounds: Vec<_> = request
            .regions
            .iter()
            .map(|r| (r.start_line, r.end_line))
            .collect();
        assert_eq!(bounds, vec![(1, 6), (7, 7), (8, 8), (9, 9), (10, 12)]);
        for region in &request.regions {
            assert_eq!(region.coverage_id, 1);
        }
    }

    #[test]
    fn deleted_unit_uses_fine_source_targets() {
        let source = "fn deleted_func() {\n    first();\n    second();\n}";
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "deleted_func".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 10,
                end_line: 13,
            },
            source: source.into(),
            language: "Rust".into(),
        };
        let change = SemanticUnitChange {
            id: SemanticChangeId("id".into()),
            kind: SemanticUnitChangeKind::Deleted,
            before: Some(unit.clone()),
            after: None,
            diff: format!("-{source}\n+fn unrelated_added() {{}}"),
            overview_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            concept_explanation: UnitExplanation {
                teaching: None,
                overview: String::new(),
                annotations: vec![],
                deep: None,
            },
            deep_explanation: None,
        };
        let file = FileChange {
            old_path: Some(std::path::PathBuf::from("lib.rs")),
            new_path: None,
            kind: crate::change_set::FileChangeKind::Deleted,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: None,
            before: None,
            after: None,
            semantic_changes: vec![],
        };
        let context = crate::explain::AnalysisContext::working_tree();
        let request = request_for_kind(&file, &change, &context, ExplanationKind::Changes);
        assert!(!request.diff.contains("unrelated_added"));
        assert!(request.diff.contains("-fn deleted_func"));
        assert_eq!(request.source_unit, source);
        assert_eq!(
            request
                .regions
                .iter()
                .map(|r| (r.start_line, r.end_line))
                .collect::<Vec<_>>(),
            vec![(1, 2), (3, 4)]
        );
    }

    #[test]
    fn unsupported_language_falls_back_to_coarse_regions() {
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "test".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "test.xyz".into(),
                start_line: 1,
                end_line: 3,
            },
            source: "unsupported syntax\nline 2\nline 3".into(),
            language: "UnknownLanguage".into(),
        };
        let regions = annotation_regions(
            &unit,
            Some(&[crate::change_set::LineRange { start: 1, end: 3 }]),
        );
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_line, 1);
        assert_eq!(regions[0].end_line, 3);
    }

    #[test]
    fn subdivision_cap_limits_to_sixteen_targets() {
        let mut source = String::from("fn many_statements() {\n");
        for i in 0..30 {
            source.push_str(&format!("    let var_{i} = {i};\n"));
        }
        source.push_str("}\n");
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "many_statements".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "lib.rs".into(),
                start_line: 1,
                end_line: 32,
            },
            source,
            language: "Rust".into(),
        };
        let regions = annotation_regions(
            &unit,
            Some(&[crate::change_set::LineRange { start: 1, end: 32 }]),
        );
        assert!(regions.len() <= 16, "regions.len() was {}", regions.len());
        assert!(regions.len() > 1);
        assert_eq!(regions.first().unwrap().start_line, 1);
        assert_eq!(regions.last().unwrap().end_line, 32);
        for i in 0..regions.len() - 1 {
            assert_eq!(regions[i].end_line + 1, regions[i + 1].start_line);
            assert_eq!(regions[i + 1].id, regions[i].id + 1);
        }
    }

    #[test]
    fn deleted_unit_request_populates_evidence_from_before_source() {
        let before_source =
            "fn removed(limit: i32) {\n    if limit >= 0 {\n        cleanup();\n    }\n}";
        let before_unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "removed".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "test.rs".into(),
                start_line: 1,
                end_line: 5,
            },
            source: before_source.into(),
            language: "Rust".into(),
        };
        let change = SemanticUnitChange {
            id: SemanticChangeId("test".into()),
            kind: SemanticUnitChangeKind::Deleted,
            overview_explanation: UnitExplanation::default(),
            explanation: UnitExplanation::default(),
            concept_explanation: UnitExplanation::default(),
            deep_explanation: None,
            before: Some(before_unit),
            after: None,
            diff: "-fn removed()".into(),
        };
        let file = FileChange {
            old_path: Some(std::path::PathBuf::from("test.rs")),
            new_path: None,
            kind: crate::change_set::FileChangeKind::Deleted,
            similarity: None,
            old_mode: None,
            new_mode: None,
            binary: false,
            patch: None,
            before: None,
            after: None,
            semantic_changes: vec![],
        };
        let context = crate::explain::AnalysisContext::working_tree();
        let request = request_for_kind(&file, &change, &context, ExplanationKind::Changes);
        assert!(
            !request.evidence.is_empty(),
            "deleted unit must populate evidence from before source"
        );
        let has_cmp = request.evidence.facts.iter().any(|f| matches!(f, crate::language::facts::SourceFact::Comparison { operator, .. } if operator == ">="));
        assert!(
            has_cmp,
            "deleted unit evidence should contain >= comparison"
        );
    }

    #[test]
    fn file_analysis_request_populates_evidence_from_unit_source() {
        let source = "fn inspect(val: usize) -> bool {\n    if val >= 100 {\n        return true;\n    }\n    false\n}";
        let unit = SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "inspect".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "inspect.rs".into(),
                start_line: 1,
                end_line: 6,
            },
            source: source.into(),
            language: "Rust".into(),
        };
        let request = crate::file_analysis::request_for(&unit, Some("HEAD"), false, None);
        assert!(
            !request.evidence.is_empty(),
            "file analysis request must populate evidence"
        );
        let has_cmp = request.evidence.facts.iter().any(|f| matches!(f, crate::language::facts::SourceFact::Comparison { operator, .. } if operator == ">="));
        assert!(
            has_cmp,
            "file analysis request evidence must contain >= comparison"
        );
    }

    fn infra_test_unit(source: &str, language: &str) -> SemanticUnit {
        SemanticUnit {
            identity: SemanticIdentity {
                kind: SourceUnitKind::Function,
                qualified_name: "fixture".into(),
                occurrence: 1,
            },
            location: SourceLocation {
                path: "fixture".into(),
                start_line: 1,
                end_line: source.lines().count(),
            },
            source: source.into(),
            language: language.into(),
        }
    }

    #[test]
    fn infra_annotation_multiple_body_and_attribute_regions() {
        let hcl_source = "resource \"aws_instance\" \"web\" {\n  ami = \"ami-12345\"\n  instance_type = \"t3.micro\"\n}";
        let hcl_unit = infra_test_unit(hcl_source, "HCL");
        let hcl_regions = annotation_regions(&hcl_unit, None);
        assert!(
            hcl_regions.len() >= 2,
            "HCL body with multiple attributes should partition into distinct regions: {:?}",
            hcl_regions
        );

        let bash_source = "deploy() {\n  step_one\n  step_two\n}";
        let bash_unit = infra_test_unit(bash_source, "Bash");
        let bash_regions = annotation_regions(&bash_unit, None);
        assert!(
            bash_regions.len() >= 2,
            "Bash function body should partition into distinct regions: {:?}",
            bash_regions
        );

        let kotlin_source = "fun calculate(x: Int): Int {\n  val a = x * 2\n  return a + 10\n}";
        let kotlin_unit = infra_test_unit(kotlin_source, "Kotlin");
        let kotlin_regions = annotation_regions(&kotlin_unit, None);
        assert!(
            kotlin_regions.len() >= 2,
            "Kotlin function body should partition into distinct regions: {:?}",
            kotlin_regions
        );
    }

    #[test]
    fn infra_annotation_complete_literal_and_heredoc_coverage() {
        let yaml_source =
            "config:\n  template: |\n    line 1\n    line 2\n    line 3\n  timeout: 30";
        let yaml_unit = infra_test_unit(yaml_source, "YAML");
        let yaml_regions = annotation_regions(&yaml_unit, None);
        // The block scalar lines (lines 2-5) must not be fragmented into multiple regions
        let scalar_region = yaml_regions
            .iter()
            .find(|r| r.start_line <= 2 && r.end_line >= 5);
        assert!(
            scalar_region.is_some(),
            "YAML block scalar must remain intact without splitting: {:?}",
            yaml_regions
        );

        let bash_source =
            "generate() {\n  cat << 'EOF'\nline 1\nline 2\nline 3\nEOF\n  echo done\n}";
        let bash_unit = infra_test_unit(bash_source, "Bash");
        let bash_regions = annotation_regions(&bash_unit, None);
        // Heredoc lines (lines 2-6) must not be split internally
        let heredoc_region = bash_regions
            .iter()
            .find(|r| r.start_line <= 2 && r.end_line >= 6);
        assert!(
            heredoc_region.is_some(),
            "Bash heredoc must remain intact without splitting: {:?}",
            bash_regions
        );
    }

    #[test]
    fn infra_annotation_changed_only_coverage_and_full_context() {
        let source = "resource \"aws_instance\" \"web\" {\n  ami = \"ami-12345\"\n  instance_type = \"t3.micro\"\n}";
        let unit = infra_test_unit(source, "HCL");
        let ranges = [crate::change_set::LineRange { start: 2, end: 2 }];
        let regions = annotation_regions(&unit, Some(&ranges));
        assert_eq!(
            regions.len(),
            1,
            "Changed-only region must cover only changed lines: {:?}",
            regions
        );
        assert_eq!(regions[0].start_line, 2);
        assert_eq!(regions[0].end_line, 2);
        assert!(regions[0].source.contains("ami = \"ami-12345\""));
        assert!(
            unit.source.contains("resource \"aws_instance\""),
            "Unit retains full source context"
        );
    }
}
