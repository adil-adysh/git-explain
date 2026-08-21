use crate::explain::ExplainedFunction;
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn render(items: &[ExplainedFunction]) -> String {
    let mut h = String::from(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Working tree explanation</title><style>body{max-width:75rem;margin:auto;padding:1rem;font:1rem system-ui}pre{white-space:pre-wrap;background:#f5f5f5;padding:1rem}article{margin-block:2rem}.annotation{border-left:4px solid #555;padding:.5rem 1rem}button{padding:.5rem;margin:.5rem 0}</style></head><body><main><h1>Working tree</h1><p>Explanations are generated from changed functions only and are not written to source files.</p><button id="toggle" type="button">Hide explanations</button><div id="status" aria-live="polite"></div>"#,
    );
    for (i, x) in items.iter().enumerate() {
        h.push_str(&format!(r#"<section><h2>{}</h2><article><h3>{}</h3><h4>Overview</h4><p class="explanation">{}</p><h4>Code and explanation</h4>"#,escape(&x.file),escape(&x.symbol.name),escape(&x.explanation.overview)));
        let lines: Vec<_> = x.symbol.source.lines().collect();
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
        h.push_str(&format!(r#"<button type="button" data-index="{}" class="deep">Explain this function in depth</button><section id="deep-{}" hidden><h4>Detailed explanation</h4><p></p></section></article></section>"#,i,i));
    }
    h.push_str(r#"</main><script>const ex=document.querySelectorAll('.explanation');document.querySelector('#toggle').onclick=()=>{const hide=ex[0]&&!ex[0].hidden;ex.forEach(x=>x.hidden=hide);document.querySelector('#toggle').textContent=hide?'Show explanations':'Hide explanations'};document.querySelectorAll('.deep').forEach(b=>b.onclick=async()=>{const i=b.dataset.index,s=document.querySelector('#deep-'+i),p=s.querySelector('p');s.hidden=false;p.textContent='Generating detailed explanation.';document.querySelector('#status').textContent='Generating detailed explanation.';try{const r=await fetch('/api/deep/'+i,{method:'POST'});const j=await r.json();p.textContent=j.deep||j.overview||'Explanation unavailable.';document.querySelector('#status').textContent=j.ok===false?'Detailed explanation unavailable.':'Detailed explanation ready.'}catch(_){p.textContent='Detailed explanation unavailable.';document.querySelector('#status').textContent='Detailed explanation unavailable.'}});</script></body></html>"#);
    h
}
