//! Revision and deletion contracts over original synthetic infrastructure input.
use git_explain::{
    change_set::{SemanticUnitChangeKind, SourceFile},
    language::facts::extract_evidence,
    semantic::{extract_document, match_documents},
};

#[test]
fn new_formats_preserve_deleted_revision_source_and_facts() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/infra-languages.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let path = fixture["path"].as_str().unwrap();
        let source = fixture["before"].as_str().unwrap();
        let before = extract_document(&SourceFile {
            path: path.into(),
            content: source.into(),
        })
        .unwrap();
        assert!(!before.units.is_empty(), "{path}");
        let changes = match_documents(Some(&before), None, String::new());
        assert_eq!(changes.len(), before.units.len());
        for change in &changes {
            assert_eq!(change.kind, SemanticUnitChangeKind::Deleted, "{path}");
            assert!(change.after.is_none());
            // Matching is identity-sorted, so find the exact before unit by identity.
            let actual = change.before.as_ref().unwrap();
            let original = before
                .units
                .iter()
                .find(|unit| unit.identity == actual.identity)
                .unwrap();
            assert_eq!(actual.source, original.source);
            assert_eq!(actual.location, original.location);
            assert_eq!(actual.language, fixture["language"].as_str().unwrap());
            let facts = extract_evidence(&actual.language, &actual.source);
            assert!(facts.matches_source(&original.source));
            assert!(
                !facts.facts.is_empty(),
                "{path}: deleted section lacks source evidence"
            );
            for fact in facts.facts {
                let span = fact.span();
                assert!(span.end_byte <= original.source.len());
                assert!(original.source.is_char_boundary(span.start_byte));
                assert!(original.source.is_char_boundary(span.end_byte));
            }
        }
    }
}
