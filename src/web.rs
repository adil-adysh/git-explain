use crate::explain::{AnalysisContext, AnalysisMode, ExplainedUnit};
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn render(items: &[ExplainedUnit], context: &AnalysisContext) -> String {
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
                "<p>{}</p><p>Compared with parent: {}</p>",
                escape(subject),
                escape(parent_oid.as_deref().unwrap_or("<empty tree>"))
            );
            if *merge_parent_count > 1 {
                metadata.push_str(
                    "<p>Merge commit detected. Showing changes relative to first parent.</p>",
                );
            }
            (
                format!("Commit {} explanation", &oid[..oid.len().min(12)]),
                format!("Commit {}", &oid[..oid.len().min(12)]),
                metadata,
            )
        }
    };
    let mut h = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>{}</title><style>body{{max-width:75rem;margin:auto;padding:1rem;font:1rem system-ui}}pre{{white-space:pre-wrap;background:#f5f5f5;padding:1rem}}article{{margin-block:2rem}}.annotation{{border-left:4px solid #555;padding:.5rem 1rem}}button{{padding:.5rem;margin:.5rem 0}}</style></head><body><main><h1>{}</h1>{}<p>Explanations are generated from changed code units and are not written to source files.</p><button id="toggle" type="button">Hide explanations</button><div id="status" aria-live="polite"></div>"#,
        escape(&title),
        escape(&heading),
        metadata
    );
    for file in &context.deleted_files {
        h.push_str(&format!("<section><h2>Deleted file: {}</h2><p>Detailed annotated source explanation is not currently supported for deleted files.</p></section>", escape(file)));
    }
    for (i, x) in items.iter().enumerate() {
        let starts_file = i == 0 || items[i - 1].file != x.file;
        if starts_file {
            let changed_units = items
                .iter()
                .filter(|item| item.file == x.file)
                .map(|item| format!("{:?} {}", item.unit.kind, item.unit.name))
                .collect::<Vec<_>>()
                .join(", ");
            h.push_str(&format!(
                r#"<section><h2>{}</h2><h3>File changes</h3><p>Changed code units: {}.</p>"#,
                escape(&x.file),
                escape(&changed_units)
            ));
        }
        let overview = if x.explanation.overview.is_empty() {
            "Explanation has not been generated.".to_string()
        } else {
            escape(&x.explanation.overview)
        };
        let normal_button = if x.explanation.overview.is_empty() {
            "Generate explanation"
        } else {
            "Regenerate explanation"
        };
        h.push_str(&format!(r#"<article data-unit="{}"><p>Changed unit: <strong>{:?}</strong> <code>{}</code>, lines {}-{}.</p><h3>{}</h3><p><strong>{:?}</strong></p><h4>Overview</h4><p class="explanation">{}</p><button type="button" data-index="{}" class="explain">{}</button><h4>Code and explanation</h4>"#,i,x.unit.kind,escape(&x.unit.name),x.unit.start_line,x.unit.end_line,escape(&x.unit.name),x.unit.kind,overview,i,normal_button));
        let lines: Vec<_> = x.unit.source.lines().collect();
        let mut annotations = x.explanation.annotations.clone();
        annotations.sort_by_key(|a| a.start_line);
        let mut next = 0usize;
        for annotation in annotations {
            let at = annotation.start_line.saturating_sub(1).min(lines.len());
            if at > next {
                h.push_str(&format!(
                    r#"<pre><code>{}</code></pre>"#,
                    escape(&lines[next..at].join("\n"))
                ));
            }
            let end = annotation.end_line.min(lines.len()).max(at);
            if end > at {
                h.push_str(&format!(
                    r#"<pre><code>{}</code></pre>"#,
                    escape(&lines[at..end].join("\n"))
                ));
            }
            h.push_str(&format!(
                r#"<section class="annotation explanation"><h5>{}</h5><p>{}</p></section>"#,
                escape(&annotation.kind),
                escape(&annotation.text)
            ));
            next = end.max(next);
        }
        if next < lines.len() {
            h.push_str(&format!(
                r#"<pre><code>{}</code></pre>"#,
                escape(&lines[next..].join("\n"))
            ));
        }
        let ends_file = i + 1 == items.len() || items[i + 1].file != x.file;
        h.push_str(&format!(r#"<button type="button" data-index="{}" class="deep">Explain this code in depth</button><button type="button" data-index="{}" class="regenerate">Regenerate explanation</button><section id="deep-{}" hidden><h4>Detailed explanation</h4><p>{}</p></section></article>{}"#,i,i,i,escape(x.deep_explanation.as_deref().unwrap_or("Deep explanation has not been generated.")),if ends_file { "</section>" } else { "" }));
    }
    h.push_str(r#"</main><script>const status=document.querySelector('#status'),ex=document.querySelectorAll('.explanation');document.querySelector('#toggle').onclick=()=>{const hide=ex[0]&&!ex[0].hidden;ex.forEach(x=>x.hidden=hide);document.querySelector('#toggle').textContent=hide?'Show explanations':'Hide explanations'};async function call(b,url){const i=b.dataset.index,article=b.closest('article');status.textContent='Generating explanation.';try{const r=await fetch(url,{method:'POST'}),j=await r.json();if(!j.ok)throw Error();if(url.endsWith('/deep')){const s=article.querySelector('section[id^=deep-']);s.hidden=false;s.querySelector('p').textContent=j.deep||j.overview;}else{article.querySelector('.explanation').textContent=j.overview||'Explanation unavailable.';}status.textContent='Explanation ready.';}catch(_){status.textContent='Unable to generate explanation.';}}document.querySelectorAll('.explain').forEach(b=>b.onclick=()=>call(b,'/api/units/'+b.dataset.index+'/explain'));document.querySelectorAll('.regenerate').forEach(b=>b.onclick=()=>call(b,'/api/units/'+b.dataset.index+'/regenerate'));document.querySelectorAll('.deep').forEach(b=>b.onclick=()=>call(b,'/api/units/'+b.dataset.index+'/deep'));</script></body></html>"#);
    h
}
