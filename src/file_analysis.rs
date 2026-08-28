use crate::{
    cache::ExplanationCache,
    config::{ExplanationConfig, ReaderConfig, ResolvedProfile},
    model::{ExplanationRequest, UnitExplanation},
    semantic::SemanticDocument,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct FileAnalysis {
    pub revision: Option<String>,
    pub documents: Vec<SemanticDocument>,
    pub explanations: BTreeMap<String, UnitExplanation>,
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
        for document in &self.documents {
            for unit in &document.units {
                let request = request_for(unit, self.revision.as_deref(), false);
                let key = ExplanationCache::snapshot_key(&request, model, reader, explanation);
                if let Ok(Some(json)) = cache.get(&key) {
                    if let Ok(value) = serde_json::from_str(&json) {
                        self.explanations.insert(unit_key(unit), value);
                    }
                }
            }
        }
    }
}

pub fn request_for(
    unit: &crate::change_set::SemanticUnit,
    revision: Option<&str>,
    deep: bool,
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
        prior_explanation: None,
        deep,
    }
}

pub fn unit_key(unit: &crate::change_set::SemanticUnit) -> String {
    format!(
        "{}:{:?}:{}:{}",
        unit.location.path.display(),
        unit.identity.kind,
        unit.identity.qualified_name,
        unit.identity.occurrence
    )
}
