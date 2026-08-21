use git_explain::web::escape;
use git_explain::{
    explain::ExplainedFunction,
    language::{SourceSymbol, SymbolKind},
    model::{Annotation, FunctionExplanation},
};

#[test]
fn html_escaping_is_safe() {
    assert_eq!(escape("<script>&\""), "&lt;script&gt;&amp;&quot;");
}

#[test]
fn annotated_render_preserves_source_lines() {
    let item = ExplainedFunction {
        file: "src/example.rs".into(),
        language: "Rust".into(),
        symbol: SourceSymbol {
            name: "example".into(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 3,
            source: "fn example() {\n    changed();\n}".into(),
        },
        explanation: FunctionExplanation {
            overview: "Overview".into(),
            annotations: vec![Annotation {
                start_line: 2,
                end_line: 2,
                kind: "change".into(),
                text: "Explanation".into(),
            }],
            deep: None,
        },
    };
    let html = git_explain::web::render(&[item]);
    assert_eq!(html.matches("changed();").count(), 1);
    assert!(html.find("changed();").unwrap() < html.find("<p>Explanation</p>").unwrap());
}
