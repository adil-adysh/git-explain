//! Independent counterexamples for the infrastructure parser review.
use git_explain::{
    change_set::{SemanticUnitChangeKind, SourceFile},
    diff::LineRange,
    language::{LanguageRegistry, SourceUnitKind},
    semantic::{extract_document, match_documents},
};
use std::path::Path;

fn units(path: &str, source: &str) -> Vec<git_explain::language::SourceUnit> {
    LanguageRegistry::find_changed_units(
        Path::new(path),
        source,
        &[LineRange {
            start: 1,
            end: source.lines().count().max(1),
        }],
    )
    .unwrap()
}

#[test]
fn compact_members_have_one_coherent_source_view() {
    for (path, source) in [
        ("config.json", r#"{"left": 1, "right": 2}"#),
        ("config.jsonc", r#"{"left": 1, "right": 2,}"#),
        ("config.yaml", "{left: 1, right: 2}\n"),
    ] {
        let sections = units(path, source);
        assert_eq!(
            sections.len(),
            1,
            "{path}: compact source must not be copied into multiple overlapping sections"
        );
        assert_eq!(sections[0].source, source.trim_end_matches('\n'));
    }
}

#[test]
fn ambiguous_docker_names_use_the_same_detector_and_analyzer() {
    for path in ["Dockerfile.rs", "Dockerfile.sh", "Containerfile.json"] {
        assert_eq!(
            LanguageRegistry::language_for_path(Path::new(path)),
            Some("Dockerfile")
        );
        let sections = units(path, "FROM alpine\nRUN echo ready\n");
        assert_eq!(
            sections.len(),
            2,
            "{path}: detector and analyzer must agree"
        );
        assert_eq!(sections[1].start_line, 2);
    }
}

#[test]
fn changing_statement_arguments_is_a_modification() {
    for (path, before, after) in [
        (
            "Dockerfile",
            "FROM alpine\nRUN echo before\n",
            "FROM alpine\nRUN echo after\n",
        ),
        ("main.sh", "echo before\n", "echo after\n"),
        ("main.ps1", "Write-Host 'before'\n", "Write-Host 'after'\n"),
        ("build.kts", "println(\"before\")\n", "println(\"after\")\n"),
        ("list.yaml", "- before\n", "- after\n"),
        ("value.yaml", "before\n", "after\n"),
    ] {
        let old = extract_document(&SourceFile {
            path: path.into(),
            content: before.into(),
        })
        .unwrap();
        let new = extract_document(&SourceFile {
            path: path.into(),
            content: after.into(),
        })
        .unwrap();
        let changes = match_documents(Some(&old), Some(&new), String::new());
        assert_eq!(
            changes
                .iter()
                .filter(|c| c.kind == SemanticUnitChangeKind::Modified)
                .count(),
            1,
            "{path}: argument changes need stable statement identity"
        );
        assert!(
            !changes.iter().any(|c| matches!(
                c.kind,
                SemanticUnitChangeKind::Added | SemanticUnitChangeKind::Deleted
            )),
            "{path}"
        );
    }
}

#[test]
fn strict_json_comments_and_multiple_roots_do_not_become_fine_members() {
    for source in [
        "{\n// comment\n\"a\":1,\n\"b\":2\n}\n",
        "{\"a\":1}\n{\"b\":2}\n",
    ] {
        let sections = units("config.json", source);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].source, source.trim_end_matches('\n'));
    }
}

#[test]
fn yaml_literal_stops_before_the_next_document() {
    let source = "data:\n  config: |\n    alpha\n    beta\n---\nkind: Service\n";
    let sections = units("config.yaml", source);
    let data = sections.iter().find(|s| s.name == "data").unwrap();
    assert_eq!(data.end_line, 4);
    assert_eq!(data.source, "data:\n  config: |\n    alpha\n    beta");
    assert!(sections
        .iter()
        .any(|s| s.name == "kind" && s.start_line == 6));
}

#[test]
fn jsonc_normalization_preserves_string_contents_and_original_lines() {
    let source = "{\n  \"url\": \"https://host/path,} // text\",\n  \"escaped\": \"quote: \\\" and café\", /* keep */\n}\n";
    let sections = units("config.jsonc", source);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].source, source.lines().nth(1).unwrap());
    assert_eq!(sections[1].source, source.lines().nth(2).unwrap());
}

#[test]
fn kotlin_properties_are_not_compile_time_constants_from_their_names() {
    let sections = units(
        "Config.kt",
        "var count = 1\nval DEFAULT_VERSION = readVersion()\nconst val LIMIT = 2\n",
    );
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].kind, SourceUnitKind::Property);
    assert_eq!(sections[1].kind, SourceUnitKind::Property);
    assert_eq!(sections[2].kind, SourceUnitKind::Constant);
}
