//! Independent provenance and parser-fragment counterexamples.
use git_explain::language::facts::{extract_evidence, SourceFact};

#[test]
fn jsonc_document_comments_are_not_mistaken_for_a_member_fragment() {
    for source in [
        "// leading comment\n{\"key\":42,}\n",
        "/* leading café */\r\n{\r\n  \"key\":42,\r\n}\r\n",
    ] {
        let evidence = extract_evidence("JSONC", source);
        assert!(
            !evidence.facts.is_empty(),
            "valid commented JSONC has member evidence"
        );
        assert!(evidence.matches_source(source));
        for fact in evidence.facts {
            let SourceFact::Syntax { span, syntax, .. } = fact else {
                panic!("unexpected inferred fact")
            };
            assert_eq!(syntax, source[span.start_byte..span.end_byte]);
            assert!(syntax.contains("\"key\":42"));
        }
    }
}

#[test]
fn crlf_unicode_members_keep_original_bytes_and_line_numbers() {
    let source = "\r\n  \"café\": [true, false,], // unchanged comment\r\n";
    let evidence = extract_evidence("JSONC", source);
    assert!(!evidence.facts.is_empty());
    assert!(evidence.matches_source(source));
    for fact in evidence.facts {
        let SourceFact::Syntax { span, syntax, .. } = fact else {
            panic!("unexpected inferred fact")
        };
        assert_eq!(span.start_line, 2);
        assert_eq!(span.end_line, 2);
        assert_eq!(syntax, source[span.start_byte..span.end_byte]);
        assert!(source.is_char_boundary(span.start_byte));
        assert!(source.is_char_boundary(span.end_byte));
    }
}

#[test]
fn multibyte_final_tokens_are_safe_in_config_and_commands() {
    for (language, source) in [("YAML", "name: café\n"), ("Bash", "echo café\n")] {
        let evidence = extract_evidence(language, source);
        assert!(!evidence.facts.is_empty(), "{language}");
        for fact in evidence.facts {
            let span = fact.span();
            assert_eq!(span.start_line, 1);
            assert_eq!(span.end_line, 1);
            assert!(source.is_char_boundary(span.end_byte));
        }
    }
}

#[test]
fn malformed_new_language_input_never_uses_the_legacy_extractor() {
    for language in [
        "HCL",
        "YAML",
        "JSON",
        "JSONC",
        "Dockerfile",
        "Bash",
        "PowerShell",
        "Kotlin",
    ] {
        let result =
            git_explain::language::infra_facts::extract_infra_facts(language, "\"unterminated");
        assert!(
            result.is_some(),
            "{language}: None is reserved for legacy languages"
        );
        assert!(result.unwrap().is_empty(), "{language}");
    }
}
