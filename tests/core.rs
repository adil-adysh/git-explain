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
    assert!(
        html.contains(r#"<div class="source-region rendered-source"><pre><code>fn example() {"#)
    );
    assert!(html.contains("<textarea"));
    assert!(html.contains("readonly"));
    assert!(html.contains("spellcheck=\"false\""));
    assert!(html.contains("changed code unit"));
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
    assert!(!html.contains(">Regenerate explanation<"));
    assert!(!html.contains(">Regenerate detailed explanation<"));
    assert!(html.contains("<button"));
    assert!(html.contains("changed();"));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("data-unit-id=\"unit-b\""));
    assert!(html.contains("generation"));
}

fn action_state_item(overview: &str, deep: Option<&str>) -> ExplainedUnit {
    ExplainedUnit {
        id: UnitId("actions".into()),
        file: "src/actions.rs".into(),
        language: "Rust".into(),
        diff: "+changed();".into(),
        regions: vec![],
        unit: SourceUnit {
            name: "load".into(),
            qualified_name: None,
            kind: SourceUnitKind::Function,
            start_line: 1,
            end_line: 1,
            source: "fn load() { changed(); }".into(),
        },
        explanation: UnitExplanation {
            overview: overview.into(),
            annotations: vec![],
            deep: None,
        },
        deep_explanation: deep.map(str::to_owned),
    }
}

#[test]
fn explanation_actions_match_all_four_states() {
    let cases = [
        (
            "",
            None,
            "Generate explanation",
            "Explain this code in depth",
            "normal-generate",
            "/explain",
            "deep-generate",
            "/deep",
        ),
        (
            "Overview",
            None,
            "Regenerate explanation",
            "Explain this code in depth",
            "normal-regenerate",
            "/regenerate",
            "deep-generate",
            "/deep",
        ),
        (
            "",
            Some("Deep"),
            "Generate explanation",
            "Regenerate detailed explanation",
            "normal-generate",
            "/explain",
            "deep-regenerate",
            "/deep/regenerate",
        ),
        (
            "Overview",
            Some("Deep"),
            "Regenerate explanation",
            "Regenerate detailed explanation",
            "normal-regenerate",
            "/regenerate",
            "deep-regenerate",
            "/deep/regenerate",
        ),
    ];
    for (
        overview,
        deep,
        normal_label,
        deep_label,
        normal_action,
        normal_endpoint,
        deep_action,
        deep_endpoint,
    ) in cases
    {
        let html = git_explain::web::render(
            &[action_state_item(overview, deep)],
            &AnalysisContext::working_tree(),
        );
        assert!(html.contains(&format!(">{normal_label}<")));
        assert!(html.contains(&format!(">{deep_label}<")));
        assert!(html.contains(&format!("data-action=\"{normal_action}\"")));
        assert!(html.contains(&format!("data-endpoint=\"{normal_endpoint}\"")));
        assert!(html.contains(&format!("data-action=\"{deep_action}\"")));
        assert!(html.contains(&format!("data-endpoint=\"{deep_endpoint}\"")));
        assert_eq!(
            html.matches(">Generate explanation<").count(),
            usize::from(normal_label == "Generate explanation")
        );
        assert_eq!(
            html.matches(">Regenerate explanation<").count(),
            usize::from(normal_label == "Regenerate explanation")
        );
        assert_eq!(
            html.matches(">Explain this code in depth<").count(),
            usize::from(deep_label == "Explain this code in depth")
        );
        assert_eq!(
            html.matches(">Regenerate detailed explanation<").count(),
            usize::from(deep_label == "Regenerate detailed explanation")
        );
    }
}

#[test]
fn explanation_actions_transition_without_reload_and_preserve_visibility() {
    let html = git_explain::web::render(
        &[action_state_item("", None)],
        &AnalysisContext::working_tree(),
    );
    assert!(html.contains("button.dataset.action='normal-regenerate'"));
    assert!(html.contains("button.dataset.endpoint='/regenerate'"));
    assert!(html.contains("button.textContent='Regenerate explanation'"));
    assert!(html.contains("button.dataset.action='deep-regenerate'"));
    assert!(html.contains("button.dataset.endpoint='/deep/regenerate'"));
    assert!(html.contains("button.textContent='Regenerate detailed explanation'"));
    assert!(html.contains("let explanationsHidden=false"));
    assert!(html.contains("section.hidden=explanationsHidden"));
    assert!(html.contains("class=\"annotation ai-explanation\"'+(explanationsHidden?' hidden':'')"));
}

#[test]
fn code_reader_preserves_whitespace_and_has_native_text_controls() {
    let source = "fn load() {\n\tif ready {\n        run();\n\n\t}\n}";
    let item = ExplainedUnit {
        id: UnitId("unit-reader".into()),
        file: "src/example.rs".into(),
        language: "Rust".into(),
        diff: String::new(),
        regions: vec![],
        unit: SourceUnit {
            name: "load".into(),
            qualified_name: None,
            kind: SourceUnitKind::Function,
            start_line: 4,
            end_line: 8,
            source: source.into(),
        },
        explanation: UnitExplanation {
            overview: String::new(),
            annotations: vec![],
            deep: None,
        },
        deep_explanation: None,
    };
    let html = git_explain::web::render(&[item], &AnalysisContext::working_tree());
    assert!(html.contains("<textarea"));
    assert!(html.contains("readonly spellcheck=\"false\" wrap=\"off\""));
    assert!(html.contains("Source code for load, read only"));
    assert!(html.contains(&format!(">{source}</textarea>")));
    assert!(!html.contains(">Indent 8 spaces</textarea>"));
    assert!(html.contains("\tif ready"));
    assert!(html.contains("        run();"));
    assert!(html.contains("<span class=\"sr-only\">, indent 1 tab, </span>"));
    assert!(html.contains("<span class=\"sr-only\">, indent 8 spaces, </span>"));
    assert!(html.contains("blank line"));
    assert!(html.contains("Read code as text"));
    assert!(html.contains("Show indentation details"));
}

#[test]
fn code_modes_hide_duplicate_accessible_source() {
    let item = ExplainedUnit {
        id: UnitId("unit-modes".into()),
        file: "src/example.rs".into(),
        language: "Rust".into(),
        diff: String::new(),
        regions: vec![],
        unit: SourceUnit {
            name: "example".into(),
            qualified_name: None,
            kind: SourceUnitKind::Function,
            start_line: 1,
            end_line: 1,
            source: "example();".into(),
        },
        explanation: UnitExplanation {
            overview: String::new(),
            annotations: vec![],
            deep: None,
        },
        deep_explanation: None,
    };
    let html = git_explain::web::render(&[item], &AnalysisContext::working_tree());
    assert!(html.contains("class=\"source-region rendered-source\""));
    assert!(html.contains("class=\"source-region text-source\" hidden"));
    assert!(html.contains("mode.textContent=textMode?'Show rendered code':'Read code as text'"));
    assert!(html.contains("rendered.hidden=textMode;text.hidden=!textMode"));
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
