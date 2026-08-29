use crate::change_set::{ChangeSet, FileChange, SemanticUnitChange, SemanticUnitChangeKind};
use crate::explain::AnalysisMode;
use crate::file_analysis::{unit_key, FileAnalysis};

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

pub fn render_file_analysis(analysis: &FileAnalysis) -> String {
    let mut html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>git-explain files</title><style>body{{font:1rem/1.55 system-ui,sans-serif;max-width:1100px;margin:auto;padding:2rem}}section,article{{border:1px solid #bbb;border-radius:.6rem;padding:1.25rem;margin:1.25rem 0}}pre,textarea{{box-sizing:border-box;width:100%;overflow:auto;padding:1rem;white-space:pre;font:.9rem/1.55 ui-monospace,monospace}}button{{margin:.25rem;padding:.45rem .7rem}}</style></head><body><main><h1>Files</h1><p>Revision: {}</p><button id=\"toggle-explanations\">Hide explanations</button><p id=\"status\" role=\"status\" aria-live=\"polite\"></p>",
        escape(analysis.revision.as_deref().unwrap_or("working tree"))
    );
    for document in &analysis.documents {
        html.push_str(&format!(
            "<section><h2>{}</h2><p>Language: {}</p><p>Semantic units: {}</p>",
            escape(&document.source.path.display().to_string()),
            escape(document.language.as_deref().unwrap_or("unsupported")),
            document.units.len()
        ));
        if document.units.is_empty() {
            html.push_str("<p>No semantic units were found in this file.</p>");
        }
        for unit in &document.units {
            let id = unit_key(unit);
            let explanation = analysis
                .explanations
                .get(&id)
                .map(|value| value.overview.as_str())
                .unwrap_or("Explanation has not been generated.");
            let deep = analysis.deep_explanations.get(&id);
            let normal_cached = analysis.explanations.contains_key(&id);
            html.push_str(&format!("<article data-unit-id=\"{id}\"><h3>{}</h3><p>{:?} · lines {}–{}</p><h4>Overview</h4><p class=\"overview explanation\">{}</p><button data-endpoint=\"/api/file-units/{id}/{}\" data-regenerate-label=\"Regenerate explanation\">{}</button><button data-deep=\"true\" data-endpoint=\"/api/file-units/{id}/{}\" data-regenerate-label=\"Regenerate in-depth explanation\">{}</button><button class=\"source-toggle\">Read code as text</button><h4>Code and explanation</h4><div class=\"source-code\">{}</div><textarea class=\"source-text\" readonly hidden>{}</textarea><section class=\"deep-section explanation\" {}><h4>Detailed explanation</h4><p class=\"deep\">{}</p></section></article>", escape(&unit.identity.qualified_name), unit.identity.kind, unit.location.start_line, unit.location.end_line, escape(explanation), if normal_cached{"regenerate"}else{"explain"}, if normal_cached{"Regenerate explanation"}else{"Generate explanation"}, if deep.is_some(){"deep/regenerate"}else{"deep"}, if deep.is_some(){"Regenerate in-depth explanation"}else{"Explain this code in depth"}, rendered_source(&unit.source, analysis.explanations.get(&id).map(|value|value.annotations.as_slice()).unwrap_or(&[])), escape(&unit.source), if deep.is_some(){""}else{"hidden"}, escape(deep.map(String::as_str).unwrap_or(""))));
        }
        html.push_str("</section>");
    }
    html.push_str(r#"<script>const status=document.querySelector('#status');function announce(value){status.textContent=value}function escapeHtml(value){return value.replace(/[&<>"']/g,character=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[character]))}function renderAnnotations(article,annotations){const lines=article.querySelector('.source-text').value.split('\n'),target=article.querySelector('.source-code');let cursor=0,html='';(annotations||[]).slice().sort((a,b)=>a.start_line-b.start_line).forEach(annotation=>{const start=Math.max(cursor,Math.min(lines.length,annotation.start_line-1)),end=Math.max(start,Math.min(lines.length,annotation.end_line));if(start>cursor)html+='<pre><code>'+escapeHtml(lines.slice(cursor,start).join('\n'))+'</code></pre>';if(end>start)html+='<pre><code>'+escapeHtml(lines.slice(start,end).join('\n'))+'</code></pre><section class="explanation"><h4>'+escapeHtml(annotation.kind)+'</h4><p>'+escapeHtml(annotation.text)+'</p></section>';cursor=end});if(cursor<lines.length)html+='<pre><code>'+escapeHtml(lines.slice(cursor).join('\n'))+'</code></pre>';target.innerHTML=html}async function explain(button){const original=button.textContent;button.disabled=true;button.textContent='Generating…';try{const value=await(await fetch(button.dataset.endpoint,{method:'POST'})).json();if(!value.ok)throw Error(value.error);const article=button.closest('article');if(button.dataset.deep==='true'){article.querySelector('.deep').textContent=value.deep||value.overview;article.querySelector('.deep-section').hidden=false}else{article.querySelector('.overview').textContent=value.overview||'Explanation unavailable.';renderAnnotations(article,value.annotations);article.querySelector('.deep').textContent='';article.querySelector('.deep-section').hidden=true}button.dataset.endpoint+=button.dataset.endpoint.endsWith('/regenerate')?'':'/regenerate';button.textContent=button.dataset.regenerateLabel;announce(value.cache_hit?'Cached explanation ready.':'Explanation ready.')}catch(error){button.textContent=original;announce(error.message)}finally{button.disabled=false}}document.querySelectorAll('[data-endpoint]').forEach(button=>button.onclick=()=>explain(button));document.querySelectorAll('.source-toggle').forEach(button=>button.onclick=()=>{const article=button.closest('article'),code=article.querySelector('.source-code'),text=article.querySelector('.source-text'),open=text.hidden;text.hidden=!open;code.hidden=open;button.textContent=open?'Show rendered code':'Read code as text';announce(open?'Text code view available.':'Rendered code view available.')});document.querySelector('#toggle-explanations').onclick=event=>{const hide=event.target.textContent.startsWith('Hide');document.querySelectorAll('.explanation').forEach(element=>element.hidden=hide);event.target.textContent=hide?'Show explanations':'Hide explanations';announce(hide?'Explanations hidden.':'Explanations shown.')};</script></main></body></html>"#);
    html
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
    html.push_str(r#"<script>const status=document.querySelector('#status');function announce(v){status.textContent=v}function escapeHtml(v){return v.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}function renderAnnotations(article,annotations){const lines=article.querySelector('.source-text').value.split('\n'),target=article.querySelector('.source-code');let cursor=0,html='';(annotations||[]).slice().sort((a,b)=>a.start_line-b.start_line).forEach(a=>{const start=Math.max(cursor,Math.min(lines.length,a.start_line-1)),end=Math.max(start,Math.min(lines.length,a.end_line));if(start>cursor)html+='<pre><code>'+escapeHtml(lines.slice(cursor,start).join('\n'))+'</code></pre>';if(end>start)html+='<pre><code>'+escapeHtml(lines.slice(start,end).join('\n'))+'</code></pre><section class="explanation"><h4>'+escapeHtml(a.kind)+'</h4><p>'+escapeHtml(a.text)+'</p></section>';cursor=end});if(cursor<lines.length)html+='<pre><code>'+escapeHtml(lines.slice(cursor).join('\n'))+'</code></pre>';target.innerHTML=html}async function explain(button){let original=button.textContent;button.disabled=true;button.textContent='Generating…';try{let r=await fetch(button.dataset.endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({generation:Number(button.dataset.generation)})}),v=await r.json();if(!v.ok)throw Error(v.error);let article=button.closest('article');if(button.dataset.deep==='true'){article.querySelector('.deep').textContent=v.deep||v.overview;article.querySelector('.deep-section').hidden=false}else{article.querySelector('.overview').textContent=v.overview||'Explanation unavailable.';renderAnnotations(article,v.annotations);article.querySelector('.deep').textContent='';article.querySelector('.deep-section').hidden=true;let deepButton=article.querySelector('[data-deep="true"]');deepButton.dataset.endpoint=deepButton.dataset.endpoint.replace('/deep/regenerate','/deep');deepButton.textContent='Explain this code in depth'}button.dataset.endpoint+=button.dataset.endpoint.endsWith('/regenerate')?'':'/regenerate';button.textContent=button.dataset.regenerateLabel;announce(v.cache_hit?'Cached explanation ready.':'Explanation ready.')}catch(e){button.textContent=original;button.closest('article').querySelector('.overview').textContent=e.message;announce(e.message)}finally{button.disabled=false}}document.querySelectorAll('[data-endpoint]').forEach(b=>b.onclick=()=>explain(b));document.querySelectorAll('.source-toggle').forEach(b=>b.onclick=()=>{let a=b.closest('article'),code=a.querySelector('.source-code'),text=a.querySelector('.source-text'),open=text.hidden;text.hidden=!open;code.hidden=open;b.textContent=open?'Show rendered code':'Read code as text';announce(open?'Text code view available.':'Rendered code view available.')});document.querySelectorAll('.diff-toggle').forEach(b=>b.onclick=()=>{let target=document.getElementById(b.dataset.target),open=target.hidden;target.hidden=!open;b.setAttribute('aria-expanded',String(open));b.textContent=open?'Hide Git diff':'Show Git diff'});document.querySelector('#toggle-explanations').onclick=e=>{let hide=e.target.textContent.startsWith('Hide');document.querySelectorAll('.explanation').forEach(x=>x.hidden=hide);e.target.textContent=hide?'Show explanations':'Hide explanations';announce(hide?'Explanations hidden.':'Explanations shown.')};</script>"#);
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
    let normal_cached = !change.explanation.overview.is_empty();
    let deep = change.deep_explanation.as_deref();
    let normal_suffix = if normal_cached {
        "/regenerate"
    } else {
        "/explain"
    };
    let deep_suffix = if deep.is_some() {
        "/deep/regenerate"
    } else {
        "/deep"
    };
    html.push_str(&format!("<article data-unit-id=\"{id}\"><h3>{}</h3><p>Status: {:?} · {} · lines {}–{}</p><h4>Overview</h4><p class=\"overview explanation\">{}</p><div class=\"actions\"><button data-generation=\"{}\" data-endpoint=\"{}{}{}\" data-regenerate-label=\"Regenerate explanation\">{}</button><button data-generation=\"{}\" data-deep=\"true\" data-endpoint=\"{}{}{}\" data-regenerate-label=\"Regenerate in-depth explanation\">{}</button><button class=\"source-toggle\">Read code as text</button><button class=\"diff-toggle\" aria-controls=\"{}\" aria-expanded=\"false\" data-target=\"{}\">Show Git diff</button></div><h4>Code and explanation</h4><div class=\"source-code\">{}</div><textarea class=\"source-text\" readonly hidden aria-label=\"Source code for {}\">{}</textarea><section class=\"deep-section explanation\" {}><h4>Detailed explanation</h4><p class=\"deep\">{}</p></section><section id=\"{}\" hidden><h4>Git diff</h4><pre><code>{}</code></pre></section></article>", escape(&unit.identity.qualified_name), change.kind, escape(&unit.language), unit.location.start_line, unit.location.end_line, escape(overview), generation, endpoint, id, normal_suffix, if normal_cached {"Regenerate explanation"} else {"Generate explanation"}, generation, endpoint, id, deep_suffix, if deep.is_some() {"Regenerate in-depth explanation"} else {"Explain this code in depth"}, diff_id, diff_id, rendered_source(&unit.source, &change.explanation.annotations), escape(&unit.identity.qualified_name), escape(&unit.source), if deep.is_some(){""}else{"hidden"}, escape(deep.unwrap_or("")), diff_id, escape(&change.diff)));
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

    #[test]
    fn file_analysis_keeps_rich_controls_without_fake_change_status() {
        let source = crate::change_set::SourceFile {
            path: "src/lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        };
        let document = crate::semantic::extract_document(&source).unwrap();
        let analysis = FileAnalysis {
            revision: Some("abc123".into()),
            documents: vec![document],
            explanations: Default::default(),
            deep_explanations: Default::default(),
        };
        let html = render_file_analysis(&analysis);
        assert!(html.contains("Generate explanation"));
        assert!(html.contains("Explain this code in depth"));
        assert!(html.contains("Read code as text"));
        assert!(html.contains("Hide explanations"));
        assert!(html.contains("renderAnnotations(article,value.annotations)"));
        assert!(html.contains("value.cache_hit"));
        assert!(!html.contains("value.cached"));
        assert!(!html.contains("Status: Added"));
        assert!(!html.contains("Show Git diff"));
    }

    #[test]
    fn file_analysis_renders_cached_results_with_regenerate_actions() {
        let source = crate::change_set::SourceFile {
            path: "src/lib.rs".into(),
            content: "fn inspect() {}\n".into(),
        };
        let document = crate::semantic::extract_document(&source).unwrap();
        let id = unit_key(&document.units[0]);
        let mut analysis = FileAnalysis {
            revision: Some("abc123".into()),
            documents: vec![document],
            explanations: Default::default(),
            deep_explanations: Default::default(),
        };
        analysis.explanations.insert(
            id.clone(),
            crate::model::UnitExplanation {
                overview: "Cached overview".into(),
                annotations: vec![crate::model::Annotation {
                    start_line: 1,
                    end_line: 1,
                    kind: "Behavior".into(),
                    text: "Cached annotation".into(),
                }],
                deep: None,
            },
        );
        analysis
            .deep_explanations
            .insert(id.clone(), "Cached detail".into());

        let html = render_file_analysis(&analysis);
        assert!(html.contains(&format!("/api/file-units/{id}/regenerate")));
        assert!(html.contains(&format!("/api/file-units/{id}/deep/regenerate")));
        assert!(html.contains("Cached overview"));
        assert!(html.contains("Cached annotation"));
        assert!(html.contains("Cached detail"));
    }
}
