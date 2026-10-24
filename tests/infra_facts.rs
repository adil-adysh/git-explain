use git_explain::language::facts::{
    extract_evidence, SourceFact, EVIDENCE_VERSION, MAX_FACTS, MAX_FACT_TEXT_BYTES,
    MAX_TOTAL_TEXT_BYTES,
};

#[test]
fn every_new_family_has_exact_syntax_facts() {
    for (language, source) in [
        (
            "HCL",
            "resource \"example_service\" \"api\" {\n  replicas = 2\n}\n",
        ),
        ("YAML", "spec:\n  replicas: 2\n"),
        ("JSON", "{\"retries\":2}"),
        ("JSONC", "{\n// café\n\"options\":[true,false,],\n}\n"),
        ("Dockerfile", "FROM alpine\nRUN echo ready\n"),
        (
            "Bash",
            "save() {\n  record=$(fetch_record \"$1\")\n  write_record \"$record\"\n}\n",
        ),
        (
            "PowerShell",
            "function Save-Settings {\n  $record = Get-Record\n  $record | Write-Record\n}\n",
        ),
        (
            "Kotlin",
            "fun save(input: String?): Boolean {\n  val value = input ?: return false\n  writeRecord(value)\n  return true\n}\n",
        ),
    ] {
        let evidence = extract_evidence(language, source);
        assert!(!evidence.facts.is_empty(), "missing facts: {language}");
        for fact in evidence.facts {
            let span = fact.span();
            assert!(span.start_byte < span.end_byte && span.end_byte <= source.len());
            assert!(
                source.is_char_boundary(span.start_byte) && source.is_char_boundary(span.end_byte)
            );
            let json = serde_json::to_value(&fact).unwrap();
            assert_eq!(
                json["syntax"].as_str().unwrap(),
                &source[span.start_byte..span.end_byte]
            );
        }
    }
}

#[test]
fn all_infra_languages_and_kotlin_extract_useful_evidence() {
    let cases = [
        (
            "hcl",
            r#"resource "aws_instance" "web" {
  ami           = "ami-12345"
  instance_type = "t3.micro"
}
"#,
        ),
        (
            "yaml",
            r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: game-config
data:
  game.properties: |
    enemies=10
    lives=3
"#,
        ),
        (
            "json",
            r#"{
  "name": "my-app",
  "version": "1.0.0",
  "private": true
}
"#,
        ),
        (
            "jsonc",
            r#"{
  // Application settings
  "name": "my-app",
  "options": [true, false,],
}
"#,
        ),
        (
            "dockerfile",
            r#"FROM alpine:3.19
RUN apk add --no-cache curl
COPY . /app
WORKDIR /app
CMD ["./run.sh"]
"#,
        ),
        (
            "bash",
            r#"function deploy_app() {
  local target="$1"
  echo "Deploying to $target"
  docker build -t app:latest .
  return 0
}
"#,
        ),
        (
            "powershell",
            r#"function Publish-Artifact {
  param([string]$Path)
  Write-Output "Publishing $Path"
  return $true
}
"#,
        ),
        (
            "kotlin",
            r#"fun calculateScore(base: Int): Int {
  if (base >= 100) {
    return computeBonus(base)
  }
  return base
}
"#,
        ),
    ];

    for (lang, source) in cases {
        let evidence = extract_evidence(lang, source);
        assert_eq!(
            evidence.version, EVIDENCE_VERSION,
            "{lang}: expected evidence version {EVIDENCE_VERSION}"
        );
        assert_eq!(
            evidence.version, "source-evidence-v2",
            "{lang}: must be source-evidence-v2"
        );
        assert!(
            evidence.matches_source(source),
            "{lang}: source hash must match original source"
        );
        assert!(
            !evidence.facts.is_empty(),
            "{lang}: expected non-empty facts"
        );

        let mut total_summary_bytes = 0;
        for fact in &evidence.facts {
            let span = fact.span();
            assert!(
                span.start_byte < span.end_byte,
                "{lang}: span start must be before end"
            );
            assert!(
                span.end_byte <= source.len(),
                "{lang}: span end must be within source"
            );
            assert!(
                source.is_char_boundary(span.start_byte),
                "{lang}: start_byte on char boundary"
            );
            assert!(
                source.is_char_boundary(span.end_byte),
                "{lang}: end_byte on char boundary"
            );

            // Line numbers must be 1-based and match original source byte slice
            let expected_start_line = source.as_bytes()[..span.start_byte]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1;
            assert_eq!(
                span.start_line, expected_start_line,
                "{lang}: start line mismatch"
            );
            let last_content_byte = span.end_byte.saturating_sub(1);
            let expected_end_line = source.as_bytes()[..last_content_byte]
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                + 1;
            assert_eq!(
                span.end_line, expected_end_line,
                "{lang}: end line mismatch"
            );

            let summary = fact.display_summary();
            assert!(
                summary.len() <= MAX_FACT_TEXT_BYTES,
                "{lang}: fact summary exceeded MAX_FACT_TEXT_BYTES: {summary}"
            );
            total_summary_bytes += summary.len();
        }

        assert!(
            total_summary_bytes <= MAX_TOTAL_TEXT_BYTES,
            "{lang}: total summary bytes exceeded limit"
        );
    }
}

#[test]
fn json_and_jsonc_member_fragment_and_unicode_handling() {
    // 1. JSONC member snippet with trailing comma without outer braces
    let jsonc_fragment = "  \"options\": [true, false,],";
    let ev_jsonc = extract_evidence("jsonc", jsonc_fragment);
    assert!(
        !ev_jsonc.facts.is_empty(),
        "JSONC fragment should yield facts"
    );
    for fact in &ev_jsonc.facts {
        let span = fact.span();
        let slice = &jsonc_fragment[span.start_byte..span.end_byte];
        assert!(
            slice.contains("\"options\""),
            "Span should cover member without synthetic wrapper braces"
        );
        assert!(
            span.end_byte <= jsonc_fragment.len(),
            "Span must never exceed original fragment"
        );
    }

    // 2. Strict JSON member snippet without braces
    let json_fragment = "  \"release\": 42";
    let ev_json = extract_evidence("json", json_fragment);
    assert!(!ev_json.facts.is_empty(), "Strict JSON fragment facts");

    // 3. Strict JSON must reject comments even in fragment
    let strict_with_comment = "// comment\n  \"key\": \"value\"";
    let ev_strict = extract_evidence("json", strict_with_comment);
    assert!(
        ev_strict.facts.is_empty(),
        "Strict JSON with comments must produce no facts"
    );

    // 4. Unicode and CRLF in JSON
    let unicode_json = "{\r\n  \"naïve\": \"café\",\r\n  \"count\": 1\r\n}";
    let ev_unicode = extract_evidence("json", unicode_json);
    assert!(!ev_unicode.facts.is_empty());
    for fact in &ev_unicode.facts {
        let span = fact.span();
        let slice = &unicode_json[span.start_byte..span.end_byte];
        assert!(!slice.is_empty());
        assert!(unicode_json.is_char_boundary(span.start_byte));
        assert!(unicode_json.is_char_boundary(span.end_byte));
    }

    // 5. Malformed whole roots must not become accepted by wrapping
    let malformed_whole_root = "{\n  \"broken\": [1, 2,\n";
    let ev_broken = extract_evidence("json", malformed_whole_root);
    assert!(
        ev_broken.facts.is_empty(),
        "Malformed whole root JSON must not yield facts"
    );
}

#[test]
fn malformed_sources_produce_no_fabricated_facts() {
    let malformed_cases = [
        ("hcl", "resource \"aws\" { { broken = = ="),
        ("yaml", "foo: bar:\n  - : : [broken"),
        ("json", "{ \"unterminated string"),
        ("jsonc", "{ /* unclosed comment "),
        ("dockerfile", "FROM\nRUN\nCOPY\n<<<"),
        ("bash", "function { (( broken syntax ;;"),
        ("powershell", "function { broken $$ %%%"),
        ("kotlin", "fun ( { class { =="),
    ];

    for (lang, source) in malformed_cases {
        let evidence = extract_evidence(lang, source);
        assert!(
            evidence.facts.is_empty(),
            "{lang}: malformed source must produce 0 facts, got: {:?}",
            evidence.facts
        );
    }
}

#[test]
fn unseen_commands_produce_no_invented_types_or_effects() {
    // Shell command with unseen helper
    let bash_cmd = "deploy_service --replicas 5 --env production\n";
    let ev_bash = extract_evidence("bash", bash_cmd);
    assert!(!ev_bash.facts.is_empty());
    for fact in &ev_bash.facts {
        let summary = fact.display_summary();
        assert!(
            summary.contains("unresolved") || summary.contains("external behavior"),
            "Bash summary should state unresolved behavior: {summary}"
        );
        // Ensure not reported as typed Call with assumed arity
        assert!(
            !matches!(fact, SourceFact::Call { .. }),
            "Shell command should be SourceFact::Syntax, not SourceFact::Call"
        );
    }

    // PowerShell command
    let ps_cmd = "Invoke-DeploymentStep -Stage 'Validation' -Verbose\n";
    let ev_ps = extract_evidence("powershell", ps_cmd);
    assert!(!ev_ps.facts.is_empty());
    for fact in &ev_ps.facts {
        let summary = fact.display_summary();
        assert!(
            summary.contains("unresolved") || summary.contains("external behavior"),
            "PowerShell summary should state unresolved behavior: {summary}"
        );
    }
}

#[test]
fn kotlin_nullable_guard_and_extension_expressions() {
    let kt_source = r#"fun String.isLong(): Boolean = this.length > 10

fun processNullable(item: String?): Int {
  if (item != null) {
    return item.length
  }
  return 0
}
"#;
    let evidence = extract_evidence("kotlin", kt_source);
    assert!(
        !evidence.facts.is_empty(),
        "Kotlin evidence should not be empty"
    );

    let has_comparison = evidence.facts.iter().any(|f| match f {
        SourceFact::Comparison { operator, .. } => operator == ">" || operator == "!=",
        SourceFact::Syntax { syntax, .. } => syntax.contains('>') || syntax.contains("!="),
        _ => false,
    });
    assert!(has_comparison, "Expected comparison fact in Kotlin");

    let has_return = evidence.facts.iter().any(|f| match f {
        SourceFact::Return { .. } => true,
        SourceFact::Syntax { syntax, .. } => syntax.contains("return"),
        _ => false,
    });
    assert!(has_return, "Expected return fact in Kotlin");
}

#[test]
fn non_ascii_final_character_in_ast_nodes_does_not_panic() {
    let cases = [
        ("yaml", "name: café\n"),
        ("bash", "echo café\n"),
        ("powershell", "Write-Output café\n"),
        ("kotlin", "val name = \"café\"\n"),
        ("json", "{\"greeting\": \"café\"}"),
        ("hcl", "name = \"café\"\n"),
    ];
    for (lang, source) in cases {
        let evidence = extract_evidence(lang, source);
        assert!(
            !evidence.facts.is_empty(),
            "{lang}: expected facts for multibyte trailing chars"
        );
        for fact in &evidence.facts {
            let span = fact.span();
            assert!(span.end_byte <= source.len());
            assert!(source.is_char_boundary(span.start_byte));
            assert!(source.is_char_boundary(span.end_byte));
            assert_eq!(
                &source[span.start_byte..span.end_byte],
                match fact {
                    SourceFact::Syntax { syntax, .. } => syntax,
                    _ => panic!("Expected syntax fact"),
                }
            );
        }
    }
}

#[test]
fn malformed_new_language_never_falls_through_to_legacy_extraction() {
    let malformed_yaml = "broken: [unclosed\n";
    let facts_opt = git_explain::language::infra_facts::extract_infra_facts("yaml", malformed_yaml);
    assert_eq!(
        facts_opt,
        Some(vec![]),
        "Malformed YAML must yield Some(empty), not None"
    );
}

#[test]
fn budgets_and_determinism() {
    // Large source with many items
    let mut large_json = String::from("{\n");
    for i in 0..100 {
        large_json.push_str(&format!("  \"key_{i}\": \"value_{i}\",\n"));
    }
    large_json.push_str("  \"key_last\": \"value_last\"\n}\n");

    let ev1 = extract_evidence("json", &large_json);
    let ev2 = extract_evidence("json", &large_json);

    assert_eq!(ev1.facts, ev2.facts, "Extraction must be deterministic");
    assert!(
        ev1.facts.len() <= MAX_FACTS,
        "Facts count must not exceed MAX_FACTS (32), got {}",
        ev1.facts.len()
    );

    let total_bytes: usize = ev1.facts.iter().map(|f| f.display_summary().len()).sum();
    assert!(
        total_bytes <= MAX_TOTAL_TEXT_BYTES,
        "Total bytes must not exceed MAX_TOTAL_TEXT_BYTES (2048), got {total_bytes}"
    );
}
