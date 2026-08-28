use crate::semantic::SemanticDocument;

#[derive(Clone, Debug)]
pub struct FileAnalysis {
    pub revision: Option<String>,
    pub documents: Vec<SemanticDocument>,
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
            }
        }
        output
    }
}
