use crate::{
    cache::ExplanationCache,
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    model::{ExplanationRequest, UnitExplanation},
    semantic::SemanticDocument,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct FileAnalysis {
    pub revision: Option<String>,
    pub documents: Vec<SemanticDocument>,
    pub explanations: BTreeMap<String, UnitExplanation>,
    pub deep_explanations: BTreeMap<String, String>,
}

impl FileAnalysis {
    pub fn format_debug(&self) -> String {
        use std::fmt::Write;

        let mut output = String::from("File analysis\n");
        writeln!(
            output,
            "\nRevision: {}",
            self.revision.as_deref().unwrap_or("working tree")
        )
        .unwrap();
        writeln!(output, "Files: {}", self.documents.len()).unwrap();
        for document in &self.documents {
            writeln!(output, "\nFile: {}", document.source.path.display()).unwrap();
            writeln!(
                output,
                "Language: {}",
                document.language.as_deref().unwrap_or("unsupported")
            )
            .unwrap();
            writeln!(output, "Semantic units: {}", document.units.len()).unwrap();
            for unit in &document.units {
                writeln!(
                    output,
                    "\n  Unit\n    Kind: {:?}\n    Name: {}\n    Lines: {}-{}\n\n{}",
                    unit.identity.kind,
                    unit.identity.qualified_name,
                    unit.location.start_line,
                    unit.location.end_line,
                    unit.source
                )
                .unwrap();
                if let Some(explanation) = self.explanations.get(&unit_key(unit)) {
                    writeln!(output, "\n    Explanation: {}", explanation.overview).unwrap();
                }
            }
        }
        output
    }

    pub fn hydrate(
        &mut self,
        cache: &ExplanationCache,
        model: &ResolvedProfile,
        reader: &ReaderConfig,
        explanation: &ExplanationConfig,
    ) {
        let mut normal = vec![];
        for document in &self.documents {
            for unit in &document.units {
                let request = request_for(unit, self.revision.as_deref(), false, None);
                let key = ExplanationCache::snapshot_key(&request, model, reader, explanation);
                if let Ok(Some(json)) = cache.get(&key) {
                    if let Ok(value) = serde_json::from_str::<UnitExplanation>(&json) {
                        normal.push((unit_key(unit), value));
                    }
                }
            }
        }
        for (id, value) in normal {
            self.apply(&id, value, false);
        }
        let mut deep = vec![];
        for document in &self.documents {
            for unit in &document.units {
                let id = unit_key(unit);
                let prior = self
                    .explanations
                    .get(&id)
                    .map(|value| value.overview.clone());
                let request = request_for(unit, self.revision.as_deref(), true, prior);
                let key = ExplanationCache::snapshot_key(&request, model, reader, explanation);
                if let Ok(Some(json)) = cache.get(&key) {
                    if let Ok(value) = serde_json::from_str::<UnitExplanation>(&json) {
                        deep.push((id, value));
                    }
                }
            }
        }
        for (id, value) in deep {
            self.apply(&id, value, true);
        }
    }

    pub fn find_unit(&self, id: &str) -> Option<&crate::change_set::SemanticUnit> {
        self.documents
            .iter()
            .flat_map(|document| &document.units)
            .find(|unit| unit_key(unit) == id)
    }

    pub fn apply(&mut self, id: &str, result: UnitExplanation, deep: bool) -> bool {
        if self.find_unit(id).is_none() {
            return false;
        }
        if deep {
            self.deep_explanations
                .insert(id.into(), result.deep.unwrap_or(result.overview));
        } else {
            self.explanations.insert(id.into(), result);
        }
        true
    }
}

pub fn request_for(
    unit: &crate::change_set::SemanticUnit,
    revision: Option<&str>,
    deep: bool,
    prior_explanation: Option<String>,
) -> ExplanationRequest {
    ExplanationRequest {
        source_unit: unit.source.clone(),
        unit_name: unit.identity.qualified_name.clone(),
        unit_kind: format!("{:?}", unit.identity.kind),
        diff: String::new(),
        language: unit.language.clone(),
        git_context: format!(
            "Semantic file snapshot\nRevision: {}\nPath: {}",
            revision.unwrap_or("working tree"),
            unit.location.path.display()
        ),
        regions: vec![],
        prior_explanation,
        deep,
    }
}

pub fn unit_key(unit: &crate::change_set::SemanticUnit) -> String {
    let identity = format!(
        "{}:{:?}:{}:{}:{}",
        unit.location.path.display(),
        unit.identity.kind,
        unit.identity.qualified_name,
        unit.identity.occurrence,
        unit.source
    );
    format!("{:x}", Sha256::digest(identity.as_bytes()))
}
