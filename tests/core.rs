use git_explain::web::escape;
use git_explain::{
    explain::AnalysisContext,
    explain::ExplainedUnit,
    language::{SourceUnit, SourceUnitKind},
    model::{Annotation, UnitExplanation},
};

#[test]
fn html_escaping_is_safe() {
    assert_eq!(escape("<script>&\""), "&lt;script&gt;&amp;&quot;");
}

#[test]
fn annotated_render_preserves_source_lines() {
    let item = ExplainedUnit {
        file: "src/example.rs".into(),
        language: "Rust".into(),
        diff: "+changed();".into(),
        regions: vec![],
        unit: SourceUnit {
            name: "example".into(),
            qualified_name: None,
            kind: SourceUnitKind::Function,
            start_line: 1,
            end_line: 3,
            source: "fn example() {\n    changed();\n}".into(),
        },
        explanation: UnitExplanation {
            overview: "Overview".into(),
            annotations: vec![Annotation {
                start_line: 2,
                end_line: 2,
                kind: "change".into(),
                text: "Explanation".into(),
            }],
            deep: None,
        },
        deep_explanation: None,
    };
    let html = git_explain::web::render(&[item], &AnalysisContext::working_tree());
    assert_eq!(html.matches("changed();").count(), 1);
    assert!(html.contains("File changes"));
    assert!(html.contains("Changed code units"));
    assert!(html.find("changed();").unwrap() < html.find("<p>Explanation</p>").unwrap());
}

#[test]
fn commit_render_identifies_revision_parent_and_deleted_files() {
    let context = AnalysisContext {
        mode: git_explain::explain::AnalysisMode::Commit {
            oid: "abcdef1234567890".into(),
            parent_oid: Some("1234567890abcdef".into()),
            subject: "change <subject>".into(),
            merge_parent_count: 1,
        },
        deleted_files: vec!["src/old.rs".into()],
    };
    let html = git_explain::web::render(&[], &context);
    assert!(html.contains("Commit abcdef123456"));
    assert!(html.contains("Compared with parent: 1234567890abcdef"));
    assert!(html.contains("Deleted file: src/old.rs"));
    assert!(html.contains("change &lt;subject&gt;"));
}
