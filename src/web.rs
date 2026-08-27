use crate::explain::{AnalysisContext, AnalysisMode, ExplainedUnit};

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn indentation_description(line: &str) -> String {
    let mut tabs = 0;
    let mut spaces = 0;
    for character in line.chars() {
        match character {
            '\t' => tabs += 1,
            ' ' => spaces += 1,
            _ => break,
        }
    }
    match (tabs, spaces) {
        (0, 0) => "indent 0 spaces".to_string(),
        (0, spaces) => format!(
            "indent {spaces} {}",
            if spaces == 1 { "space" } else { "spaces" }
        ),
        (tabs, 0) => format!("indent {tabs} {}", if tabs == 1 { "tab" } else { "tabs" }),
        (tabs, spaces) => format!(
            "indent {tabs} {} and {spaces} {}",
            if tabs == 1 { "tab" } else { "tabs" },
            if spaces == 1 { "space" } else { "spaces" }
        ),
    }
}

fn indentation_details(source: &str, start_line: usize) -> String {
    let mut html = String::from(
        r#"<ol class="indentation-list" aria-label="Source with indentation details">"#,
    );
    for (offset, raw_line) in source.split('\n').enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let number = start_line + offset;
        if line.is_empty() {
            html.push_str(&format!(
                r#"<li><span class="line-label">Line {number}</span><span class="sr-only">, blank line</span><code aria-hidden="true">&nbsp;</code></li>"#
            ));
        } else {
            html.push_str(&format!(
                r#"<li><span class="line-label">Line {number}</span><span class="sr-only">, {}, </span><code>{}</code></li>"#,
                escape(&indentation_description(line)),
                escape(line)
            ));
        }
    }
    html.push_str("</ol>");
    html
}

fn rendered_source(source: &str, annotations: &[crate::model::Annotation]) -> String {
    let lines: Vec<_> = source.split('\n').collect();
    let mut html = String::new();
    let mut next = 0usize;
    let mut annotations = annotations.to_vec();
    annotations.sort_by_key(|annotation| annotation.start_line);
    for annotation in annotations {
        let at = annotation
            .start_line
            .saturating_sub(1)
            .min(lines.len())
            .max(next);
        if at > next {
            html.push_str(&format!(
                r#"<pre><code>{}</code></pre>"#,
                escape(&lines[next..at].join("\n"))
            ));
        }
        let end = annotation.end_line.min(lines.len()).max(at);
        if end > at {
            html.push_str(&format!(
                r#"<pre><code>{}</code></pre>"#,
                escape(&lines[at..end].join("\n"))
            ));
        }
        html.push_str(&format!(
            r#"<section class="annotation ai-explanation" data-start-line="{}" data-end-line="{}"><h5>{} <span class="annotation-lines">Lines {}–{}</span></h5><p>{}</p></section>"#,
            annotation.start_line,
            annotation.end_line.max(annotation.start_line),
            escape(&annotation.kind),
            annotation.start_line,
            annotation.end_line.max(annotation.start_line),
            escape(&annotation.text)
        ));
        next = end.max(next);
    }
    if next < lines.len() {
        html.push_str(&format!(
            r#"<pre><code>{}</code></pre>"#,
            escape(&lines[next..].join("\n"))
        ));
    }
    html
}

fn rendered_diff(diff: &str) -> String {
    if diff.trim().is_empty() {
        return "<p class=\"muted\">No textual diff is available for this unit.</p>".to_string();
    }
    let mut html = String::from(r#"<ol class="diff-list" aria-label="Git diff lines">"#);
    for line in diff.lines() {
        let (class, marker, text, label) = if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("@@")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            ("diff-meta", "", line, "Diff metadata")
        } else if let Some(line) = line.strip_prefix('+') {
            ("diff-added", "+", line, "Added line")
        } else if let Some(line) = line.strip_prefix('-') {
            ("diff-removed", "−", line, "Removed line")
        } else {
            (
                "diff-context",
                " ",
                line.strip_prefix(' ').unwrap_or(line),
                "Unchanged line",
            )
        };
        html.push_str(&format!(
            r#"<li class="{class}"><span aria-hidden="true">{marker}</span><span class="sr-only">{label}: </span><code>{}</code></li>"#,
            escape(text)
        ));
    }
    html.push_str("</ol>");
    html
}

pub fn render(items: &[ExplainedUnit], context: &AnalysisContext) -> String {
    render_for_session(items, context, "")
}

pub fn render_for_session(
    items: &[ExplainedUnit],
    context: &AnalysisContext,
    session_id: &str,
) -> String {
    render_for_session_at_generation(items, context, session_id, 1)
}

pub fn render_for_session_at_generation(
    items: &[ExplainedUnit],
    context: &AnalysisContext,
    session_id: &str,
    generation: u64,
) -> String {
    let (title, heading, metadata) = match &context.mode {
        AnalysisMode::WorkingTree => (
            "Working tree explanation".to_string(),
            "Working tree".to_string(),
            String::new(),
        ),
        AnalysisMode::Commit {
            oid,
            parent_oid,
            subject,
            merge_parent_count,
        } => {
            let mut metadata = format!(
                "<p class=\"context-subtitle\">{}</p><p class=\"context-detail\">Compared with parent: {}</p>",
                escape(subject), escape(parent_oid.as_deref().unwrap_or("<empty tree>"))
            );
            if *merge_parent_count > 1 {
                metadata.push_str("<p class=\"context-detail\">Merge commit detected. Showing changes relative to first parent.</p>");
            }
            (
                format!("Commit {} explanation", &oid[..oid.len().min(12)]),
                format!("Commit {}", &oid[..oid.len().min(12)]),
                metadata,
            )
        }
    };
    let unit_path = if session_id.is_empty() {
        "/api/units/".to_string()
    } else {
        format!("/api/sessions/{session_id}/units/")
    };
    let mut html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{}</title><style>
:root {{ color-scheme: light dark; --page: #f7f8fa; --panel: #fff; --text: #18202a; --muted: #5c6875; --border: #d7dde5; --code: #f1f4f7; --accent: #185abc; --annotation: #e8f0fe; }}
* {{ box-sizing: border-box; }} body {{ margin: 0; background: var(--page); color: var(--text); font: 1rem/1.55 system-ui, sans-serif; }} main {{ width: min(1100px, calc(100% - 2rem)); margin: 0 auto; padding: 2rem 0 4rem; }}
h1, h2, h3, h4, h5 {{ line-height: 1.2; }} h1 {{ margin: 0; }} h2, h3 {{ margin: 0; }} h4 {{ margin: 1.25rem 0 .5rem; }} h5 {{ margin: 0 0 .25rem; }}
.page-header {{ border-bottom: 1px solid var(--border); margin-bottom: 1.5rem; padding-bottom: 1rem; }} .context-subtitle, .context-detail, .unit-kind, .unit-lines, .file-count, .muted {{ color: var(--muted); }} .context-subtitle {{ font-size: 1.1rem; margin: .4rem 0 0; }} .context-detail {{ margin: .2rem 0; }} .intro {{ color: var(--muted); margin: 1rem 0 1.5rem; }}
.snapshot-update {{ border: 2px solid var(--accent); background: var(--panel); border-radius: .5rem; padding: .75rem 1rem; margin: 1rem 0; }} .file-section {{ margin: 2rem 0; }} .file-header {{ border-bottom: 1px solid var(--border); margin-bottom: 1rem; padding-bottom: .65rem; }}
article {{ background: var(--panel); border: 1px solid var(--border); border-radius: .6rem; margin: 1.25rem 0; padding: 1.25rem; }} .unit-meta {{ color: var(--muted); margin: .35rem 0 1rem; }} .unit-meta code {{ color: var(--text); }}
.unit-actions {{ display: flex; flex-wrap: wrap; gap: .5rem; margin: 1rem 0; }} button {{ border: 1px solid var(--border); border-radius: .35rem; background: var(--panel); color: var(--text); cursor: pointer; font: inherit; padding: .5rem .75rem; }} button:hover, button:focus-visible {{ border-color: var(--accent); }} button.primary {{ background: var(--accent); border-color: var(--accent); color: white; }}
.source-region {{ background: var(--code); border: 1px solid var(--border); border-radius: .35rem; overflow-x: auto; }} pre {{ margin: 0; min-width: max-content; overflow-x: auto; padding: 1rem; }} pre code, textarea, .indentation-list code {{ font: .95rem/1.65 ui-monospace, SFMono-Regular, Consolas, monospace; }} .source-text {{ display: block; min-height: 14rem; resize: vertical; width: 100%; white-space: pre; overflow: auto; padding: 1rem; border: 0; background: var(--code); color: var(--text); }} .diff-list {{ list-style: none; margin: 0; padding: 1rem; }} .diff-list li {{ padding: .1rem .5rem; white-space: pre-wrap; }} .diff-added {{ background: color-mix(in srgb, #2e7d32 16%, transparent); }} .diff-removed {{ background: color-mix(in srgb, #c62828 16%, transparent); }} .diff-meta {{ color: var(--muted); }}
.annotation {{ background: var(--annotation); border-left: 4px solid var(--accent); margin: .75rem 1rem; padding: .65rem 1rem; }} .annotation h5 {{ display: flex; flex-wrap: wrap; gap: .5rem; }} .annotation-lines {{ color: var(--muted); font-size: .85em; font-weight: 400; }} .annotation p {{ margin: 0; }} .indentation-details {{ border-top: 1px solid var(--border); margin-top: 1rem; padding-top: 1rem; }} .indentation-list {{ margin: 0; padding-left: 3.5rem; }} .indentation-list li {{ padding: .15rem 0; }} .line-label {{ color: var(--muted); display: inline-block; min-width: 5rem; margin-left: -2.5rem; margin-right: .5rem; }}
.sr-only {{ clip: rect(0 0 0 0); clip-path: inset(50%); height: 1px; overflow: hidden; position: absolute; white-space: nowrap; width: 1px; }} #status {{ min-height: 1.5em; color: var(--muted); margin: .5rem 0; }} @media (max-width: 600px) {{ main {{ width: min(100% - 1rem, 1100px); padding-top: 1rem; }} article {{ padding: 1rem; }} .unit-actions button {{ flex: 1 1 100%; }} }} @media (prefers-color-scheme: dark) {{ :root {{ --page: #151a21; --panel: #1d242d; --text: #e8edf2; --muted: #aab6c2; --border: #3a4652; --code: #11171d; --accent: #8ab4f8; --annotation: #24364f; }} button.primary {{ color: #101820; }} }}
</style></head><body><main><header class="page-header"><h1>git-explain</h1><p class="context-subtitle">{}</p>{}</header><p class="intro">Explanations are annotations around changed source code. They are never written to source files.</p>{}<div id="snapshot-update" class="snapshot-update" data-generation="{}" role="status" aria-live="polite" hidden><span id="snapshot-update-message"></span> <button id="reload-snapshot" type="button">Reload updated snapshot</button></div><div class="unit-actions"><button id="toggle" type="button">Hide explanations</button></div><div id="status" role="status" aria-live="polite"></div>"#,
        escape(&title),
        escape(&heading),
        metadata,
        context
            .no_op
            .as_deref()
            .map(|message| format!(
                r#"<p class="empty-state" role="status">{}</p>"#,
                escape(message)
            ))
            .unwrap_or_default(),
        generation
    );
    let accessibility_css = r#"<style>button:focus-visible { outline: 3px solid var(--accent); outline-offset: 2px; } @media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation-duration: 0.01ms !important; animation-iteration-count: 1 !important; scroll-behavior: auto !important; transition-duration: 0.01ms !important; } }</style>"#;
    let head_end = html.find("</head>").expect("rendered page has a head");
    html.insert_str(head_end, accessibility_css);
    for file in &context.deleted_files {
        html.push_str(&format!(r#"<section class="file-section"><header class="file-header"><h2>Deleted file: {}</h2></header><p>Detailed annotated source explanation is not currently supported for deleted files.</p></section>"#, escape(file)));
    }
    for (index, item) in items.iter().enumerate() {
        let starts_file = index == 0 || items[index - 1].file != item.file;
        if starts_file {
            let count = items.iter().filter(|other| other.file == item.file).count();
            html.push_str(&format!(r#"<section class="file-section"><header class="file-header"><h2>{}</h2><p class="file-count">{} changed code {}</p></header>"#, escape(&item.file), count, if count == 1 { "unit" } else { "units" }));
        }
        let overview = if item.explanation.overview.is_empty() {
            "Explanation has not been generated."
        } else {
            &item.explanation.overview
        };
        let has_normal = !item.explanation.overview.is_empty();
        let has_deep = item
            .deep_explanation
            .as_deref()
            .is_some_and(|explanation| !explanation.is_empty());
        let normal_action = if has_normal {
            "normal-regenerate"
        } else {
            "normal-generate"
        };
        let normal_endpoint = if has_normal {
            "/regenerate"
        } else {
            "/explain"
        };
        let normal_label = if has_normal {
            "Regenerate explanation"
        } else {
            "Generate explanation"
        };
        let deep_action = if has_deep {
            "deep-regenerate"
        } else {
            "deep-generate"
        };
        let deep_endpoint = if has_deep {
            "/deep/regenerate"
        } else {
            "/deep"
        };
        let deep_label = if has_deep {
            "Regenerate detailed explanation"
        } else {
            "Explain this code in depth"
        };
        let label_id = format!("source-label-{}", item.id);
        let text_id = format!("source-text-{}", item.id);
        let rendered_id = format!("rendered-source-{}", item.id);
        let text_region_id = format!("text-source-{}", item.id);
        let indent_id = format!("indentation-{}", item.id);
        html.push_str(&format!(r#"<article data-unit-id="{}" aria-label="Code unit {}"><h3>{}</h3><p class="unit-meta"><span class="unit-kind">{:?}</span> · {} · <span class="unit-lines">lines {}–{}</span></p><h4>Overview</h4><p class="ai-explanation">{}</p><div class="unit-actions"><button type="button" data-unit-id="{}" data-generation="{}" data-action="{}" data-endpoint="{}" class="explain">{}</button><button type="button" data-unit-id="{}" data-generation="{}" data-action="{}" data-endpoint="{}" class="deep">{}</button><button type="button" class="mode" aria-controls="{} {}">Read code as text</button><button type="button" class="indent-toggle" aria-controls="{}" aria-expanded="false">Show indentation details</button><button type="button" class="diff-toggle" aria-controls="diff-{}" aria-expanded="false">Show Git diff</button></div><h4>Code and explanation</h4><div id="{}" class="source-region rendered-source" role="region" aria-label="Rendered source code">{}</div><div id="{}" class="source-region text-source" hidden><label id="{}" class="sr-only" for="{}">Source code for {}, read only</label><textarea id="{}" class="source-text" readonly spellcheck="false" wrap="off">{}</textarea></div><section id="diff-{}" class="source-region diff-source" role="region" aria-label="Git diff" hidden><h4>Git diff</h4>{}</section><section class="indentation-details" id="{}" hidden><h4>Indentation details</h4>{}</section><section id="deep-{}" class="ai-explanation" hidden><h4>Detailed explanation</h4><p>{}</p></section></article>"#,
            escape(&item.id.to_string()), escape(&item.unit.name), escape(&item.unit.name), item.unit.kind, escape(&item.language), item.unit.start_line, item.unit.end_line, escape(overview), escape(&item.id.to_string()), generation, normal_action, normal_endpoint, normal_label, escape(&item.id.to_string()), generation, deep_action, deep_endpoint, deep_label, escape(&rendered_id), escape(&text_region_id), escape(&indent_id), escape(&item.id.to_string()), escape(&rendered_id), rendered_source(&item.unit.source, &item.explanation.annotations), escape(&text_region_id), escape(&label_id), escape(&text_id), escape(&item.unit.name), escape(&text_id), escape(&item.unit.source), escape(&item.id.to_string()), rendered_diff(&item.diff), escape(&indent_id), indentation_details(&item.unit.source, item.unit.start_line), escape(&item.id.to_string()), escape(item.deep_explanation.as_deref().unwrap_or("Deep explanation has not been generated."))));
        if index + 1 == items.len() || items[index + 1].file != item.file {
            html.push_str("</section>");
        }
    }
    html.push_str(&format!(r#"<script>
const status=document.querySelector('#status'), unitPath='{}';
function announce(message) {{ status.textContent=message; }}
function escapeHtml(value) {{ return value.replace(/[&<>"']/g, character => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[character])); }}
let explanationsHidden=false;
function renderAnnotations(article, annotations) {{ const rendered=article.querySelector('.rendered-source'), lines=article.querySelector('.source-text').value.split('\n'); let next=0; rendered.replaceChildren(); (annotations||[]).slice().sort((a,b)=>a.start_line-b.start_line).forEach(annotation=>{{ const start=Math.max(next,Math.min(lines.length,annotation.start_line-1)),end=Math.max(start,Math.min(lines.length,annotation.end_line)),startLine=annotation.start_line,endLine=Math.max(startLine,annotation.end_line),hidden=explanationsHidden?' hidden data-toggle-hidden="true"':''; if(start>next)rendered.insertAdjacentHTML('beforeend','<pre><code>'+escapeHtml(lines.slice(next,start).join('\n'))+'</code></pre>'); if(end>start)rendered.insertAdjacentHTML('beforeend','<pre><code>'+escapeHtml(lines.slice(start,end).join('\n'))+'</code></pre>'); rendered.insertAdjacentHTML('beforeend','<section class="annotation ai-explanation" data-start-line="'+startLine+'" data-end-line="'+endLine+'"'+hidden+'><h5>'+escapeHtml(annotation.kind)+' <span class="annotation-lines">Lines '+startLine+'–'+endLine+'</span></h5><p>'+escapeHtml(annotation.text)+'</p></section>'); next=Math.max(next,end); }}); if(next<lines.length)rendered.insertAdjacentHTML('beforeend','<pre><code>'+escapeHtml(lines.slice(next).join('\n'))+'</code></pre>'); }}
document.querySelectorAll('article[data-unit-id]').forEach(article=>{{ const mode=article.querySelector('.mode'),rendered=article.querySelector('.rendered-source'),text=article.querySelector('.text-source'); mode.addEventListener('click',()=>{{ const textMode=text.hidden; rendered.hidden=textMode;text.hidden=!textMode;mode.textContent=textMode?'Show rendered code':'Read code as text';announce(textMode?'Text code view available.':'Rendered code view available.'); }}); const indent=article.querySelector('.indent-toggle'),details=article.querySelector('.indentation-details'); indent.addEventListener('click',()=>{{ const open=details.hidden;details.hidden=!open;indent.setAttribute('aria-expanded',String(open));indent.textContent=open?'Hide indentation details':'Show indentation details';announce(open?'Indentation details available.':'Indentation details hidden.'); }}); }});
document.querySelector('#toggle').addEventListener('click',event=>{{ const hide=event.target.textContent.startsWith('Hide');explanationsHidden=hide;document.querySelectorAll('.ai-explanation').forEach(node=>{{if(hide){{if(!node.hidden){{node.dataset.toggleHidden='true';node.hidden=true;}}}}else if(node.dataset.toggleHidden==='true'){{node.hidden=false;delete node.dataset.toggleHidden;}}}});event.target.textContent=hide?'Show explanations':'Hide explanations';announce(hide?'Explanations hidden.':'Explanations shown.'); }});
document.querySelectorAll('.diff-toggle').forEach(toggle=>toggle.addEventListener('click',()=>{{ const diff=document.getElementById(toggle.getAttribute('aria-controls')); const open=diff.hidden; diff.hidden=!open; toggle.setAttribute('aria-expanded',String(open)); toggle.textContent=open?'Hide Git diff':'Show Git diff'; announce(open?'Git diff available.':'Git diff hidden.'); }}));
// Keep failures visible on the unit that produced them, while retaining the global live status.
async function call(button) {{ const article=button.closest('article'),endpoint=button.dataset.endpoint,original=button.textContent;let errorBox=article.querySelector('.generation-error');if(!errorBox){{errorBox=document.createElement('p');errorBox.className='generation-error';errorBox.setAttribute('role','alert');button.closest('.unit-actions').before(errorBox);}}let timer;button.disabled=true;button.setAttribute('aria-busy','true');button.textContent='Generating…';errorBox.hidden=true;errorBox.textContent='';announce('Generating explanation.');try{{const controller=new AbortController();timer=setTimeout(()=>controller.abort(),130000);const response=await fetch(unitPath+button.dataset.unitId+endpoint,{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{generation:Number(button.dataset.generation)}}),signal:controller.signal}});let result;try{{result=await response.json();}}catch(_){{throw Error('The local server returned an invalid response.');}}if(!response.ok||!result.ok)throw Error(result.error||'The model could not generate an explanation.');if(button.dataset.action.startsWith('deep-')){{const section=article.querySelector('[id^="deep-"]');section.hidden=explanationsHidden;section.querySelector('p').textContent=result.deep||result.overview;button.dataset.action='deep-regenerate';button.dataset.endpoint='/deep/regenerate';button.textContent='Regenerate detailed explanation';}}else{{article.querySelector('h4 + .ai-explanation').textContent=result.overview||'Explanation unavailable.';renderAnnotations(article,result.annotations);button.dataset.action='normal-regenerate';button.dataset.endpoint='/regenerate';button.textContent='Regenerate explanation';}}announce('Explanation ready.');}}catch(error){{const message=error&&error.name==='AbortError'?'The request timed out. Try again or reduce the requested detail.':error&&error.name==='TypeError'?'Unable to reach the local git-explain server. Check that it is still running.':(error&&error.message)||'Unable to generate explanation. Try again.';button.textContent=original;errorBox.textContent=message;errorBox.hidden=false;announce(message);}}finally{{clearTimeout(timer);button.disabled=false;button.removeAttribute('aria-busy');}} }}
document.querySelectorAll('.explain,.deep').forEach(button=>button.addEventListener('click',()=>call(button)));
</script>"#, escape(&unit_path)));
    if !session_id.is_empty() {
        html.push_str(&format!(r#"<script>(function(){{const box=document.querySelector('#snapshot-update'),message=document.querySelector('#snapshot-update-message'),reload=document.querySelector('#reload-snapshot'),session={:?};function showSnapshotUpdate(text){{message.textContent=text;box.hidden=false}}window.showSnapshotUpdate=showSnapshotUpdate;reload.onclick=()=>location.reload();async function checkSnapshot(){{try{{const response=await fetch('/api/sessions/'+session+'/snapshot'),value=await response.json();if(value.ok&&Number(value.generation)>Number(box.dataset.generation))showSnapshotUpdate('A newer repository snapshot is available. Reload to view it.')}}catch(_){{}}}}setInterval(checkSnapshot,5000)}})();</script>"#, session_id));
    }
    html.push_str("</main></body></html>");
    html
}
