use crate::change_set::{ChangeSet, FileChange, SemanticUnitChange, SemanticUnitChangeKind};
use crate::explain::AnalysisMode;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn render(change_set: &ChangeSet) -> String {
    render_for_session_at_generation(change_set, "", change_set.generation.0)
}
pub fn render_for_session_at_generation(
    change_set: &ChangeSet,
    session_id: &str,
    generation: u64,
) -> String {
    let (heading, detail) = match &change_set.context.mode {
        AnalysisMode::WorkingTree => ("Working tree".to_string(), "".to_string()),
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
    };
    let summary = change_set.summary();
    let endpoint = if session_id.is_empty() {
        "/api/units/".to_string()
    } else {
        format!("/api/sessions/{session_id}/units/")
    };
    let mut html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>git-explain changes</title><style>body{{font:1rem/1.5 system-ui,sans-serif;margin:auto;max-width:1100px;padding:2rem}}article,section{{border:1px solid #bbb;border-radius:.4rem;padding:1rem;margin:1rem 0}}code,pre{{white-space:pre-wrap}}.muted{{color:#555}}button{{padding:.45rem;margin-right:.5rem}}</style></head><body><main><header><h1>Changes</h1><p>{}</p>{}</header><section aria-labelledby="summary"><h2 id="summary">Change summary</h2><p>{} files changed.</p><ul><li>Added: {}</li><li>Modified: {}</li><li>Deleted: {}</li><li>Renamed: {}</li><li>Copied: {}</li></ul><p>Semantic changes — Added: {}; Modified: {}; Deleted: {}; Preserved: {}</p></section>"#,
        escape(&heading),
        detail,
        change_set.files.len(),
        summary.files_added,
        summary.files_modified,
        summary.files_deleted,
        summary.files_renamed,
        summary.files_copied,
        summary.units_added,
        summary.units_modified,
        summary.units_deleted,
        summary.units_preserved
    );
    if let Some(message) = change_set.empty_state() {
        html.push_str(&format!("<p role=\"status\">{}</p>", escape(&message)));
    }
    html.push_str("<nav aria-labelledby=\"files\"><h2 id=\"files\">Files</h2><ol>");
    for (index, file) in change_set.files.iter().enumerate() {
        html.push_str(&format!(
            "<li><a href=\"#file-{}\">{}</a> — {}</li>",
            index + 1,
            escape(&file.display_path().display().to_string()),
            file.status_label()
        ));
    }
    html.push_str("</ol></nav>");
    for (index, file) in change_set.files.iter().enumerate() {
        render_file(&mut html, file, index + 1, &endpoint, generation);
    }
    html.push_str(r#"<script>async function explain(button){button.disabled=true;try{let r=await fetch(button.dataset.endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({generation:Number(button.dataset.generation)})}),v=await r.json();if(!v.ok)throw Error(v.error);button.closest('article').querySelector('.overview').textContent=v.overview||'Explanation unavailable.'}catch(e){button.closest('article').querySelector('.overview').textContent=e.message}finally{button.disabled=false}}document.querySelectorAll('[data-endpoint]').forEach(b=>b.onclick=()=>explain(b));</script></main></body></html>"#);
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
    if let Some(path) = &file.old_path {
        if Some(path) != file.new_path.as_ref() {
            html.push_str(&format!(
                "<p>Previous path: {}</p>",
                escape(&path.display().to_string())
            ));
        }
    }
    if let Some(similarity) = file.similarity {
        html.push_str(&format!("<p>Similarity: {similarity}%</p>"));
    }
    if file.binary {
        html.push_str("<p>Content: Binary. No semantic source explanation is available.</p>");
    }
    if file.mode_changed() {
        html.push_str(&format!(
            "<p>File mode: {} → {}</p>",
            file.old_mode.as_deref().unwrap_or("<none>"),
            file.new_mode.as_deref().unwrap_or("<none>")
        ));
    }
    html.push_str(&format!(
        "<p>Source content changed: {}</p>",
        if file.source_content_changed() {
            "Yes"
        } else {
            "No"
        }
    ));
    let preserved = file
        .semantic_changes
        .iter()
        .filter(|change| change.kind == SemanticUnitChangeKind::Preserved)
        .count();
    if preserved > 0 {
        html.push_str(&format!("<p>Semantic units preserved: {preserved}</p>"));
    }
    if file
        .semantic_changes
        .iter()
        .all(|change| !change.kind.explainable())
    {
        html.push_str("<p>No semantic changes require explanation.</p>");
    }
    for change in file
        .semantic_changes
        .iter()
        .filter(|change| change.kind.explainable())
    {
        render_change(html, change, endpoint, generation);
    }
    html.push_str("</section>");
}
fn render_change(html: &mut String, change: &SemanticUnitChange, endpoint: &str, generation: u64) {
    let unit = change.active_unit();
    let overview = if change.explanation.overview.is_empty() {
        "Explanation has not been generated."
    } else {
        &change.explanation.overview
    };
    html.push_str(&format!("<article data-unit-id=\"{}\"><h3>{}</h3><p>Status: {:?}</p><p class=\"overview\">{}</p><button data-generation=\"{}\" data-endpoint=\"{}{}{}\">Generate explanation</button><h4>Source</h4><pre><code>{}</code></pre></article>", escape(&change.id.0), escape(&unit.identity.qualified_name), change.kind, escape(overview), generation, endpoint, escape(&change.id.0), "/explain", escape(&unit.source)));
}
