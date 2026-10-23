//! Deterministic Tree-sitter syntactic source facts.
//!
//! Extracts bounded, typed syntactic evidence from source code with exact byte
//! and line provenance, without inferring unseen helper behaviors or runtime semantics.

use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use tree_sitter::{Node, Parser};

pub const EVIDENCE_VERSION: &str = "source-evidence-v1";
pub const MAX_FACTS: usize = 32;
pub const MAX_FACT_TEXT_BYTES: usize = 200;
pub const MAX_TOTAL_TEXT_BYTES: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceFact {
    Comparison {
        span: SourceSpan,
        operator: String,
        left: String,
        right: String,
    },
    Call {
        span: SourceSpan,
        syntax: String,
        callee: String,
        receiver: Option<String>,
        argument_count: Option<usize>,
    },
    Assertion {
        span: SourceSpan,
        syntax: String,
        macro_or_fn: String,
        arguments: String,
    },
    ExplicitType {
        span: SourceSpan,
        declaration: String,
        /// Raw binding/declarator syntax, not a resolved symbol.
        target: String,
        /// Raw type-node syntax; pointer/reference modifiers may be in the declarator.
        declared_type: String,
    },
    Return {
        span: SourceSpan,
        syntax: String,
        has_value: bool,
        is_tail_expression: bool,
    },
}

impl SourceFact {
    pub fn span(&self) -> &SourceSpan {
        match self {
            SourceFact::Comparison { span, .. } => span,
            SourceFact::Call { span, .. } => span,
            SourceFact::Assertion { span, .. } => span,
            SourceFact::ExplicitType { span, .. } => span,
            SourceFact::Return { span, .. } => span,
        }
    }

    pub fn display_summary(&self) -> String {
        match self {
            SourceFact::Comparison {
                operator,
                left,
                right,
                span,
            } => {
                format!(
                    "Comparison: {left} {operator} {right} (lines {}-{})",
                    span.start_line, span.end_line
                )
            }
            SourceFact::Call {
                syntax,
                argument_count,
                span,
                ..
            } => {
                let arguments = argument_count.map_or_else(
                    || "argument arity not established".to_string(),
                    |count| format!("{count} syntactic arguments"),
                );
                format!(
                    "Call: {syntax} [{arguments}; callee behavior/type unresolved] (lines {}-{})",
                    span.start_line, span.end_line
                )
            }
            SourceFact::Assertion { syntax, span, .. } => {
                format!(
                    "Macro invocation or assertion statement: {syntax} (lines {}-{})",
                    span.start_line, span.end_line
                )
            }
            SourceFact::ExplicitType {
                declaration, span, ..
            } => {
                format!(
                    "Declaration syntax: {declaration} (lines {}-{})",
                    span.start_line, span.end_line
                )
            }
            SourceFact::Return {
                syntax,
                is_tail_expression,
                span,
                ..
            } => {
                let kind = if *is_tail_expression {
                    "TailExpression"
                } else {
                    "Return"
                };
                format!(
                    "{kind}: {syntax} (lines {}-{})",
                    span.start_line, span.end_line
                )
            }
        }
    }

    fn order_key(&self) -> (usize, usize, usize) {
        let span = self.span();
        let kind_index = match self {
            SourceFact::Assertion { .. } => 0,
            SourceFact::Comparison { .. } => 1,
            SourceFact::ExplicitType { .. } => 2,
            SourceFact::Return { .. } => 3,
            SourceFact::Call { .. } => 4,
        };
        (span.start_byte, span.end_byte, kind_index)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceEvidence {
    pub version: String,
    pub language: String,
    pub source_hash: String,
    pub facts: Vec<SourceFact>,
}

impl Default for SourceEvidence {
    fn default() -> Self {
        Self {
            version: EVIDENCE_VERSION.into(),
            language: "unknown".into(),
            source_hash: String::new(),
            facts: Vec::new(),
        }
    }
}

impl SourceEvidence {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    pub fn matches_source(&self, source: &str) -> bool {
        let expected = format!("{:x}", Sha256::digest(source.as_bytes()));
        self.source_hash == expected
    }

    #[allow(dead_code)]
    pub fn facts_in_range(&self, start_line: usize, end_line: usize) -> Vec<&SourceFact> {
        self.facts
            .iter()
            .filter(|f| {
                let s = f.span();
                s.start_line >= start_line && s.end_line <= end_line
            })
            .collect()
    }
}

fn tree_sitter_language_for(language_name: &str) -> Option<tree_sitter::Language> {
    match language_name.to_lowercase().as_str() {
        "rust" | "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" | "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "c#" | "cs" | "csharp" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "typescript" | "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" | "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "c++" | "cpp" | "cxx" | "cc" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        _ => None,
    }
}

struct WrappedSource {
    full_text: String,
    byte_offset: usize,
    line_offset: usize,
}

fn wrap_source_if_needed(language_name: &str, source: &str) -> Option<WrappedSource> {
    match language_name.to_lowercase().as_str() {
        "java" | "c#" | "cs" | "csharp" => {
            let prefix = "class _WrapUnitScope {\n";
            let suffix = "\n}";
            let full_text = format!("{prefix}{source}{suffix}");
            Some(WrappedSource {
                full_text,
                byte_offset: prefix.len(),
                line_offset: 1,
            })
        }
        "go" => {
            let prefix = "package _wrap\n";
            let full_text = format!("{prefix}{source}");
            Some(WrappedSource {
                full_text,
                byte_offset: prefix.len(),
                line_offset: 1,
            })
        }
        _ => None,
    }
}

fn make_span(
    source: &str,
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
) -> Option<SourceSpan> {
    if start_byte >= end_byte || end_byte > source.len() {
        return None;
    }
    if !source.is_char_boundary(start_byte) || !source.is_char_boundary(end_byte) {
        return None;
    }
    Some(SourceSpan {
        start_byte,
        end_byte,
        start_line,
        end_line,
    })
}

pub fn extract_evidence(language: &str, source: &str) -> SourceEvidence {
    let source_hash = format!("{:x}", Sha256::digest(source.as_bytes()));
    let Some(ts_lang) = tree_sitter_language_for(language) else {
        return SourceEvidence {
            version: EVIDENCE_VERSION.into(),
            language: language.into(),
            source_hash,
            facts: Vec::new(),
        };
    };

    let mut parser = Parser::new();
    if parser.set_language(&ts_lang).is_err() {
        return SourceEvidence {
            version: EVIDENCE_VERSION.into(),
            language: language.into(),
            source_hash,
            facts: Vec::new(),
        };
    }

    let mut parsed_tree = parser.parse(source, None);
    if matches!(language.to_lowercase().as_str(), "typescript" | "ts")
        && parsed_tree
            .as_ref()
            .is_some_and(|tree| tree.root_node().has_error())
        && parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .is_ok()
    {
        if let Some(jsx_tree) = parser.parse(source, None) {
            if !jsx_tree.root_node().has_error() {
                parsed_tree = Some(jsx_tree);
            }
        }
    }
    let mut facts = Vec::new();

    if let Some(tree) = &parsed_tree {
        let root = tree.root_node();
        let use_wrapper = root.has_error()
            && matches!(
                language.to_lowercase().as_str(),
                "java" | "c#" | "cs" | "csharp" | "go"
            );

        if use_wrapper {
            if let Some(wrapped) = wrap_source_if_needed(language, source) {
                if let Some(w_tree) = parser.parse(&wrapped.full_text, None) {
                    walk_and_collect(
                        w_tree.root_node(),
                        wrapped.full_text.as_bytes(),
                        source,
                        wrapped.byte_offset,
                        wrapped.line_offset,
                        language,
                        &mut facts,
                    );
                }
            }
        } else {
            walk_and_collect(root, source.as_bytes(), source, 0, 0, language, &mut facts);
        }
    }

    // Sort deterministically
    facts.sort_by(|a, b| {
        let (a_start, a_end, a_k) = a.order_key();
        let (b_start, b_end, b_k) = b.order_key();
        match a_start.cmp(&b_start) {
            Ordering::Equal => match a_end.cmp(&b_end) {
                Ordering::Equal => a_k.cmp(&b_k),
                other => other,
            },
            other => other,
        }
    });

    // Deduplicate identical facts
    facts.dedup_by(|a, b| a == b);

    // Enforce budgets: MAX_FACTS and MAX_TOTAL_TEXT_BYTES
    let mut bounded_facts = Vec::new();
    let mut total_bytes = 0;

    for fact in facts {
        if bounded_facts.len() >= MAX_FACTS {
            break;
        }
        let summary = fact.display_summary();
        if summary.len() > MAX_FACT_TEXT_BYTES {
            continue;
        }
        if total_bytes + summary.len() > MAX_TOTAL_TEXT_BYTES {
            break;
        }
        total_bytes += summary.len();
        bounded_facts.push(fact);
    }

    SourceEvidence {
        version: EVIDENCE_VERSION.into(),
        language: language.into(),
        source_hash,
        facts: bounded_facts,
    }
}

fn walk_and_collect(
    node: Node,
    bytes: &[u8],
    original_source: &str,
    byte_offset: usize,
    line_offset: usize,
    language: &str,
    out: &mut Vec<SourceFact>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }

    // Never extract synthetic wrapper facts (e.g. from _WrapUnitScope)
    let is_wrapper_decl = node.kind().contains("class_declaration")
        && node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            == Some("_WrapUnitScope");

    if !is_wrapper_decl && !node.has_error() {
        extract_facts_from_node(
            node,
            bytes,
            original_source,
            byte_offset,
            line_offset,
            language,
            out,
        );
    }

    for child in node.children(&mut node.walk()) {
        walk_and_collect(
            child,
            bytes,
            original_source,
            byte_offset,
            line_offset,
            language,
            out,
        );
    }
}

fn count_arguments(node: Node<'_>) -> Option<usize> {
    match node.kind() {
        "generator_expression" => Some(1),
        "arguments" | "argument_list" => Some(
            node.named_children(&mut node.walk())
                .filter(|child| !child.kind().contains("comment"))
                .count(),
        ),
        // Tagged templates are invocation syntax, not an ordinary argument list.
        _ => None,
    }
}

fn map_node_span(
    node: Node,
    original_source: &str,
    byte_offset: usize,
    _line_offset: usize,
) -> Option<SourceSpan> {
    let start_b = node.start_byte();
    let end_b = node.end_byte();
    if start_b < byte_offset || end_b < byte_offset {
        return None;
    }
    let mapped_start = start_b - byte_offset;
    let mapped_end = end_b - byte_offset;
    if mapped_end > original_source.len() || mapped_start >= mapped_end {
        return None;
    }
    if !original_source.is_char_boundary(mapped_start)
        || !original_source.is_char_boundary(mapped_end)
    {
        return None;
    }

    let mapped_start_line = original_source[..mapped_start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let mapped_end_line = original_source[..mapped_end]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;

    make_span(
        original_source,
        mapped_start,
        mapped_end,
        mapped_start_line,
        mapped_end_line,
    )
}

fn extract_facts_from_node(
    node: Node,
    bytes: &[u8],
    original_source: &str,
    byte_offset: usize,
    line_offset: usize,
    language: &str,
    out: &mut Vec<SourceFact>,
) {
    let kind = node.kind();

    // 1. Comparisons
    let is_comparison = match kind {
        "binary_expression" => {
            if let Some(op_node) = node.child_by_field_name("operator") {
                let op = op_node.utf8_text(bytes).unwrap_or("");
                matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "!==")
            } else {
                let mut found_op = false;
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        let text = child.utf8_text(bytes).unwrap_or("");
                        if matches!(text, "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "!==") {
                            found_op = true;
                            break;
                        }
                    }
                }
                found_op
            }
        }
        "comparison_operator" => true, // Python
        _ => false,
    };

    if is_comparison {
        if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
            let left_node = node.child_by_field_name("left").or_else(|| node.child(0));
            let right_node = node.child_by_field_name("right").or_else(|| node.child(2));
            let op_node = node.child_by_field_name("operator").or_else(|| {
                (0..node.child_count())
                    .map(|i| node.child(i).unwrap())
                    .find(|c| {
                        matches!(
                            c.utf8_text(bytes).unwrap_or(""),
                            "==" | "!=" | "<" | "<=" | ">" | ">=" | "===" | "!=="
                        )
                    })
            });

            if let (Some(left_n), Some(op_n), Some(right_n)) = (left_node, op_node, right_node) {
                if let (Ok(left_str), Ok(op_str), Ok(right_str)) = (
                    left_n.utf8_text(bytes),
                    op_n.utf8_text(bytes),
                    right_n.utf8_text(bytes),
                ) {
                    let left = left_str.trim().to_string();
                    let right = right_str.trim().to_string();
                    let operator = op_str.to_string();
                    if !left.is_empty()
                        && !right.is_empty()
                        && left.len() <= 60
                        && right.len() <= 60
                    {
                        out.push(SourceFact::Comparison {
                            span,
                            operator,
                            left,
                            right,
                        });
                    }
                }
            }
        }
    }

    // 2. Syntactic assertions
    // Rust standard assertion macros: assert!, assert_eq!, assert_ne!, debug_assert!, debug_assert_eq!, debug_assert_ne!
    if kind == "macro_invocation" {
        if let Some(macro_node) = node.child_by_field_name("macro") {
            if let Ok(macro_name) = macro_node.utf8_text(bytes) {
                if matches!(
                    macro_name,
                    "assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "debug_assert"
                        | "debug_assert_eq"
                        | "debug_assert_ne"
                ) {
                    if let Some(span) =
                        map_node_span(node, original_source, byte_offset, line_offset)
                    {
                        // Select actual token_tree child
                        let token_tree = (0..node.child_count())
                            .map(|i| node.child(i).unwrap())
                            .find(|c| c.kind() == "token_tree");

                        if let Some(tt) = token_tree {
                            if let Ok(tt_text) = tt.utf8_text(bytes) {
                                let trimmed = tt_text.trim();
                                let clean = if trimmed.starts_with('(') && trimmed.ends_with(')') {
                                    &trimmed[1..trimmed.len() - 1]
                                } else if trimmed.starts_with('{') && trimmed.ends_with('}') {
                                    &trimmed[1..trimmed.len() - 1]
                                } else {
                                    trimmed
                                }
                                .trim();

                                if clean.len() <= 100 {
                                    out.push(SourceFact::Assertion {
                                        syntax: original_source[span.start_byte..span.end_byte]
                                            .to_string(),
                                        span,
                                        macro_or_fn: macro_name.to_string(),
                                        arguments: clean.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Python assert statement
    if kind == "assert_statement" {
        if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
            if let Ok(text) = node.utf8_text(bytes) {
                let trimmed = text.trim();
                let args = trimmed.strip_prefix("assert").unwrap_or(trimmed).trim();
                if args.len() <= 100 {
                    out.push(SourceFact::Assertion {
                        syntax: original_source[span.start_byte..span.end_byte].to_string(),
                        span,
                        macro_or_fn: "assert".into(),
                        arguments: args.to_string(),
                    });
                }
            }
        }
    }

    // 3. Calls across all supported grammars
    let is_call_node = matches!(
        kind,
        "call_expression"
            | "method_call_expression"
            | "invocation_expression"
            | "call"
            | "method_invocation"
    );

    if is_call_node {
        if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
            let mut callee = String::new();
            let mut receiver = None;
            let mut argument_count = None;

            if kind == "method_invocation" {
                // Java method_invocation: object.name(arguments)
                if let Some(name_n) = node.child_by_field_name("name") {
                    if let Ok(s) = name_n.utf8_text(bytes) {
                        callee = s.to_string();
                    }
                }
                if let Some(obj_n) = node.child_by_field_name("object") {
                    if let Ok(s) = obj_n.utf8_text(bytes) {
                        receiver = Some(s.trim().to_string());
                    }
                }
                if let Some(args_n) = node.child_by_field_name("arguments") {
                    argument_count = count_arguments(args_n);
                }
            } else if kind == "method_call_expression" {
                // Rust method call: obj.method(args)
                if let Some(name_n) = node.child_by_field_name("name") {
                    if let Ok(s) = name_n.utf8_text(bytes) {
                        callee = s.to_string();
                    }
                }
                if let Some(rec_n) = node.child_by_field_name("receiver") {
                    if let Ok(s) = rec_n.utf8_text(bytes) {
                        receiver = Some(s.trim().to_string());
                    }
                }
                if let Some(args_n) = node.child_by_field_name("arguments") {
                    argument_count = count_arguments(args_n);
                }
            } else if kind == "invocation_expression" {
                // C# invocation: expression(args)
                if let Some(fn_n) = node
                    .child_by_field_name("function")
                    .or_else(|| node.child(0))
                {
                    if fn_n.kind() == "member_access_expression" {
                        if let Some(name_n) = fn_n.child_by_field_name("name") {
                            if let Ok(s) = name_n.utf8_text(bytes) {
                                callee = s.to_string();
                            }
                        }
                        if let Some(rec_n) = fn_n.child_by_field_name("expression") {
                            if let Ok(s) = rec_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if let Ok(s) = fn_n.utf8_text(bytes) {
                        callee = s.trim().to_string();
                    }
                }
                if let Some(args_n) = node.child_by_field_name("arguments") {
                    argument_count = count_arguments(args_n);
                }
            } else if kind == "call" {
                // Python call: function(args)
                if let Some(fn_n) = node.child_by_field_name("function") {
                    if fn_n.kind() == "attribute" {
                        if let Some(attr_n) = fn_n.child_by_field_name("attribute") {
                            if let Ok(s) = attr_n.utf8_text(bytes) {
                                callee = s.to_string();
                            }
                        }
                        if let Some(obj_n) = fn_n.child_by_field_name("object") {
                            if let Ok(s) = obj_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if let Ok(s) = fn_n.utf8_text(bytes) {
                        callee = s.trim().to_string();
                    }
                }
                if let Some(args_n) = node.child_by_field_name("arguments") {
                    argument_count = count_arguments(args_n);
                }
            } else {
                // Java method_invocation / C++ / Go / TS / JS / C call_expression
                let fn_node = node
                    .child_by_field_name("function")
                    .or_else(|| node.child_by_field_name("callee"))
                    .or_else(|| node.child(0));

                if let Some(fn_n) = fn_node {
                    let fn_kind = fn_n.kind();
                    if fn_kind == "member_expression" {
                        // TS/JS: obj.method
                        if let Some(prop_n) = fn_n.child_by_field_name("property") {
                            if let Ok(s) = prop_n.utf8_text(bytes) {
                                callee = s.to_string();
                            }
                        }
                        if let Some(obj_n) = fn_n.child_by_field_name("object") {
                            if let Ok(s) = obj_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if fn_kind == "selector_expression" {
                        // Go: operand.field
                        if let Some(f_n) = fn_n.child_by_field_name("field") {
                            if let Ok(s) = f_n.utf8_text(bytes) {
                                callee = s.to_string();
                            }
                        }
                        if let Some(op_n) = fn_n.child_by_field_name("operand") {
                            if let Ok(s) = op_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if fn_kind == "field_expression" {
                        // Rust: value.field; C++: argument.field
                        if let Some(f_n) = fn_n
                            .child_by_field_name("field")
                            .or_else(|| fn_n.child_by_field_name("property"))
                        {
                            if let Ok(s) = f_n.utf8_text(bytes) {
                                callee = s.to_string();
                            }
                        }
                        if let Some(arg_n) = fn_n
                            .child_by_field_name("value")
                            .or_else(|| fn_n.child_by_field_name("argument"))
                        {
                            if let Ok(s) = arg_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if let Some(name_n) = node.child_by_field_name("name") {
                        // Java method_invocation: object.name(args)
                        if let Ok(s) = name_n.utf8_text(bytes) {
                            callee = s.to_string();
                        }
                        if let Some(obj_n) = node.child_by_field_name("object") {
                            if let Ok(s) = obj_n.utf8_text(bytes) {
                                receiver = Some(s.trim().to_string());
                            }
                        }
                    } else if let Ok(s) = fn_n.utf8_text(bytes) {
                        callee = s.trim().to_string();
                    }
                }

                if let Some(args_n) = node.child_by_field_name("arguments") {
                    argument_count = count_arguments(args_n);
                }
            }

            if !callee.is_empty() && callee.len() <= 60 {
                // Names such as assertTrue do not resolve an assertion API.
                out.push(SourceFact::Call {
                    syntax: original_source[span.start_byte..span.end_byte].to_string(),
                    span,
                    callee,
                    receiver,
                    argument_count,
                });
            }
        }
    }

    // 4. Explicit types: parameter and variable declarations with direct type annotations
    if kind == "parameter"
        || kind == "formal_parameter"
        || kind == "parameter_declaration"
        || kind == "typed_parameter"
        || kind == "required_parameter"
        || kind == "optional_parameter"
    {
        if let Some(type_n) = node.child_by_field_name("type") {
            if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
                let param_str = node
                    .child_by_field_name("name")
                    .or_else(|| node.child_by_field_name("pattern"))
                    .or_else(|| node.child_by_field_name("declarator"))
                    .or_else(|| node.child(0))
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .unwrap_or("param");
                if let Ok(type_str) = type_n.utf8_text(bytes) {
                    let declared_type = type_str.trim_start_matches(':').trim().to_string();
                    let target = param_str.trim().to_string();
                    if !declared_type.is_empty() && declared_type.len() <= 50 {
                        out.push(SourceFact::ExplicitType {
                            declaration: original_source[span.start_byte..span.end_byte]
                                .to_string(),
                            span,
                            target,
                            declared_type,
                        });
                    }
                }
            }
        }
    } else if kind == "let_declaration"
        || kind == "local_variable_declaration"
        || kind == "variable_declaration"
    {
        if let Some(type_n) = node.child_by_field_name("type") {
            if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
                if let Ok(type_str) = type_n.utf8_text(bytes) {
                    let declared_type = type_str.trim().to_string();
                    let target = node
                        .child_by_field_name("pattern")
                        .or_else(|| node.child_by_field_name("name"))
                        .or_else(|| node.child_by_field_name("declarator"))
                        .and_then(|n| n.utf8_text(bytes).ok())
                        .unwrap_or("binding")
                        .trim()
                        .to_string();
                    if !declared_type.is_empty() && declared_type.len() <= 50 {
                        out.push(SourceFact::ExplicitType {
                            declaration: original_source[span.start_byte..span.end_byte]
                                .to_string(),
                            span,
                            target,
                            declared_type,
                        });
                    }
                }
            }
        }
    } else if kind == "type_annotation"
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == "variable_declarator")
    {
        // A function's return annotation is not the type of its function binding.
        let declaration = node.parent().expect("checked variable declaration");
        if let Some(span) = map_node_span(declaration, original_source, byte_offset, line_offset) {
            if let Ok(type_str) = node.utf8_text(bytes) {
                let clean_type = type_str.trim_start_matches(':').trim().to_string();
                let target = node
                    .parent()
                    .and_then(|p| {
                        p.child_by_field_name("name")
                            .or_else(|| p.child_by_field_name("pattern"))
                    })
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .unwrap_or("binding")
                    .trim()
                    .to_string();
                if !clean_type.is_empty() && clean_type.len() <= 50 {
                    out.push(SourceFact::ExplicitType {
                        declaration: original_source[span.start_byte..span.end_byte].to_string(),
                        span,
                        target,
                        declared_type: clean_type,
                    });
                }
            }
        }
    }

    // 5. Returns and tail expressions
    if kind == "return_statement" || kind == "return_expression" {
        if let Some(span) = map_node_span(node, original_source, byte_offset, line_offset) {
            let has_value = (0..node.child_count())
                .map(|i| node.child(i).unwrap())
                .any(|c| c.is_named() && !c.kind().contains("comment"));
            let syntax = original_source[span.start_byte..span.end_byte].to_string();
            out.push(SourceFact::Return {
                span,
                syntax,
                has_value,
                is_tail_expression: false,
            });
        }
    }

    // Rust tail expressions: expression at the end of a block without trailing semicolon
    if language == "Rust" && kind == "block" {
        let child_count = node.child_count();
        if child_count >= 2 {
            // Check before closing brace '}'
            let mut last_expr = None;
            for i in (0..child_count).rev() {
                if let Some(c) = node.child(i) {
                    let c_kind = c.kind();
                    if c_kind == "}" || c_kind == ";" {
                        continue;
                    }
                    if c_kind.ends_with("_expression")
                        || c_kind == "identifier"
                        || c_kind == "binary_expression"
                        || c_kind.ends_with("_literal")
                    {
                        last_expr = Some(c);
                    }
                    break;
                }
            }

            if let Some(tail) = last_expr {
                if tail.kind() != "return_expression" {
                    if let Some(span) =
                        map_node_span(tail, original_source, byte_offset, line_offset)
                    {
                        out.push(SourceFact::Return {
                            syntax: original_source[span.start_byte..span.end_byte].to_string(),
                            span,
                            has_value: true,
                            is_tail_expression: true,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_comparison_with_exact_operator_and_spans() {
        let source = "fn check(x: i32) -> bool {\n    if x >= 10 {\n        return true;\n    }\n    false\n}";
        let evidence = extract_evidence("Rust", source);
        let comparison = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Comparison { .. }))
            .expect("found comparison");
        if let SourceFact::Comparison {
            operator,
            left,
            right,
            span,
        } = comparison
        {
            assert_eq!(operator, ">=");
            assert_eq!(left, "x");
            assert_eq!(right, "10");
            assert_eq!(span.start_line, 2);
            assert_eq!(span.end_line, 2);
            assert_eq!(&source[span.start_byte..span.end_byte], "x >= 10");
        }
    }

    #[test]
    fn distinguishes_greater_from_greater_or_equal() {
        let source = "fn compare(a: usize, b: usize) -> bool {\n    a > b\n}";
        let evidence = extract_evidence("Rust", source);
        let comparison = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Comparison { .. }))
            .expect("found comparison");
        if let SourceFact::Comparison { operator, .. } = comparison {
            assert_eq!(operator, ">");
        }
    }

    #[test]
    fn extracts_macro_assertions_and_preserves_invocation_syntax() {
        let source = "fn test_it() {\n    assert!(ready);\n    assert_eq!(left, right);\n}";
        let evidence = extract_evidence("Rust", source);
        let asserts: Vec<_> = evidence
            .facts
            .iter()
            .filter(|f| matches!(f, SourceFact::Assertion { .. }))
            .collect();
        assert_eq!(
            asserts.len(),
            2,
            "adjacent asserts must be preserved as separate facts"
        );
        if let SourceFact::Assertion {
            macro_or_fn,
            arguments,
            ..
        } = &asserts[0]
        {
            assert_eq!(macro_or_fn, "assert");
            assert_eq!(arguments, "ready");
        }
        if let SourceFact::Assertion {
            macro_or_fn,
            arguments,
            ..
        } = &asserts[1]
        {
            assert_eq!(macro_or_fn, "assert_eq");
            assert_eq!(arguments, "left, right");
        }
    }

    #[test]
    fn distinguishes_free_vs_member_call_syntax() {
        let source = "fn run() {\n    calc(1);\n    obj.perform(2);\n}";
        let evidence = extract_evidence("Rust", source);
        let calls: Vec<_> = evidence
            .facts
            .iter()
            .filter(|f| matches!(f, SourceFact::Call { .. }))
            .collect();
        assert_eq!(calls.len(), 2);
        if let SourceFact::Call {
            callee,
            receiver,
            argument_count,
            ..
        } = &calls[0]
        {
            assert_eq!(callee, "calc");
            assert_eq!(*receiver, None);
            assert_eq!(*argument_count, Some(1));
        }
        if let SourceFact::Call {
            callee,
            receiver,
            argument_count,
            ..
        } = &calls[1]
        {
            assert_eq!(callee, "perform");
            assert_eq!(*receiver, Some("obj".into()));
            assert_eq!(*argument_count, Some(1));
        }
    }

    #[test]
    fn extracts_isolated_java_method_without_enclosing_class() {
        let source = "public int compute(int limit) {\n    if (limit >= 5) {\n        return check(limit);\n    }\n    return 0;\n}";
        let evidence = extract_evidence("Java", source);
        assert!(
            !evidence.facts.is_empty(),
            "isolated Java method must yield facts via mapped wrapper"
        );
        let cmp = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Comparison { .. }));
        assert!(cmp.is_some());
        if let Some(SourceFact::Comparison { operator, span, .. }) = cmp {
            assert_eq!(operator, ">=");
            assert_eq!(&source[span.start_byte..span.end_byte], "limit >= 5");
            assert_eq!(span.start_line, 2);
        }
        let call = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Call { callee, .. } if callee == "check"));
        assert!(call.is_some(), "Java method call must be extracted");
    }

    #[test]
    fn extracts_isolated_csharp_method_without_enclosing_class() {
        let source = "public bool Verify(int score) {\n    if (score >= 100) {\n        return true;\n    }\n    return false;\n}";
        let evidence = extract_evidence("C#", source);
        assert!(
            !evidence.facts.is_empty(),
            "isolated C# method must yield facts via mapped wrapper"
        );
        let cmp = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Comparison { .. }));
        assert!(cmp.is_some());
    }

    #[test]
    fn extracts_facts_for_all_nine_supported_languages() {
        let cases = [
            (
                "Rust",
                "fn calc(v: i32) -> i32 {\n    if v >= 10 {\n        return run(v);\n    }\n    0\n}",
            ),
            (
                "Python",
                "def verify(x):\n    if x >= 0:\n        return calculate(x)\n",
            ),
            (
                "Go",
                "func Process(v int) int {\n    if v >= 10 {\n        return calculate(v)\n    }\n    return 0\n}",
            ),
            (
                "Java",
                "public int calculate(int x) {\n    if (x >= 10) {\n        return compute(x);\n    }\n    return 0;\n}",
            ),
            (
                "C#",
                "public int Calculate(int x) {\n    if (x >= 10) {\n        return Compute(x);\n    }\n    return 0;\n}",
            ),
            (
                "TypeScript",
                "function check(n: number): boolean {\n    if (n >= 42) {\n        return true;\n    }\n    return false;\n}",
            ),
            (
                "JavaScript",
                "function test(a) {\n    if (a >= 1) {\n        return doWork(a);\n    }\n}",
            ),
            (
                "C++",
                "int calculate(int x) {\n    if (x >= 10) {\n        return compute(x);\n    }\n    return 0;\n}",
            ),
            (
                "C",
                "int calculate(int x) {\n    if (x >= 10) {\n        return compute(x);\n    }\n    return 0;\n}",
            ),
        ];

        for (lang, code) in cases {
            let ev = extract_evidence(lang, code);
            assert!(!ev.facts.is_empty(), "expected facts for language {lang}");
            let has_cmp = ev
                .facts
                .iter()
                .any(|f| matches!(f, SourceFact::Comparison { operator, .. } if operator == ">="));
            assert!(has_cmp, "expected >= comparison fact for language {lang}");
            let has_return = ev
                .facts
                .iter()
                .any(|f| matches!(f, SourceFact::Return { .. }));
            assert!(has_return, "expected return fact for language {lang}");
        }
    }

    #[test]
    fn tsx_jsx_syntax_extracts_cleanly() {
        let source = "export const Component = ({ count }: { count: number }) => {\n    if (count >= 10) {\n        return <div>Max</div>;\n    }\n    return <div>{count}</div>;\n};";
        let evidence = extract_evidence("TypeScript", source);
        assert!(!evidence.facts.is_empty(), "TSX must be supported");
        let cmp = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::Comparison { .. }));
        assert!(cmp.is_some());
    }

    #[test]
    fn tail_expressions_and_returns_are_identified() {
        let source =
            "fn compute(x: i32) -> i32 {\n    if x < 0 {\n        return 0;\n    }\n    x + 1\n}";
        let evidence = extract_evidence("Rust", source);
        let ret = evidence.facts.iter().find(|f| {
            matches!(
                f,
                SourceFact::Return {
                    is_tail_expression: false,
                    ..
                }
            )
        });
        assert!(ret.is_some(), "found return statement");
        let tail = evidence.facts.iter().find(|f| {
            matches!(
                f,
                SourceFact::Return {
                    is_tail_expression: true,
                    ..
                }
            )
        });
        assert!(tail.is_some(), "found tail expression");
    }

    #[test]
    fn explicit_vs_inferred_types() {
        let source = "fn types() {\n    let explicit: i32 = 42;\n    let inferred = 100;\n}";
        let evidence = extract_evidence("Rust", source);
        let explicit = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::ExplicitType { target, .. } if target == "explicit"));
        assert!(explicit.is_some(), "explicit type should be extracted");
        let inferred = evidence
            .facts
            .iter()
            .find(|f| matches!(f, SourceFact::ExplicitType { target, .. } if target == "inferred"));
        assert!(
            inferred.is_none(),
            "inferred type should not produce an explicit type fact"
        );
    }

    #[test]
    fn unsupported_language_returns_empty_evidence() {
        let evidence = extract_evidence("Cobol", "IDENTIFICATION DIVISION.");
        assert_eq!(evidence.facts.len(), 0);
        assert_eq!(evidence.language, "Cobol");
    }

    #[test]
    fn malformed_source_safely_omits_affected_facts() {
        let evidence = extract_evidence("Rust", "fn broken( { let x = ;");
        assert!(evidence.facts.len() <= MAX_FACTS);
    }

    #[test]
    fn budget_enforces_bounds_deterministically() {
        let mut source = String::from("fn many() {\n");
        for i in 0..100 {
            source.push_str(&format!(
                "    let v{i} = {i};\n    if v{i} >= {i} {{ call_{i}(); }}\n"
            ));
        }
        source.push_str("}\n");

        let evidence = extract_evidence("Rust", &source);
        assert!(evidence.facts.len() <= MAX_FACTS);
    }

    #[test]
    fn utf8_source_slicing_is_exact() {
        let source = "fn greeting() {\n    let emoji = \"🦀\";\n    if count >= 1 {\n        log(\"🔥\");\n    }\n}";
        let evidence = extract_evidence("Rust", source);
        for fact in &evidence.facts {
            let span = fact.span();
            let slice = &source[span.start_byte..span.end_byte];
            assert!(!slice.is_empty());
        }
    }
}
