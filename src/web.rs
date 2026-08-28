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
    let summary = change_set.summary();
    let endpoint = if session_id.is_empty() {
        "/api/units/".to_string()
    } else {
        format!("/api/sessions/{session_id}/units/")
    };
    let mut html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>git-explain changes</title><style>:root{{--page:#f7f8fa;--panel:#fff;--text:#18202a;--muted:#5c6875;--border:#d7dde5;--code:#f1f4f7;--accent:#185abc}}body{{font:1rem/1.55 system-ui,sans-serif;background:var(--page);color:var(--text);margin:auto;max-width:1100px;padding:2rem}}section,article{{background:var(--panel);border:1px solid var(--border);border-radius:.6rem;padding:1.25rem;margin:1.25rem 0}}h1,h2,h3,h4{{line-height:1.2}}pre,textarea{{font:.9rem/1.55 ui-monospace,Consolas,monospace;background:var(--code);white-space:pre;overflow:auto;padding:1rem;width:100%;box-sizing:border-box}}button{{padding:.45rem .7rem;margin:.25rem;border:1px solid var(--border);border-radius:.35rem;background:var(--panel);color:var(--text)}}button:focus-visible{{outline:3px solid var(--accent)}}.muted{{color:var(--muted)}}.actions{{margin:.75rem 0}}.sr-only{{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)}}@media(prefers-color-scheme:dark){{:root{{--page:#151a21;--panel:#1d242d;--text:#e8edf2;--muted:#aab6c2;--border:#3a4652;--code:#11171d;--accent:#8ab4f8}}}}</style></head><body><main><header><h1>Changes</h1><p>{}</p>{}</header><div class="actions"><button id="toggle-explanations">Hide explanations</button></div><p id="status" role="status" aria-live="polite"></p><section aria-labelledby="summary"><h2 id="summary">Change summary</h2><p>{} files changed.</p><ul><li>Added: {}</li><li>Modified: {}</li><li>Deleted: {}</li><li>Renamed: {}</li><li>Copied: {}</li></ul><p>Semantic changes — Added: {}; Modified: {}; Deleted: {}; Preserved: {}</p></section>"#,
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
    html.push_str(r#"<script>const status=document.querySelector('#status');function announce(v){status.textContent=v}function escapeHtml(v){return v.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}function renderAnnotations(article,annotations){const lines=article.querySelector('.source-text').value.split('\n'),target=article.querySelector('.source-code');let cursor=0,html='';(annotations||[]).slice().sort((a,b)=>a.start_line-b.start_line).forEach(a=>{const start=Math.max(cursor,Math.min(lines.length,a.start_line-1)),end=Math.max(start,Math.min(lines.length,a.end_line));if(start>cursor)html+='<pre><code>'+escapeHtml(lines.slice(cursor,start).join('\n'))+'</code></pre>';if(end>start)html+='<pre><code>'+escapeHtml(lines.slice(start,end).join('\n'))+'</code></pre><section class="explanation"><h4>'+escapeHtml(a.kind)+'</h4><p>'+escapeHtml(a.text)+'</p></section>';cursor=end});if(cursor<lines.length)html+='<pre><code>'+escapeHtml(lines.slice(cursor).join('\n'))+'</code></pre>';target.innerHTML=html}async function explain(button){let original=button.textContent;button.disabled=true;button.textContent='Generating…';try{let r=await fetch(button.dataset.endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({generation:Number(button.dataset.generation)})}),v=await r.json();if(!v.ok)throw Error(v.error);let article=button.closest('article');if(button.dataset.deep==='true'){article.querySelector('.deep').textContent=v.deep||v.overview;article.querySelector('.deep-section').hidden=false}else{article.querySelector('.overview').textContent=v.overview||'Explanation unavailable.';renderAnnotations(article,v.annotations)}announce('Explanation ready.')}catch(e){button.closest('article').querySelector('.overview').textContent=e.message;announce(e.message)}finally{button.disabled=false;button.textContent=original}}document.querySelectorAll('[data-endpoint]').forEach(b=>b.onclick=()=>explain(b));document.querySelectorAll('.source-toggle').forEach(b=>b.onclick=()=>{let a=b.closest('article'),code=a.querySelector('.source-code'),text=a.querySelector('.source-text'),open=text.hidden;text.hidden=!open;code.hidden=open;b.textContent=open?'Show rendered code':'Read code as text';announce(open?'Text code view available.':'Rendered code view available.')});document.querySelectorAll('.diff-toggle').forEach(b=>b.onclick=()=>{let target=document.getElementById(b.dataset.target),open=target.hidden;target.hidden=!open;b.setAttribute('aria-expanded',String(open));b.textContent=open?'Hide Git diff':'Show Git diff'});document.querySelector('#toggle-explanations').onclick=e=>{let hide=e.target.textContent.startsWith('Hide');document.querySelectorAll('.explanation').forEach(x=>x.hidden=hide);e.target.textContent=hide?'Show explanations':'Hide explanations';announce(hide?'Explanations hidden.':'Explanations shown.')};</script>"#);
    if !session_id.is_empty() {
        html.push_str(&format!(r#"<script>(function(){{const generation={generation},session={session_id:?};setInterval(async()=>{{try{{const value=await (await fetch('/api/sessions/'+session+'/snapshot')).json();if(value.ok&&Number(value.generation)>generation){{status.innerHTML='A newer repository snapshot is available. <button type="button">Reload updated snapshot</button>';status.querySelector('button').onclick=()=>location.reload()}}}}catch(_){{}}}},5000)}})();</script>"#));
    }
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
    let id = escape(&change.id.0);
    let diff_id = format!("diff-{id}");
    html.push_str(&format!("<article data-unit-id=\"{id}\"><h3>{}</h3><p>Status: {:?} · {} · lines {}–{}</p><h4>Overview</h4><p class=\"overview explanation\">{}</p><div class=\"actions\"><button data-generation=\"{}\" data-endpoint=\"{}{}{}\">Generate explanation</button><button data-generation=\"{}\" data-deep=\"true\" data-endpoint=\"{}{}{}\">Explain this code in depth</button><button class=\"source-toggle\">Read code as text</button><button class=\"diff-toggle\" aria-controls=\"{}\" aria-expanded=\"false\" data-target=\"{}\">Show Git diff</button></div><h4>Code and explanation</h4><div class=\"source-code\">{}</div><textarea class=\"source-text\" readonly hidden aria-label=\"Source code for {}\">{}</textarea><section class=\"deep-section explanation\" hidden><h4>Detailed explanation</h4><p class=\"deep\"></p></section><section id=\"{}\" hidden><h4>Git diff</h4><pre><code>{}</code></pre></section></article>", escape(&unit.identity.qualified_name), change.kind, escape(&unit.language), unit.location.start_line, unit.location.end_line, escape(overview), generation, endpoint, id, "/explain", generation, endpoint, id, "/deep", diff_id, diff_id, rendered_source(&unit.source, &change.explanation.annotations), escape(&unit.identity.qualified_name), escape(&unit.source), diff_id, escape(&change.diff)));
}

fn rendered_source(source: &str, annotations: &[crate::model::Annotation]) -> String {
    let lines: Vec<_> = source.lines().collect();
    let mut html = String::new();
    let mut cursor = 0;
    let mut annotations = annotations.to_vec();
    annotations.sort_by_key(|annotation| annotation.start_line);
    for annotation in annotations {
        let start = annotation.start_line.saturating_sub(1).min(lines.len());
        let end = annotation
            .end_line
            .max(annotation.start_line)
            .min(lines.len());
        if start > cursor {
            html.push_str(&format!(
                "<pre><code>{}</code></pre>",
                escape(&lines[cursor..start].join("\n"))
            ));
        }
        if end > start {
            html.push_str(&format!("<pre><code>{}</code></pre><section class=\"explanation\"><h4>{}</h4><p>{}</p></section>", escape(&lines[start..end].join("\n")), escape(&annotation.kind), escape(&annotation.text)));
        }
        cursor = cursor.max(end);
    }
    if cursor < lines.len() {
        html.push_str(&format!(
            "<pre><code>{}</code></pre>",
            escape(&lines[cursor..].join("\n"))
        ));
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
            &[crate::model::Annotation {
                start_line: 2,
                end_line: 2,
                kind: "Behavior".into(),
                text: "Explains second".into(),
            }],
        );
        assert!(html.contains("first"));
        assert!(html.contains("second"));
        assert!(html.contains("Explains second"));
        assert!(html.contains("third"));
    }
}
