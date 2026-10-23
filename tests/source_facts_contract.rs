//! Independent grammar coverage checks against actual code-section fragments.
use git_explain::language::facts::{extract_evidence, SourceFact};

#[test]
fn every_supported_language_preserves_comparison_call_and_return_syntax() {
    let cases = [
        ("Rust", "fn check(n: i32) -> bool { if n >= 10 { return helper(n); } false }"),
        ("Python", "def check(n: int) -> bool:\n    if n >= 10:\n        return helper(n)\n    return False\n"),
        ("Go", "func check(n int) bool { if n >= 10 { return helper(n) }; return false }"),
        ("Java", "boolean check(int n) { if (n >= 10) { return helper(n); } return false; }"),
        ("C#", "bool Check(int n) { if (n >= 10) { return helper(n); } return false; }"),
        ("JavaScript", "function check(n) { if (n >= 10) { return helper(n); } return false; }"),
        ("TypeScript", "function check(n: number): boolean { if (n >= 10) { return helper(n); } return false; }"),
        ("C++", "bool check(int n) { if (n >= 10) { return helper(n); } return false; }"),
        ("C", "int check(int n) { if (n >= 10) { return helper(n); } return 0; }"),
    ];
    for (language, source) in cases {
        let evidence = extract_evidence(language, source);
        let comparison = evidence
            .facts
            .iter()
            .find(|fact| {
                matches!(fact, SourceFact::Comparison { operator, left, right, .. }
                if operator == ">=" && left == "n" && right == "10")
            })
            .unwrap_or_else(|| {
                panic!("{language}: missing exact comparison: {:?}", evidence.facts)
            });
        let span = comparison.span();
        assert_eq!(
            &source[span.start_byte..span.end_byte],
            "n >= 10",
            "{language}"
        );
        assert!(
            evidence.facts.iter().any(|fact| {
                matches!(fact, SourceFact::Call { callee, argument_count, .. }
                if callee == "helper" && *argument_count == Some(1))
            }),
            "{language}: missing actual helper call: {:?}",
            evidence.facts
        );
        assert!(
            evidence.facts.iter().any(|fact| {
                matches!(
                    fact,
                    SourceFact::Return {
                        is_tail_expression: false,
                        has_value: true,
                        ..
                    }
                )
            }),
            "{language}: missing explicit return: {:?}",
            evidence.facts
        );
        for fact in &evidence.facts {
            let span = fact.span();
            assert!(
                source.get(span.start_byte..span.end_byte).is_some(),
                "{language}: invalid byte span"
            );
            let expected_start = source[..span.start_byte]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            assert_eq!(
                span.start_line, expected_start,
                "{language}: wrapper offset leaked"
            );
        }
    }
}

#[test]
fn long_unicode_assertion_and_assert_named_user_call_are_safe_syntax() {
    let unicode = "🦀".repeat(45);
    let source = format!("fn check() {{ assert_eq!(message, \"{unicode}\"); assertiveness(); }}");
    let evidence = extract_evidence("Rust", &source);
    assert!(!evidence.facts.iter().any(|fact| {
        matches!(fact, SourceFact::Assertion { macro_or_fn, .. } if macro_or_fn == "assertiveness")
    }), "a helper name does not establish assertion behavior");
    for fact in &evidence.facts {
        let span = fact.span();
        assert!(source.get(span.start_byte..span.end_byte).is_some());
    }
}

#[test]
fn declared_parameter_types_are_observations_in_each_typed_language() {
    let cases = [
        ("Rust", "fn check(n: i32) {}", "i32"),
        ("Python", "def check(n: int):\n    pass\n", "int"),
        ("Go", "func check(n int) {}", "int"),
        ("Java", "void check(int n) {}", "int"),
        ("C#", "void Check(int n) {}", "int"),
        ("TypeScript", "function check(n: number): void {}", "number"),
        ("C++", "void check(int n) {}", "int"),
        ("C", "void check(int n) {}", "int"),
    ];
    for (language, source, expected_type) in cases {
        let evidence = extract_evidence(language, source);
        assert!(
            evidence.facts.iter().any(|fact| {
                matches!(fact, SourceFact::ExplicitType { target, declared_type, .. }
                if target == "n" && declared_type.trim_start_matches(':').trim() == expected_type)
            }),
            "{language}: missing declared parameter type: {:?}",
            evidence.facts
        );
    }
    let dynamic = extract_evidence("JavaScript", "function check(n) { return n; }");
    assert!(!dynamic
        .facts
        .iter()
        .any(|fact| matches!(fact, SourceFact::ExplicitType { .. })));
}

#[test]
fn malformed_call_is_not_promoted_to_an_observed_call_fact() {
    let source = "fn broken() { helper( ; }";
    let evidence = extract_evidence("Rust", source);
    assert!(
        !evidence
            .facts
            .iter()
            .any(|fact| { matches!(fact, SourceFact::Call { callee, .. } if callee == "helper") }),
        "an error-affected invocation is not reliable syntax evidence: {:?}",
        evidence.facts
    );
}

#[test]
fn type_fact_span_covers_the_binding_as_well_as_its_annotation() {
    let source = "fn check(\n    count:\n        i32\n) {}";
    let evidence = extract_evidence("Rust", source);
    let fact = evidence
        .facts
        .iter()
        .find(|fact| matches!(fact, SourceFact::ExplicitType { target, .. } if target == "count"))
        .expect("typed parameter fact");
    let span = fact.span();
    assert_eq!(span.start_line, 2);
    assert_eq!(span.end_line, 3);
    assert!(source[span.start_byte..span.end_byte].contains("count"));
    assert!(source[span.start_byte..span.end_byte].contains("i32"));
}

#[test]
fn unresolved_assert_named_function_is_only_call_syntax() {
    let evidence = extract_evidence("Rust", "fn check() { assertTrue(); }");
    assert!(evidence
        .facts
        .iter()
        .any(|fact| { matches!(fact, SourceFact::Call { callee, .. } if callee == "assertTrue") }));
    assert!(
        !evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::Assertion { macro_or_fn, .. } if macro_or_fn == "assertTrue")
        }),
        "an unresolved function name is not assertion behavior evidence"
    );
}

#[test]
fn typescript_return_annotation_is_not_the_type_of_the_function_binding() {
    let evidence = extract_evidence(
        "TypeScript",
        "function check(n: number): boolean { return true; }",
    );
    assert!(
        !evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::ExplicitType { target, declared_type, .. }
            if target == "check" && declared_type == "boolean")
        }),
        "a return annotation does not mean the function binding has boolean type"
    );
}

#[test]
fn literal_tail_and_empty_commented_return_are_preserved() {
    let source = "fn check() -> bool { helper(); true }";
    let evidence = extract_evidence("Rust", source);
    assert!(
        evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::Return { span, is_tail_expression: true, .. }
            if &source[span.start_byte..span.end_byte] == "true")
        }),
        "the literal tail expression must be observed"
    );
    let empty = extract_evidence("Rust", "fn check() { return /* reason */; }");
    assert!(
        empty.facts.iter().any(|fact| {
            matches!(
                fact,
                SourceFact::Return {
                    has_value: false,
                    is_tail_expression: false,
                    ..
                }
            )
        }),
        "a comment is not a return value"
    );
}

#[test]
fn pointer_call_syntax_and_commented_argument_counts_are_exact() {
    for language in ["C", "C++"] {
        let evidence = extract_evidence(
            language,
            "void run(struct Context *ctx) { ctx->helper(1); }",
        );
        let call = evidence
            .facts
            .iter()
            .find(|fact| matches!(fact, SourceFact::Call { callee, .. } if callee == "helper"))
            .expect("pointer member call");
        assert!(
            call.display_summary().contains("ctx->helper(1)"),
            "{language}: {}",
            call.display_summary()
        );
    }
    let evidence = extract_evidence("Rust", "fn run() { helper(/* reason */ 1); }");
    assert!(
        evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::Call { callee, argument_count: Some(1), .. } if callee == "helper")
        }),
        "a comment is not an argument: {:?}",
        evidence.facts
    );
}

#[test]
fn declaration_evidence_preserves_pointer_reference_and_array_syntax() {
    for (language, declaration) in [
        ("C", "int *p"),
        ("C++", "const Item& value"),
        ("C", "int values[4]"),
    ] {
        let evidence = extract_evidence(language, &format!("void check({declaration}) {{}}"));
        assert!(
            evidence.facts.iter().any(|fact| {
                matches!(fact, SourceFact::ExplicitType { .. })
                    && fact.display_summary().contains(declaration)
            }),
            "{language}: declaration evidence must preserve modifiers: {:?}",
            evidence.facts
        );
    }
}

#[test]
fn typescript_and_jsx_variants_preserve_valid_return_syntax() {
    let source = "function check(value: unknown) { return <number>value; }";
    let evidence = extract_evidence("TypeScript", source);
    assert!(
        evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::Return { span, .. }
            if &source[span.start_byte..span.end_byte] == "return <number>value;")
        }),
        "valid TypeScript assertion syntax must not be treated as broken TSX"
    );
    for language in ["JavaScript", "TypeScript"] {
        let jsx = extract_evidence(
            language,
            "function Component() { if (count >= 1) { return <div/>; } return null; }",
        );
        assert!(
            jsx.facts.iter().any(
                |fact| matches!(fact, SourceFact::Comparison { operator, .. } if operator == ">=")
            ),
            "{language} JSX"
        );
        assert!(
            jsx.facts
                .iter()
                .any(|fact| matches!(fact, SourceFact::Return { .. })),
            "{language} JSX"
        );
    }
}

#[test]
fn python_generator_expression_is_one_syntactic_argument() {
    for expression in ["sum(x for x in xs)", "sum(x for x in xs if x >= 1)"] {
        let source = format!("def total(xs):\n    return {expression}\n");
        let evidence = extract_evidence("Python", &source);
        assert!(evidence.facts.iter().any(|fact| {
            matches!(fact, SourceFact::Call { callee, argument_count: Some(1), .. } if callee == "sum")
        }), "generator expression is one argument: {:?}", evidence.facts);
    }
}

#[test]
fn tagged_templates_do_not_claim_an_ordinary_argument_list() {
    for language in ["JavaScript", "TypeScript"] {
        let evidence = extract_evidence(language, "function check() { tag`a${value}b`; }");
        let call = evidence
            .facts
            .iter()
            .find(|fact| matches!(fact, SourceFact::Call { callee, .. } if callee == "tag"))
            .expect("tagged invocation syntax");
        assert!(!call.display_summary().contains("syntactic arguments"));
    }
}

#[test]
fn macro_evidence_preserves_invocation_punctuation_and_delimiters() {
    for invocation in ["assert_eq!(left, right)", "assert![ready]"] {
        let evidence = extract_evidence("Rust", &format!("fn check() {{ {invocation}; }}"));
        let fact = evidence
            .facts
            .iter()
            .find(|fact| matches!(fact, SourceFact::Assertion { .. }))
            .expect("macro syntax fact");
        assert!(
            fact.display_summary().contains(invocation),
            "macro syntax changed: {}",
            fact.display_summary()
        );
    }
}
