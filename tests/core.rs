use git_explain::web::escape;
use git_explain::{
    explain::AnalysisContext,
    explain::ExplainedUnit,
    language::{SourceUnit, SourceUnitKind},
    model::{Annotation, UnitExplanation},
    snapshot::UnitId,
};

#[test]
fn html_escaping_is_safe() {
    assert_eq!(escape("<script>&\""), "&lt;script&gt;&amp;&quot;");
}

#[test]
fn annotated_render_preserves_source_lines() {
    let item = ExplainedUnit {
        id: UnitId("unit-a".into()),
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
    assert_eq!(html.matches("<code>    changed();</code>").count(), 1);
    assert!(html.contains("File changes"));
    assert!(html.contains("Changed code units"));
    assert!(html.find("changed();").unwrap() < html.find("<p>Explanation</p>").unwrap());
}

#[test]
fn initial_render_keeps_source_visible_without_explanation() {
    let item = ExplainedUnit {
        id: UnitId("unit-b".into()),
        file: "src/example.rs".into(),
        language: "Rust".into(),
        diff: "+changed();".into(),
        regions: vec![],
        unit: SourceUnit {
            name: "example".into(),
            qualified_name: None,
            kind: SourceUnitKind::Function,
            start_line: 1,
            end_line: 1,
            source: "changed();".into(),
        },
        explanation: UnitExplanation {
            overview: String::new(),
            annotations: vec![],
            deep: None,
        },
        deep_explanation: None,
    };
    let html = git_explain::web::render(&[item], &AnalysisContext::working_tree());
    assert!(html.contains("Explanation has not been generated."));
    assert!(html.contains("Generate explanation"));
    assert!(html.contains("<button"));
    assert!(html.contains("changed();"));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("data-unit-id=\"unit-b\""));
    assert!(html.contains("generation"));
}

#[test]
fn daemon_render_carries_generation_and_update_action() {
    let html = git_explain::web::render_for_session_at_generation(
        &[],
        &AnalysisContext::working_tree(),
        "session-opaque",
        7,
    );
    assert!(html.contains("data-generation=\"7\""));
    assert!(html.contains("A newer repository snapshot is available"));
    assert!(html.contains("Reload updated snapshot"));
    assert!(html.contains("'/api/sessions/'+session+'/snapshot'"));
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
