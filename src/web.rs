use crate::change_set::{ChangeSet, FileChange, SemanticUnitChange, SemanticUnitChangeKind};
use crate::explain::AnalysisMode;
use crate::file_analysis::{unit_key, FileAnalysis};
use crate::language::LanguageRegistry;
use crate::model::{Annotation, UnitExplanation};

const STYLE: &str = r#"<style>:root{--page:#f7f8fa;--panel:#fff;--text:#18202a;--muted:#5c6875;--border:#d7dde5;--code:#f1f4f7;--accent:#185abc}body{font:1rem/1.55 system-ui,sans-serif;background:var(--page);color:var(--text);margin:auto;max-width:1100px;padding:2rem}section,article{background:var(--panel);border:1px solid var(--border);border-radius:.6rem;padding:1.25rem;margin:1.25rem 0}h1,h2,h3,h4{line-height:1.2}pre,textarea{font:.9rem/1.55 ui-monospace,Consolas,monospace;background:var(--code);white-space:pre;overflow:auto;padding:1rem;width:100%;box-sizing:border-box}button{padding:.45rem .7rem;margin:.25rem;border:1px solid var(--border);border-radius:.35rem;background:var(--panel);color:var(--text)}button[aria-selected=true]{border-color:var(--accent);box-shadow:inset 0 -2px var(--accent)}button:focus-visible{outline:3px solid var(--accent)}.layer-controls,.actions{margin:.75rem 0}.layer-summary{min-height:1.55rem}.annotation{border-left:4px solid var(--accent);margin:.25rem 0 1rem;padding:.5rem 1rem}@media(prefers-color-scheme:dark){:root{--page:#151a21;--panel:#1d242d;--text:#e8edf2;--muted:#aab6c2;--border:#3a4652;--code:#11171d;--accent:#8ab4f8}}</style>"#;

const SCRIPT: &str = include_str!("web.js");

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn page_start(title: &str) -> String {
    format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>{}</head><body><main>", escape(title), STYLE)
}

pub fn render(change_set: &ChangeSet) -> String {
    render_for_session_at_generation(change_set, "", change_set.generation.0)
}

pub fn render_file_analysis(analysis: &FileAnalysis) -> String {
    let mut html = page_start("git-explain files");
    html.push_str(&format!("<h1>Files</h1><p>Revision: {}</p><button id=\"toggle-explanations\">Hide explanations</button><p id=\"status\" role=\"status\" aria-live=\"polite\"></p>", escape(analysis.revision.as_deref().unwrap_or("working tree"))));
    for document in analysis
        .documents
        .iter()
        .filter(|document| document.language.is_some())
    {
        html.push_str(&format!(
            "<section><h2>{}</h2><p>Language: {}</p><p>Code sections: {}</p>",
            escape(&document.source.path.display().to_string()),
            escape(document.language.as_deref().unwrap_or("unsupported")),
            document.units.len()
        ));
        if document.units.is_empty() {
            html.push_str("<p>No explainable code sections were found in this file.</p>");
        }
        for unit in &document.units {
            let id = unit_key(unit);
            let base = format!("/api/file-units/{id}");
            html.push_str(&unit_card(UnitCard {
                id: &id,
                name: &unit.identity.qualified_name,
                metadata: &format!(
                    "{:?} · lines {}–{}",
                    unit.identity.kind, unit.location.start_line, unit.location.end_line
                ),
                source: &unit.source,
                overview: analysis.explanations.get(&id),
                changes: None,
                concepts: analysis.concept_explanations.get(&id),
                deep: analysis.deep_explanations.get(&id).map(String::as_str),
                base: &base,
                generation: None,
                diff: None,
            }));
        }
        html.push_str("</section>");
    }
    html.push_str(SCRIPT);
    html.push_str("</main></body></html>");
    html
}

pub fn render_for_session_at_generation(
    change_set: &ChangeSet,
    session_id: &str,
    generation: u64,
) -> String {
    let (heading, detail) = match &change_set.context.mode {
        AnalysisMode::WorkingTree => ("Working tree".into(), String::new()),
        AnalysisMode::Commit {
            oid,
            parent_oid,
            subject,
            ..
        } => (
            format!("Commit {}", &oid[..oid.len().min(12)]),
            format!(
                "<p>{}<br>Compared with parent: {}</p>",
                escape(subject),
                escape(parent_oid.as_deref().unwrap_or("<empty tree>"))
            ),
        ),
        AnalysisMode::Range {
            expression,
            from_oid,
            to_oid,
        } => (
            format!("Range {expression}"),
            format!(
                "<p>Compared {} with {}</p>",
                escape(&from_oid[..from_oid.len().min(12)]),
                escape(&to_oid[..to_oid.len().min(12)])
            ),
        ),
    };
    let endpoint = if session_id.is_empty() {
        "/api/units/".into()
    } else {
        format!("/api/sessions/{session_id}/units/")
    };
    let summary = change_set.summary();
    let supported_files = change_set
        .files
        .iter()
        .filter(|file| LanguageRegistry::language_for_path(file.display_path()).is_some())
        .collect::<Vec<_>>();
    let mut html = page_start("git-explain changes");
    html.push_str(&format!("<header><h1>Changes</h1><p>{}</p>{}</header><button id=\"toggle-explanations\">Hide explanations</button><p id=\"status\" role=\"status\" aria-live=\"polite\"></p><section><h2>Change summary</h2><p>{} supported source files changed.</p><p>Code sections — Added: {}; Modified: {}; Deleted: {}; Unchanged: {}</p></section>", escape(&heading), detail, supported_files.len(), summary.units_added, summary.units_modified, summary.units_deleted, summary.units_preserved));
    if let Some(message) = change_set.empty_state() {
        html.push_str(&format!("<p role=\"status\">{}</p>", escape(&message)))
    }
    html.push_str("<nav aria-labelledby=\"files\"><h2 id=\"files\">Files</h2><ol>");
    for (index, file) in supported_files.iter().enumerate() {
        html.push_str(&format!(
            "<li><a href=\"#file-{}\">{}</a> — {}</li>",
            index + 1,
            escape(&file.display_path().display().to_string()),
            file.status_label()
        ))
    }
    html.push_str("</ol></nav>");
    for (index, file) in supported_files.iter().enumerate() {
        render_file(&mut html, file, index + 1, &endpoint, generation)
    }
    if !session_id.is_empty() {
        html.push_str(&format!("<div id=\"session-state\" data-session=\"{}\" data-generation=\"{}\"></div><button id=\"reload-snapshot\" hidden onclick=\"location.reload()\">Reload Git changes</button>", escape(session_id), generation));
    }
    html.push_str(SCRIPT);
    html.push_str("</main></body></html>");
    html
}

fn render_file(
    html: &mut String,
    file: &FileChange,
    number: usize,
    endpoint: &str,
    generation: u64,
) {
    html.push_str(&format!(
        "<section id=\"file-{number}\"><h2>{}</h2><p>Status: {}</p>",
        escape(&file.display_path().display().to_string()),
        file.status_label()
    ));
    if let Some(old_path) = &file.old_path {
        if file
            .new_path
            .as_ref()
            .is_some_and(|new_path| new_path != old_path)
        {
            html.push_str(&format!(
                "<p>Previous path: {}</p>",
                escape(&old_path.display().to_string())
            ));
        }
    }
    if file.binary {
        html.push_str("<p>Binary file. A source-code explanation is not available.</p>")
    }
    let preserved = file
        .semantic_changes
        .iter()
        .filter(|c| c.kind == SemanticUnitChangeKind::Preserved)
        .count();
    if preserved > 0 {
        html.push_str(&format!("<p>Unchanged code sections: {preserved}</p>"))
    }
    for change in file
        .semantic_changes
        .iter()
        .filter(|c| c.kind.explainable())
    {
        render_change(html, change, endpoint, generation)
    }
    html.push_str("</section>")
}

fn render_change(html: &mut String, change: &SemanticUnitChange, endpoint: &str, generation: u64) {
    let unit = change.active_unit();
    let id = &change.id.0;
    let base = format!("{endpoint}{id}");
    let overview =
        (!change.overview_explanation.overview.is_empty()).then_some(&change.overview_explanation);
    let changes = (!change.explanation.overview.is_empty()).then_some(&change.explanation);
    let concepts =
        (!change.concept_explanation.overview.is_empty()).then_some(&change.concept_explanation);
    html.push_str(&unit_card(UnitCard {
        id,
        name: &unit.identity.qualified_name,
        metadata: &format!(
            "Status: {:?} · {} · lines {}–{}",
            change.kind, unit.language, unit.location.start_line, unit.location.end_line
        ),
        source: &unit.source,
        overview,
        changes,
        concepts,
        deep: change.deep_explanation.as_deref(),
        base: &base,
        generation: Some(generation),
        diff: Some(&change.diff),
    }))
}

struct UnitCard<'a> {
    id: &'a str,
    name: &'a str,
    metadata: &'a str,
    source: &'a str,
    overview: Option<&'a UnitExplanation>,
    changes: Option<&'a UnitExplanation>,
    concepts: Option<&'a UnitExplanation>,
    deep: Option<&'a str>,
    base: &'a str,
    generation: Option<u64>,
    diff: Option<&'a str>,
}

fn layer_button(
    kind: &str,
    label: &str,
    endpoint: &str,
    regenerate: &str,
    generation: &str,
    selected: bool,
) -> String {
    format!("<button role=\"tab\" aria-selected=\"{}\" data-layer=\"{}\" data-endpoint=\"{}\" data-regenerate-endpoint=\"{}\"{}>{}</button>",selected,kind,escape(endpoint),escape(regenerate),generation,label)
}

fn unit_card(card: UnitCard<'_>) -> String {
    let overview = card
        .overview
        .map(|value| value.overview.as_str())
        .unwrap_or("Explanation has not been generated.");
    let annotations = &[];
    let gen = card
        .generation
        .map(|v| format!(" data-generation=\"{v}\""))
        .unwrap_or_default();
    let overview_endpoint = if card.diff.is_some() {
        "overview"
    } else {
        "explain"
    };
    let overview_regenerate = if card.diff.is_some() {
        "overview/regenerate"
    } else {
        "regenerate"
    };
    let mut controls = layer_button(
        "overview",
        "Overview",
        &format!("{}/{}", card.base, overview_endpoint),
        &format!("{}/{}", card.base, overview_regenerate),
        &gen,
        true,
    );
    if card.diff.is_some() {
        controls.push_str(&layer_button(
            "changes",
            "Changes",
            &format!("{}/explain", card.base),
            &format!("{}/regenerate", card.base),
            &gen,
            false,
        ))
    }
    controls.push_str(&layer_button(
        "concepts",
        "Concepts",
        &format!("{}/concepts", card.base),
        &format!("{}/concepts/regenerate", card.base),
        &gen,
        false,
    ));
    controls.push_str(&layer_button(
        "deep",
        "Explain this code in depth",
        &format!("{}/deep", card.base),
        &format!("{}/deep/regenerate", card.base),
        &gen,
        false,
    ));
    let(diff_button,diff_panel)=card.diff.map(|v|(format!("<button class=\"diff-toggle\" aria-expanded=\"false\" data-target=\"diff-{}\">Show Git diff</button>",escape(card.id)),format!("<section id=\"diff-{}\" hidden><h4>Git diff</h4><pre><code>{}</code></pre></section>",escape(card.id),escape(v)))).unwrap_or_default();
    let mut states = String::new();
    if let Some(value) = card.overview {
        states.push_str(&layer_state("overview", value));
    }
    if let Some(value) = card.changes {
        states.push_str(&layer_state("changes", value));
    }
    if let Some(value) = card.concepts {
        states.push_str(&layer_state("concepts", value));
    }
    if let Some(value) = card.deep {
        states.push_str(&layer_state(
            "deep",
            &UnitExplanation {
                overview: String::new(),
                annotations: vec![],
                deep: Some(value.to_string()),
            },
        ));
    }
    format!("<article data-unit-id=\"{}\"><h3>{}</h3><p>{}</p><div class=\"layer-controls\" role=\"tablist\" aria-label=\"Explanation views\">{}</div><p class=\"layer-summary explanation\">{}</p><div class=\"actions\"><button data-generate title=\"Regenerate in-depth explanation when Deep is selected\">Generate selected explanation</button><button class=\"source-toggle\">Read code as text</button>{}</div><h4>Source</h4><div class=\"source-code\">{}</div><textarea class=\"source-text\" readonly hidden aria-label=\"Source code for {}\">{}</textarea><section class=\"deep-section explanation\" hidden><h4>Detailed explanation</h4><p class=\"deep\">{}</p></section>{}{}</article>",escape(card.id),escape(card.name),escape(card.metadata),controls,escape(overview),diff_button,rendered_source(card.source,annotations),escape(card.name),escape(card.source),escape(card.deep.unwrap_or("")),diff_panel,states)
}

fn layer_state(kind: &str, value: &UnitExplanation) -> String {
    let json = serde_json::to_string(value).expect("explanation state serializes");
    format!(
        "<template data-layer-state=\"{}\">{}</template>",
        kind,
        escape(&json)
    )
}

fn rendered_source(source: &str, annotations: &[Annotation]) -> String {
    let lines: Vec<_> = source.lines().collect();
    let mut html = String::new();
    let mut cursor = 0;
    let mut annotations = annotations.to_vec();
    annotations.sort_by_key(|a| a.start_line);
    for a in annotations {
        let start = a.start_line.saturating_sub(1).min(lines.len());
        let end = a.end_line.max(a.start_line).min(lines.len());
        if start > cursor {
            html.push_str(&format!(
                "<pre><code>{}</code></pre>",
                escape(&lines[cursor..start].join("\n"))
            ))
        }
        if end > start {
            html.push_str(&format!("<pre><code>{}</code></pre><section class=\"annotation explanation\"><h4>{}</h4><p>{}</p></section>",escape(&lines[start..end].join("\n")),escape(&a.kind),escape(&a.text)))
        }
        cursor = cursor.max(end)
    }
    if cursor < lines.len() {
        html.push_str(&format!(
            "<pre><code>{}</code></pre>",
            escape(&lines[cursor..].join("\n"))
        ))
    }
    if html.is_empty() {
        format!("<pre><code>{}</code></pre>", escape(source))
    } else {
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn annotations_render_between_source_regions() {
        let html = rendered_source(
            "first\nsecond\nthird",
            &[Annotation {
                start_line: 2,
                end_line: 2,
                kind: "concept".into(),
                text: "Explains second".into(),
            }],
        );
        assert!(
            html.contains("first")
                && html.contains("second")
                && html.contains("Explains second")
                && html.contains("third")
        )
    }
    #[test]
    fn file_mode_has_one_source_and_no_changes_layer() {
        let source = crate::change_set::SourceFile {
            path: "src/lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        };
        let unsupported = crate::change_set::SourceFile {
            path: "notes.txt".into(),
            content: "not source code\n".into(),
        };
        let analysis = FileAnalysis {
            revision: None,
            documents: vec![
                crate::semantic::extract_document(&source).unwrap(),
                crate::semantic::extract_document(&unsupported).unwrap(),
            ],
            explanations: Default::default(),
            concept_explanations: Default::default(),
            deep_explanations: Default::default(),
        };
        let html = render_file_analysis(&analysis);
        assert_eq!(html.matches("class=\"source-code\"").count(), 1);
        assert!(
            html.contains("data-layer=\"overview\"")
                && html.contains("data-layer=\"concepts\"")
                && html.contains("data-layer=\"deep\"")
        );
        assert!(!html.contains("data-layer=\"changes\""));
        assert!(html.contains("data-generate"));
        assert!(html.contains("Generate selected explanation"));
        assert!(!html.contains("notes.txt"));
        assert!(!html.contains("concept-panel"))
    }
}
