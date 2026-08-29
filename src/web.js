<script>(() => {
  const status = document.querySelector('#status'), memory = new WeakMap();
  const esc = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const announce = value => { if (status) status.textContent = value; };
  function draw(article, annotations) {
    const lines = article.querySelector('.source-text').value.split('\n');
    const target = article.querySelector('.source-code');
    let cursor = 0, html = '';
    [...(annotations || [])].sort((a,b) => a.start_line - b.start_line).forEach(annotation => {
      const start = Math.max(cursor, Math.min(lines.length, annotation.start_line - 1));
      const end = Math.max(start, Math.min(lines.length, annotation.end_line));
      if (start > cursor) html += '<pre><code>' + esc(lines.slice(cursor, start).join('\n')) + '</code></pre>';
      if (end > start) html += '<pre><code>' + esc(lines.slice(start, end).join('\n')) + '</code></pre><section class="annotation explanation"><h4>' + esc(annotation.title || annotation.kind) + '</h4><p>' + esc(annotation.text) + '</p></section>';
      cursor = end;
    });
    if (cursor < lines.length) html += '<pre><code>' + esc(lines.slice(cursor).join('\n')) + '</code></pre>';
    target.innerHTML = html || '<pre><code>' + esc(lines.join('\n')) + '</code></pre>';
  }
  function show(article, kind, value) {
    article.querySelectorAll('[data-layer]').forEach(button => button.setAttribute('aria-selected', String(button.dataset.layer === kind)));
    article.querySelector('.layer-summary').textContent = value.overview || value.deep || 'No explanation was returned.';
    article.querySelector('.deep-section').hidden = kind !== 'deep';
    if (kind === 'deep') article.querySelector('.deep').textContent = value.deep || value.overview || '';
    draw(article, kind === 'changes' || kind === 'concepts' ? value.annotations : []);
    memory.get(article).set(kind, value);
    const action = article.querySelector('[data-generate]');
    action.textContent = 'Regenerate explanation';
    action.dataset.generated = 'true';
  }
  function select(article, kind) {
    const states = memory.get(article);
    if (states.has(kind)) return show(article, kind, states.get(kind));
    article.querySelectorAll('[data-layer]').forEach(button => button.setAttribute('aria-selected', String(button.dataset.layer === kind)));
    article.querySelector('.layer-summary').textContent = 'This explanation has not been generated.';
    article.querySelector('.deep-section').hidden = true;
    draw(article, []);
    const action = article.querySelector('[data-generate]');
    action.textContent = 'Generate selected explanation';
    action.dataset.generated = 'false';
  }
  async function load(button, regenerate = false) {
    const article = button.closest('article'), kind = button.dataset.layer, states = memory.get(article);
    if (!regenerate && states.has(kind)) return show(article, kind, states.get(kind));
    const original = button.textContent;
    button.disabled = true; button.textContent = 'Generating…';
    try {
      const body = button.dataset.generation ? JSON.stringify({generation:Number(button.dataset.generation)}) : undefined;
      const value = await (await fetch(regenerate ? button.dataset.regenerateEndpoint : button.dataset.endpoint, {method:'POST', headers:{'Content-Type':'application/json'}, body})).json();
      if (!value.ok) throw Error(value.error);
      show(article, kind, value);
      announce(kind + ' explanation ready.');
    } catch (error) { article.querySelector('.layer-summary').textContent = error.message; announce(error.message); }
    finally { button.disabled = false; button.textContent = original; }
  }
  document.querySelectorAll('article[data-unit-id]').forEach(article => {
    const states = new Map();
    article.querySelectorAll('template[data-layer-state]').forEach(template => {
      states.set(template.dataset.layerState, JSON.parse(template.content.textContent));
    });
    if (states.has('overview')) {
      article.querySelector('[data-generate]').textContent = 'Regenerate explanation';
      article.querySelector('[data-generate]').dataset.generated = 'true';
    }
    memory.set(article, states);
    article.querySelectorAll('[data-layer]').forEach(button => button.onclick = () => select(article, button.dataset.layer));
    article.querySelector('[data-generate]').onclick = event => load(article.querySelector('[data-layer][aria-selected=true]'), event.currentTarget.dataset.generated === 'true');
  });
  document.querySelectorAll('.source-toggle').forEach(button => button.onclick = () => { const article=button.closest('article'),code=article.querySelector('.source-code'),source=article.querySelector('.source-text'),open=source.hidden;source.hidden=!open;code.hidden=open;button.textContent=open?'Show rendered code':'Read code as text'; });
  document.querySelectorAll('.diff-toggle').forEach(button => button.onclick = () => { const target=document.getElementById(button.dataset.target),open=target.hidden;target.hidden=!open;button.setAttribute('aria-expanded',String(open));button.textContent=open?'Hide Git diff':'Show Git diff'; });
  const toggle = document.querySelector('#toggle-explanations');
  if (toggle) toggle.onclick = () => { const hide=toggle.textContent.startsWith('Hide');document.querySelectorAll('.explanation').forEach(node=>node.hidden=hide);toggle.textContent=hide?'Show explanations':'Hide explanations'; };
  const session = document.querySelector('#session-state');
  if (session) setInterval(async () => {
    try {
      const value = await (await fetch('/api/sessions/' + session.dataset.session + '/snapshot')).json();
      if (value.ok && Number(value.generation) > Number(session.dataset.generation)) {
        status.textContent = 'Your Git changes were updated.';
        document.querySelector('#reload-snapshot').hidden = false;
      }
    } catch (_) {}
  }, 5000);
})();</script>
