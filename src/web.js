<script>(() => {
  const status = document.querySelector('#status');
  const memory = new WeakMap();
  const esc = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const announce = value => { if (status) status.textContent = value; };
  const selected = article => article.querySelector('[data-layer][aria-selected=true]');
  const learningUrl = '/api/' + (document.querySelector('#session-state') ? 'sessions/' + document.querySelector('#session-state').dataset.session + '/' : '') + 'learning';
  let learning = {selection:{profile:null,use_repository_defaults:false,session:{},overrides:{}},config:{defaults:{},profiles:{}},revision:null};
  const clone = value => JSON.parse(JSON.stringify(value));
  const preferenceKeys = ['language_help','focus','detail'];
  const readPreferences = element => Object.fromEntries([...element.querySelectorAll('[data-preference]')].filter(node => node.value).map(node => [node.dataset.preference,node.value]));
  const writePreferences = (element,value) => element.querySelectorAll('[data-preference]').forEach(node => node.value = value?.[node.dataset.preference] || '');
  const requestSelection = article => ({...clone(learning.selection),overrides:clone(memory.get(article).overrides.get(selected(article).dataset.layer) || {})});
  const selectionKey = value => JSON.stringify([learning.revision,value]);
  const variantKey = (kind,policy) => kind + ':' + policy;
  function rememberVariant(article, kind, value) {
    const state = memory.get(article);
    const prerequisite = article.querySelector('[data-layer=changes]') ? 'changes' : 'overview';
    if (kind === 'deep') {
      const prior = state.variants.get(variantKey(prerequisite, value.policy_id));
      if (prior && value.prerequisite_id !== prior.result_id) return false;
    }
    state.variants.set(variantKey(kind, value.policy_id), value);
    if (kind === prerequisite) {
      for (const [key, deep] of state.variants) {
        if (key.startsWith('deep:') && deep.policy_id === value.policy_id && deep.prerequisite_id !== value.result_id) {
          state.variants.delete(key);
          if (state.values.get('deep')?.result_id === deep.result_id) state.values.delete('deep');
        }
      }
    }
    return true;
  }
  async function resolveSettings(article, restore = true) {
    const state = memory.get(article), kind = selected(article).dataset.layer, selection = requestSelection(article), requestKey = selectionKey(selection);
    state.selectionKey = requestKey;
    article.querySelector('.policy-status').textContent = 'Pending settings: resolving…';
    try {
      const response = await fetch(learningUrl + '/resolve', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({language:article.dataset.language,selection})});
      const resolved = await response.json();
      if (!resolved.ok || state.selectionKey !== requestKey || selected(article).dataset.layer !== kind) return;
      state.policy = resolved;
      state.policies.set(requestKey,resolved);
      const cached = state.variants.get(variantKey(kind,resolved.policy_id));
      if (cached) state.values.set(kind,cached);
      else if (restore) {
        const restored = await (await fetch(learningUrl + '/results', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({language:article.dataset.language,learning:selection,unit_id:article.dataset.unitId,kind})})).json();
        if (state.selectionKey !== requestKey || selected(article).dataset.layer !== kind) return;
        if (restored.ok && restored.policy_id === resolved.policy_id && (restored.result || restored.overview !== undefined || restored.deep)) {
          const value = restored.result || restored;
          value.policy_id = resolved.policy_id;
          value.policy = value.policy || resolved.policy;
          if (rememberVariant(article,kind,value)) state.values.set(kind,value);
        }
      }
      render(article);
    } catch (_) { article.querySelector('.policy-status').textContent = 'Pending settings could not be resolved. Try again.'; }
  }
  async function preloadViews(article) {
    const state = memory.get(article);
    await Promise.all([...article.querySelectorAll('[data-layer]')].filter(tab => tab.dataset.layer !== selected(article).dataset.layer).map(async tab => {
      const kind = tab.dataset.layer;
      const selection = {...clone(learning.selection),overrides:clone(state.overrides.get(kind) || {})};
      const key = selectionKey(selection);
      try {
        const resolved = await (await fetch(learningUrl + '/resolve',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({language:article.dataset.language,selection})})).json();
        if (!resolved.ok) return;
        state.policies.set(key,resolved);
        const response = await (await fetch(learningUrl + '/results',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({language:article.dataset.language,learning:selection,unit_id:article.dataset.unitId,kind})})).json();
        if (response.ok && response.policy_id === resolved.policy_id && (response.result || response.overview !== undefined || response.deep)) {
          const value = response.result || response;
          value.policy_id = resolved.policy_id;
          value.policy = value.policy || resolved.policy;
          if (!rememberVariant(article,kind,value)) return;
          const current = {...clone(learning.selection),overrides:clone(state.overrides.get(kind) || {})};
          if (selectionKey(current) === key) state.values.set(kind,value);
        }
        if (selected(article).dataset.layer === kind) { state.policy = state.policies.get(selectionKey(requestSelection(article))); render(article); }
      } catch (_) {}
    }));
  }
  const refreshSettings = () => document.querySelectorAll('article[data-unit-id]').forEach(article => { resolveSettings(article); preloadViews(article); });
  function fillProfiles() {
    const select = document.querySelector('#learning-profile');
    select.innerHTML = '<option value="">User defaults</option>' + Object.keys(learning.config.profiles || {}).map(name => '<option value="' + esc(name) + '">' + esc(name) + '</option>').join('');
    select.value = learning.selection.profile || ''; select.disabled = false;
    document.querySelector('#learning-repository').checked = learning.selection.use_repository_defaults;
    writePreferences(document.querySelector('#learning-settings fieldset'),learning.selection.session);
    fillEditor();
  }
  const listKeys = ['known_languages','learning_languages','known_frameworks','learning_frameworks','known_domains','learning_domains','known_concepts','learning_concepts'];
  const profileTarget = () => document.querySelector('#learning-edit-target').value === 'profile';
  function editorPreferences(create = false) {
    let target = learning.config.defaults;
    if (profileTarget()) {
      const name = learning.selection.profile;
      if (!name || !learning.config.profiles[name]) return null;
      const profile = learning.config.profiles[name], language = document.querySelector('#learning-language').value.trim().toLowerCase();
      if (language) { if (create) profile.languages ||= {}; if (create) profile.languages[language] ||= {}; return profile.languages?.[language] || {}; }
      target = profile.preferences;
    }
    return target || {};
  }
  function fillEditor() {
    document.querySelector('#learning-language').disabled = !profileTarget();
    const value = editorPreferences() || {};
    writePreferences(document.querySelector('#learning-editor'),value);
    document.querySelector('#learning-lists').innerHTML = listKeys.map(key => {
      const inherited = value[key] == null;
      const label = esc(key.replaceAll('_',' '));
      return '<div><label><input type="checkbox" data-list-inherit="' + key + '"' + (inherited ? ' checked' : '') + '> Inherit ' + label + '</label>' +
        '<label>' + label + '<input data-list="' + key + '" value="' + esc((value[key] || []).join(', ')) + '" placeholder="Comma-separated terms"></label></div>';
    }).join('');
  }
  function serializeLists(target) {
    listKeys.forEach(key => {
      const inherit = document.querySelector('[data-list-inherit="' + key + '"]');
      const input = document.querySelector('[data-list="' + key + '"]');
      if (inherit?.checked) {
        delete target[key];
      } else if (input) {
        target[key] = input.value.split(',').map(value => value.trim()).filter(Boolean);
      }
    });
  }
  function applyEditor() {
    const target = editorPreferences(true);
    if (!target) throw Error('Select a learning profile first.');
    applyEditorPreferences(target);
  }
  function applyEditorPreferences(target) {
    preferenceKeys.forEach(key => delete target[key]);
    Object.assign(target,readPreferences(document.querySelector('#learning-editor')));
    serializeLists(target);
  }
  function profileDraft() {
    const source = profileTarget() && learning.config.profiles[learning.selection.profile];
    const draft = source ? clone(source) : {preferences:clone(editorPreferences() || {}),languages:{}};
    draft.preferences ||= {};
    draft.languages ||= {};
    const language = source && document.querySelector('#learning-language').value.trim().toLowerCase();
    const target = language ? (draft.languages[language] ||= {}) : draft.preferences;
    applyEditorPreferences(target);
    return draft;
  }
  async function saveLearning() {
    const response = await fetch(learningUrl,{method:'PUT',headers:{'Content-Type':'application/json','x-git-explain-learning-token':learning.token},body:JSON.stringify({revision:learning.revision,config:learning.config})});
    const value = await response.json();
    if (!response.ok || !value.ok) throw Error(value.error || (response.status === 409 ? 'Settings changed elsewhere. Reload before saving again.' : 'Could not save learning settings.'));
    learning.revision = value.revision; learning.config = value.config || learning.config;
    document.querySelector('#learning-status').textContent = 'Saved learning preferences.';
    fillProfiles(); refreshSettings();
  }
  function learningError(error) { document.querySelector('#learning-status').textContent = error.message; }
  async function initializeLearning() {
    try {
      const value = await (await fetch(learningUrl)).json();
      if (!value.ok) throw Error(value.error || 'Learning settings unavailable.');
      learning = value; learning.ready = true; if (!learning.selection.profile && !learning.selection.clear_profile) learning.selection.profile = learning.config.profile || null; learning.selection.session ||= {}; learning.selection.overrides ||= {}; learning.config.profiles ||= {}; learning.config.defaults ||= {};
      fillProfiles(); document.querySelector('#learning-status').textContent = 'Active learning profile: ' + (learning.selection.profile || 'User defaults'); refreshSettings();
    } catch(error) { learningError(error); }
  }
  function draw(article, annotations) {
    const lines = article.querySelector('.source-text').value.split('\n');
    const target = article.querySelector('.source-code');
    let cursor = 0, html = '';
    [...(annotations || [])].sort((a,b) => a.start_line - b.start_line).forEach(annotation => {
      const start = Math.max(0, Math.min(lines.length, annotation.start_line - 1));
      const end = Math.max(start, Math.min(lines.length, annotation.end_line));
      if (start > cursor) html += '<pre><code>' + esc(lines.slice(cursor, start).join('\n')) + '</code></pre>';
      if (end > cursor) html += '<pre><code>' + esc(lines.slice(Math.max(cursor, start), end).join('\n')) + '</code></pre>';
      if (end > start) html += '<section class="annotation explanation"><h4>' + esc(annotation.title || annotation.kind) + '</h4><p>' + esc(annotation.text) + '</p>' + (annotation.title ? '<button data-feedback="known" data-term="' + esc(annotation.title) + '">I know this already</button><button data-feedback="learning" data-term="' + esc(annotation.title) + '">Explain this term</button>' : '') + '</section>';
      cursor = Math.max(cursor, end);
    });
    if (cursor < lines.length) html += '<pre><code>' + esc(lines.slice(cursor).join('\n')) + '</code></pre>';
    target.innerHTML = html || '<pre><code>' + esc(lines.join('\n')) + '</code></pre>';
  }
  function render(article) {
    const state = memory.get(article), tab = selected(article), kind = tab.dataset.layer;
    state.policy = state.policies.get(selectionKey(requestSelection(article)));
    const matchingVariant = state.policy && state.variants.get(variantKey(kind,state.policy.policy_id));
    if (matchingVariant) state.values.set(kind,matchingVariant);
    const value = state.values.get(kind), error = state.errors.get(kind + ':' + selectionKey(requestSelection(article)));
    article.querySelector('.layer-summary').textContent = error?.message || (kind === 'deep' && value ? 'Detailed explanation is shown below.' : value?.overview) || 'This explanation has not been generated.';
    const panel = article.querySelector('[role=tabpanel]');
    panel.setAttribute('aria-labelledby', tab.id);
    panel.setAttribute('aria-busy', String(state.pending === kind));
    article.querySelector('.deep-section').hidden = kind !== 'deep' || !value;
    const teaching = kind === 'deep' && value?.teaching;
    article.querySelector('.deep').innerHTML = teaching ? '<h5>Purpose</h5><p>' + esc(teaching.purpose) + '</p><h5>Walkthrough</h5><ol>' + (teaching.walkthrough || []).map(text => '<li>' + esc(text) + '</li>').join('') + '</ol><h5>Boundaries</h5><ul>' + (teaching.boundaries || []).map(text => '<li>' + esc(text) + '</li>').join('') + '</ul>' + (teaching.example ? '<h5>Example</h5><p>' + esc(teaching.example) + '</p>' : '') : '';
    const policy = state.policy?.policy;
    const actualPolicy = value?.policy;
    const actualSettings = actualPolicy ? [actualPolicy.language_help,actualPolicy.focus,actualPolicy.detail].filter(Boolean).join(' · ') : '';
    article.querySelector('.policy-status').textContent = 'Pending settings: ' + (policy ? [policy.language_help,policy.focus,policy.detail].filter(Boolean).join(' · ') : 'inherited') + '. ' + (value ? value.policy_id === state.policy?.policy_id ? 'Showing explanation generated with these settings.' : 'Showing an explanation generated with different settings.' + (actualSettings ? ' Displayed settings: ' + actualSettings + '.' : '') : 'Press Generate to apply.') + (state.policy?.provenance ? ' Sources: ' + JSON.stringify(state.policy.provenance) : '');
    writePreferences(article.querySelector('.view-learning'),state.overrides.get(kind) || {});
    draw(article, kind === 'changes' || kind === 'concepts' ? value?.annotations : []);
    const action = article.querySelector('[data-generate]');
    action.disabled = Boolean(state.pending) || !learning.ready || !state.policy;
    const matching = Boolean(value) && value.policy_id === state.policy?.policy_id;
    action.textContent = state.pending ? (state.pending === kind ? 'Generating…' : 'Waiting for current explanation…') : matching ? 'Regenerate explanation' : 'Generate selected explanation';
    action.dataset.generated = String(matching);
    article.querySelector('.generation-status').textContent = state.pending ? `Generating ${state.pendingLabel}. You can keep reading or switch views.` : '';
    article.querySelector('[data-reload]').hidden = !error?.stale;
  }
  function select(article, kind) {
    article.querySelectorAll('[data-layer]').forEach(tab => {
      const active = tab.dataset.layer === kind;
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
    });
    render(article);
  }
  async function load(article) {
    const state = memory.get(article);
    if (state.pending) return;
    const tab = selected(article), kind = tab.dataset.layer;
    const selection = requestSelection(article);
    const capturedSelectionKey = selectionKey(selection);
    const requestPolicy = state.policy?.policy_id;
    const previousResult = state.variants.get(variantKey(kind,requestPolicy)) || (state.values.get(kind)?.policy_id === requestPolicy ? state.values.get(kind) : null);
    const errorKey = kind + ':' + capturedSelectionKey;
    const regenerate = state.values.get(kind)?.policy_id === requestPolicy;
    state.pending = kind;
    state.pendingLabel = tab.textContent;
    state.errors.delete(errorKey);
    render(article);
    try {
      const body = JSON.stringify({...(tab.dataset.generation ? {generation:Number(tab.dataset.generation)} : {}),learning:selection});
      const response = await fetch(regenerate ? tab.dataset.regenerateEndpoint : tab.dataset.endpoint, {method:'POST', headers:{'Content-Type':'application/json'}, body});
      if (!response.ok) throw Error(`Could not generate this explanation (HTTP ${response.status}). Try again.`);
      const value = await response.json();
      if (!value.ok) {
        const error = Error(value.stale ? 'Your Git changes were updated. Reload Git changes and try again.' : value.error || 'Could not generate this explanation. Try again.');
        error.stale = Boolean(value.stale);
        throw error;
      }
      const retained = rememberVariant(article,kind,value);
      if (retained && selectionKey(requestSelection(article)) === capturedSelectionKey) state.values.set(kind, value);
      announce(`${state.pendingLabel} explanation ready.`);
    } catch (error) {
      const message = error instanceof TypeError || error instanceof SyntaxError ? 'Could not reach the explanation service or read its response. Check the connection and try again.' : error.message;
      state.errors.set(errorKey, {message, stale:Boolean(error.stale)});
      announce(message);
    } finally {
      state.pending = null;
      render(article);
    }
  }
  document.querySelectorAll('article[data-unit-id]').forEach(article => {
    const values = new Map();
    article.querySelectorAll('template[data-layer-state]').forEach(template => {
      values.set(template.dataset.layerState, JSON.parse(template.content.textContent));
    });
    const variants = new Map([...values].filter(([,value]) => value.policy_id).map(([kind,value]) => [variantKey(kind,value.policy_id),value]));
    memory.set(article, {values, variants, policies:new Map(), overrides:new Map(), feedbackCategories:new Map(), errors:new Map(), pending:null});
    article.querySelector('.view-learning').onchange = event => { if (!event.target.matches('[data-preference]')) return; memory.get(article).overrides.set(selected(article).dataset.layer,readPreferences(article.querySelector('.view-learning'))); resolveSettings(article); };
    article.querySelector('[data-reset-view]').onclick = () => { const state = memory.get(article), kind = selected(article).dataset.layer; state.overrides.delete(kind); state.feedbackCategories.delete(kind); resolveSettings(article); };
    article.onclick = event => { const button = event.target.closest('[data-feedback]'); if (!button) return; const state = memory.get(article), kind = selected(article).dataset.layer, overrides = state.overrides.get(kind) || {}; const category = button.dataset.feedback; state.feedbackCategories.set(kind,category); const key = category + '_concepts'; const opposite = category === 'known' ? 'learning_concepts' : 'known_concepts'; const currentKey = overrides[key] || state.policy?.policy?.[key] || []; const currentOpposite = overrides[opposite] || state.policy?.policy?.[opposite] || []; const termLower = button.dataset.term.toLowerCase(); overrides[opposite] = currentOpposite.filter(term => String(term).toLowerCase() !== termLower); overrides[key] = [...currentKey.filter(term => String(term).toLowerCase() !== termLower),button.dataset.term]; state.overrides.set(kind,overrides); article.querySelector('.feedback-term').value = button.dataset.term; resolveSettings(article); };
    article.querySelector('[data-save-feedback]').onclick = async () => { try { const name = learning.selection.profile; if (!name) throw Error('Select a profile before saving feedback.'); const state = memory.get(article), kind = selected(article).dataset.layer; const category = state.feedbackCategories.get(kind); const term = article.querySelector('.feedback-term').value.trim(); if (!category || !term) throw Error('Choose a concept and enter its label before saving.'); const prefs = learning.config.profiles[name].preferences ||= {}; const targetKey = category + '_concepts'; const oppositeKey = category === 'known' ? 'learning_concepts' : 'known_concepts'; const termLower = term.toLowerCase(); prefs[oppositeKey] = (prefs[oppositeKey] || []).filter(item => String(item).trim().toLowerCase() !== termLower); prefs[targetKey] = [...(prefs[targetKey] || []).filter(item => String(item).trim().toLowerCase() !== termLower),term]; await saveLearning(); const overrides = state.overrides.get(kind); if (overrides) { delete overrides.known_concepts; delete overrides.learning_concepts; } state.feedbackCategories.delete(kind); resolveSettings(article); } catch(error) { learningError(error); } };
    const tabs = [...article.querySelectorAll('[data-layer]')];
    tabs.forEach((tab, index) => {
      tab.onclick = () => select(article, tab.dataset.layer);
      tab.onkeydown = event => {
        let next;
        if (event.key === 'ArrowRight') next = (index + 1) % tabs.length;
        if (event.key === 'ArrowLeft') next = (index + tabs.length - 1) % tabs.length;
        if (event.key === 'Home') next = 0;
        if (event.key === 'End') next = tabs.length - 1;
        if (next === undefined) return;
        event.preventDefault();
        select(article, tabs[next].dataset.layer);
        tabs[next].focus();
      };
    });
    article.querySelector('[data-generate]').onclick = () => load(article);
    article.querySelector('[data-reload]').onclick = () => location.reload();
    render(article);
  });
  document.querySelector('#learning-profile').onchange = event => { learning.selection.profile = event.target.value || null; learning.selection.clear_profile = !event.target.value; document.querySelector('#learning-status').textContent = 'Active learning profile: ' + (learning.selection.profile || 'User defaults'); fillEditor(); refreshSettings(); };
  document.querySelector('#learning-repository').onchange = event => { learning.selection.use_repository_defaults = event.target.checked; refreshSettings(); };
  document.querySelector('#learning-settings fieldset').onchange = () => { learning.selection.session = readPreferences(document.querySelector('#learning-settings fieldset')); refreshSettings(); };
  document.querySelector('#learning-reset').onclick = () => { learning.selection.session = {}; document.querySelectorAll('article[data-unit-id]').forEach(article => { const state = memory.get(article); state.overrides.clear(); state.feedbackCategories?.clear(); }); writePreferences(document.querySelector('#learning-settings fieldset'),{}); refreshSettings(); };
  document.querySelector('#learning-edit-target').onchange = fillEditor;
  document.querySelector('#learning-language').onchange = fillEditor;
  const uncheckInherit = event => {
    const input = event.target.closest('[data-list]');
    if (!input) return;
    const inherit = document.querySelector('[data-list-inherit="' + input.dataset.list + '"]');
    if (inherit) inherit.checked = false;
  };
  const listContainer = document.querySelector('#learning-lists');
  listContainer.oninput = uncheckInherit;
  listContainer.onchange = uncheckInherit;
  document.querySelector('#learning-save').onclick = async () => { try { applyEditor(); await saveLearning(); } catch(error) { learningError(error); } };
  document.querySelector('#learning-save-as').onclick = async () => { try { const name = document.querySelector('#learning-name').value.trim(); if (!name) throw Error('Enter a profile name.'); if (learning.config.profiles[name]) throw Error('That profile already exists. Choose another name.'); learning.config.profiles[name] = profileDraft(); learning.selection.profile = name; learning.selection.clear_profile = false; await saveLearning(); } catch(error) { learningError(error); } };
  document.querySelector('#learning-default').onclick = async () => { try { learning.config.profile = learning.selection.profile; await saveLearning(); } catch(error) { learningError(error); } };
  initializeLearning();
  document.querySelectorAll('.source-toggle').forEach(button => button.onclick = () => {
    const article = button.closest('article'), code = article.querySelector('.source-code'), source = article.querySelector('.source-text'), open = source.hidden;
    source.hidden = !open;
    code.hidden = open;
    button.textContent = open ? 'Show rendered code' : 'Read code as text';
    button.setAttribute('aria-pressed', String(open));
  });
  document.querySelectorAll('.diff-toggle').forEach(button => button.onclick = () => {
    const target = document.getElementById(button.dataset.target), open = target.hidden;
    target.hidden = !open;
    button.setAttribute('aria-expanded', String(open));
    button.textContent = open ? 'Hide Git diff' : 'Show Git diff';
  });
  const toggle = document.querySelector('#toggle-explanations');
  if (toggle) toggle.onclick = () => {
    const hide = document.body.classList.toggle('explanations-hidden');
    toggle.textContent = hide ? 'Show explanations' : 'Hide explanations';
    toggle.setAttribute('aria-pressed', String(hide));
  };
  const filter = document.querySelector('#code-filter');
  if (filter) {
    const files = [...document.querySelectorAll('[data-file-path]')];
    const update = () => {
      const query = filter.value.trim().toLowerCase();
      let visibleFiles = 0, visibleSections = 0;
      files.forEach(file => {
        const pathMatches = file.dataset.filePath.toLowerCase().includes(query);
        const articles = [...file.querySelectorAll('article[data-unit-id]')];
        articles.forEach(article => {
          article.hidden = !pathMatches && !article.querySelector('h3').textContent.toLowerCase().includes(query);
          if (!article.hidden) visibleSections++;
        });
        file.hidden = !pathMatches && !articles.some(article => !article.hidden);
        if (!file.hidden) visibleFiles++;
        const link = document.querySelector(`a[href="#${file.id}"]`);
        if (link) link.closest('li').hidden = file.hidden;
      });
      document.querySelector('#filter-empty').hidden = visibleFiles !== 0 || !query;
      document.querySelector('#filter-status').textContent = `${visibleFiles} files · ${visibleSections} code sections`;
      document.querySelector('#clear-filter').disabled = !filter.value;
    };
    filter.oninput = update;
    document.querySelector('#clear-filter').onclick = () => { filter.value = ''; update(); filter.focus(); };
    update();
  }
  const reload = document.querySelector('#reload-snapshot');
  if (reload) reload.onclick = () => location.reload();
  const session = document.querySelector('#session-state');
  if (session) setInterval(async () => {
    try {
      const value = await (await fetch('/api/sessions/' + session.dataset.session + '/snapshot')).json();
      if (value.ok && Number(value.generation) > Number(session.dataset.generation)) {
        announce('Your Git changes were updated. Reload Git changes to read the latest code.');
        reload.hidden = false;
      }
    } catch (_) {}
  }, 5000);
})();</script>
