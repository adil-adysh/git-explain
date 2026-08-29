use crate::{
    change_set::{
        SemanticChangeId, SemanticIdentity, SemanticUnit, SemanticUnitChange,
        SemanticUnitChangeKind, SourceFile, SourceLocation,
    },
    language::LanguageRegistry,
    model::UnitExplanation,
};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// A complete, revision-specific semantic view of one source file.
///
/// File mode consumes this document directly. Diff mode compares two
/// documents through [`match_documents`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticDocument {
    pub source: SourceFile,
    pub language: Option<String>,
    pub units: Vec<SemanticUnit>,
}

/// Extracts a reusable semantic document without assigning change meaning.
pub fn extract_document(file: &SourceFile) -> Result<SemanticDocument> {
    let line_count = file.content.lines().count().max(1);
    let units = LanguageRegistry::find_changed_units(
        &file.path,
        &file.content,
        &[crate::change_set::LineRange {
            start: 1,
            end: line_count,
        }],
    )?;
    let language = LanguageRegistry::language_for_path(&file.path).map(str::to_string);
    let mut occurrences = HashMap::<(crate::language::SourceUnitKind, String), usize>::new();
    let units = units
        .into_iter()
        .map(|unit| {
            let name = unit.qualified_name.clone().unwrap_or(unit.name);
            let occurrence = occurrences
                .entry((unit.kind.clone(), name.clone()))
                .or_default();
            *occurrence += 1;
            SemanticUnit {
                identity: SemanticIdentity {
                    kind: unit.kind,
                    qualified_name: name,
                    occurrence: *occurrence,
                },
                location: SourceLocation {
                    path: file.path.clone(),
                    start_line: unit.start_line,
                    end_line: unit.end_line,
                },
                source: unit.source,
                language: language.clone().unwrap_or_else(|| "unknown".into()),
            }
        })
        .collect();
    Ok(SemanticDocument {
        source: file.clone(),
        language,
        units,
    })
}

pub fn match_documents(
    before: Option<&SemanticDocument>,
    after: Option<&SemanticDocument>,
    diff: String,
) -> Vec<SemanticUnitChange> {
    match_units(
        before
            .map(|document| document.units.clone())
            .unwrap_or_default(),
        after
            .map(|document| document.units.clone())
            .unwrap_or_default(),
        diff,
    )
}

/// Conservative deterministic matcher. Identity is kind + qualified name +
/// occurrence; uncertain duplicate matches deliberately become delete + add.
fn match_units(
    before: Vec<SemanticUnit>,
    after: Vec<SemanticUnit>,
    diff: String,
) -> Vec<SemanticUnitChange> {
    let mut old = BTreeMap::new();
    for unit in before {
        old.insert(unit.identity.clone(), unit);
    }
    let mut new = BTreeMap::new();
    for unit in after {
        new.insert(unit.identity.clone(), unit);
    }
    let mut keys: Vec<_> = old.keys().chain(new.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|identity| {
            let before = old.remove(&identity);
            let after = new.remove(&identity);
            let kind = match (&before, &after) {
                (None, Some(_)) => SemanticUnitChangeKind::Added,
                (Some(_), None) => SemanticUnitChangeKind::Deleted,
                (Some(old), Some(new)) if old.source == new.source => {
                    SemanticUnitChangeKind::Preserved
                }
                (Some(_), Some(_)) => SemanticUnitChangeKind::Modified,
                (None, None) => unreachable!(),
            };
            let id = change_id(&identity, kind, before.as_ref(), after.as_ref());
            SemanticUnitChange {
                id,
                kind,
                before,
                after,
                diff: diff.clone(),
                overview_explanation: UnitExplanation {
                    overview: String::new(),
                    annotations: vec![],
                    deep: None,
                },
                explanation: UnitExplanation {
                    overview: String::new(),
                    annotations: vec![],
                    deep: None,
                },
                concept_explanation: UnitExplanation {
                    overview: String::new(),
                    annotations: vec![],
                    deep: None,
                },
                deep_explanation: None,
            }
        })
        .collect()
}

fn change_id(
    identity: &SemanticIdentity,
    kind: SemanticUnitChangeKind,
    before: Option<&SemanticUnit>,
    after: Option<&SemanticUnit>,
) -> SemanticChangeId {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{:?}:{}:{}:{kind:?}",
        identity.kind, identity.qualified_name, identity.occurrence
    ));
    if let Some(unit) = before {
        hasher.update(&unit.source);
    }
    if let Some(unit) = after {
        hasher.update(&unit.source);
    }
    SemanticChangeId(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}
