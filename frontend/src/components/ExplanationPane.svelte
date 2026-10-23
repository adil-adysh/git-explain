<script lang="ts">
  import { untrack } from 'svelte';
  import type { SectionDetail } from '../api';
  import { inspectLearning, saveLearning, defaultSelection, type LearningSelection, type Policy, type LearningInspection } from '../learning';
  import { explanations, targetKey, kindLabel, type Kind, type Target, type Annotation } from '../explanations.svelte';
  import SectionCustomize from './SectionCustomize.svelte';

  let { detail, mode, sessionId, selection, view, visible = true, disabledReason = '', hideExplanations = false, onSelectionChange, onViewChange, onStatusChange, onAnnotationsChange }:
    { detail: SectionDetail; mode: string; sessionId: string | null; selection: LearningSelection | null; view: string | null; visible?: boolean; disabledReason?: string; hideExplanations?: boolean;
      onSelectionChange: (value: LearningSelection) => void; onViewChange: (kind: Kind) => void; onStatusChange: (message: string) => void; onAnnotationsChange: (annotations: Annotation[]) => void } = $props();
  let activeKind = $state<Kind>('overview');
  let focusedKind = $state<Kind>('overview');
  let loadingPreferences = $state(false);
  let preferencesError = $state('');
  let feedbackInspection = $state<LearningInspection | null>(null);
  let feedbackProfile = $state('');
  let feedbackError = $state('');
  let feedbackSaving = $state(false);
  const target = $derived<Target>({ session: sessionId, mode, detail });
  const kinds = $derived((['overview', 'changes', 'concepts', 'deep'] as Kind[]).filter(kind => detail.capabilities.includes(kind)));
  const viewState = $derived(explanations.read(target, activeKind));
  const busy = $derived(explanations.busy[targetKey(target)] ?? false);
  const inputMatches = $derived(!!selection && viewState.input === JSON.stringify(explanations.selection(selection, target, activeKind)));
  const matching = $derived(inputMatches && explanations.matches(viewState));
  const feedbackKey = $derived(`${targetKey(target)}:${activeKind}`);
  const feedbackChanges = $derived(explanations.feedback[feedbackKey] ?? {});
  let explanationHeading = $state<HTMLHeadingElement>();
  const policyText = (policy: Policy) => {
    const language = { automatic: 'Automatic syntax guidance', explain: 'Explain language features', brief: 'Brief syntax guidance', skip: 'Skip syntax guidance' };
    const focus = { balanced: 'Balanced coverage', syntax: 'Syntax focus', execution: 'Execution focus', reasoning: 'Reasoning focus', architecture: 'Architecture focus' };
    const detail = { concise: 'Concise explanation', balanced: 'Standard detail', detailed: 'Detailed explanation' };
    return `${language[policy.language_help]} · ${focus[policy.focus]} · ${detail[policy.detail]}`;
  };
  $effect(() => {
    const key = visible ? targetKey(target) : null;
    if (key) explanations.activeTarget = key;
    return () => { if (explanations.activeTarget === key) explanations.activeTarget = null; };
  });

  $effect(() => {
    activeKind = kinds.includes(view as Kind) ? view as Kind : 'overview';
    focusedKind = activeKind;
  });
  async function loadPreferences() {
    if (loadingPreferences) return;
    loadingPreferences = true;
    preferencesError = '';
    try { const inspected = await inspectLearning(sessionId); onSelectionChange(inspected.selection ?? defaultSelection()); }
    catch (error) { preferencesError = error instanceof Error ? error.message : 'Learning preferences could not be loaded.'; }
    finally { loadingPreferences = false; }
  }
  // Deliberately independent of activeKind/view. Tab activation is local only.
  $effect(() => {
    const source = detail;
    const base = selection;
    const revision = explanations.policyRevision;
    if (!base) { untrack(() => { void loadPreferences(); }); return; }
    const input = JSON.stringify(base);
    untrack(() => {
      const captured = { session: sessionId, mode, detail: source };
      // Restore every available view on section/preference selection. Activating
      // a tab stays local, including after a reload of previously generated work.
      for (const kind of captured.detail.capabilities as Kind[]) {
        void explanations.check(captured, kind, explanations.selection(JSON.parse(input), captured, kind), revision > 0);
      }
    });
  });
  const annotations = $derived(!hideExplanations && (activeKind === 'changes' || activeKind === 'concepts') ? (viewState.displayed?.annotations ?? []).filter(annotation =>
    annotation.start_line >= 1 && annotation.end_line >= annotation.start_line && detail.valid_annotation_regions.some(region => annotation.start_line >= region.start_line && annotation.end_line <= region.end_line)) : []);
  $effect(() => { onAnnotationsChange(annotations); });

  function activate(kind: Kind) { activeKind = kind; focusedKind = kind; onViewChange(kind); }
  function keydown(event: KeyboardEvent, kind: Kind) {
    const index = kinds.indexOf(kind);
    let next: Kind | undefined;
    if (event.key === 'ArrowRight') next = kinds[(index + 1) % kinds.length];
    else if (event.key === 'ArrowLeft') next = kinds[(index + kinds.length - 1) % kinds.length];
    else if (event.key === 'Home') next = kinds[0];
    else if (event.key === 'End') next = kinds[kinds.length - 1];
    else if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); activate(kind); return; }
    if (next) { event.preventDefault(); focusedKind = next; document.getElementById(`explanation-tab-${next}`)?.focus(); }
  }
  function check() {
    if (selection) void explanations.check(target, activeKind, explanations.selection(selection, target, activeKind), true);
  }
  function generate() {
    if (!selection || disabledReason || busy || viewState.phase === 'checking') return;
    const captured = target;
    const kind = activeKind;
    const pending = JSON.parse(JSON.stringify(explanations.selection(selection, captured, kind))) as LearningSelection;
    void explanations.generate(captured, kind, pending, matching, message => {
      if (explanations.activeTarget === targetKey(captured)) onStatusChange(message);
    });
  }
  function goToSource(event: MouseEvent, line: number) {
    event.preventDefault();
    const source = document.getElementById(`source-line-${line}`) ?? document.querySelector<HTMLTextAreaElement>('[data-source-view] textarea');
    source?.focus(); source?.scrollIntoView({ block: 'center' });
  }
  const canonical = (value: string) => value.trim().toLowerCase();
  function markConcept(term: string, category: 'known' | 'learning') {
    if (!selection || !viewState.pending || !inputMatches) return;
    const policy = viewState.pending.policy;
    const known = policy.known_concepts.filter(value => canonical(value) !== canonical(term));
    const learning = policy.learning_concepts.filter(value => canonical(value) !== canonical(term));
    (category === 'known' ? known : learning).push(term);
    explanations.overrides[feedbackKey] = { ...explanations.overrides[feedbackKey], known_concepts: known, learning_concepts: learning };
    explanations.feedback[feedbackKey] = { ...feedbackChanges, [term]: category };
    check(); onStatusChange('Concept preferences updated temporarily. Use Save feedback to keep them in a learning profile.');
  }
  async function chooseFeedbackProfile() {
    feedbackError = '';
    try {
      feedbackInspection = await inspectLearning(sessionId);
      const selected = selection?.clear_profile ? null : selection?.profile ?? feedbackInspection.config.profile;
      feedbackProfile = selected && feedbackInspection.config.profiles[selected] ? selected : '';
    } catch (error) { feedbackError = error instanceof Error ? error.message : 'Learning profiles could not be loaded.'; }
  }
  async function saveFeedback() {
    if (!feedbackInspection || !feedbackProfile || feedbackSaving) return;
    const capturedKey = feedbackKey;
    const capturedTarget = targetKey(target);
    const changes = { ...feedbackChanges };
    const config = JSON.parse(JSON.stringify(feedbackInspection.config)) as LearningInspection['config'];
    const profile = config.profiles[feedbackProfile];
    if (!profile) return;
    const preferences = profile.preferences;
    for (const [term, category] of Object.entries(changes)) {
      const known = (preferences.known_concepts ?? []).filter(value => canonical(value) !== canonical(term));
      const learning = (preferences.learning_concepts ?? []).filter(value => canonical(value) !== canonical(term));
      (category === 'known' ? known : learning).push(term);
      preferences.known_concepts = known; preferences.learning_concepts = learning;
    }
    feedbackSaving = true; feedbackError = '';
    try {
      const saved = await saveLearning(sessionId, feedbackInspection.revision, config, feedbackInspection.token);
      if (targetKey(target) !== capturedTarget) return;
      feedbackInspection = { ...feedbackInspection, revision: saved.revision, config: saved.config };
      const overrides = explanations.overrides[capturedKey];
      if (overrides) { delete overrides.known_concepts; delete overrides.learning_concepts; }
      delete explanations.feedback[capturedKey];
      explanations.invalidatePolicies();
      onStatusChange('Feedback saved to the learning profile. Existing explanations are unchanged.');
    } catch (error) { feedbackError = error instanceof Error ? error.message : 'Feedback could not be saved. Your temporary preferences are retained.'; }
    finally { feedbackSaving = false; }
  }
</script>

{#if kinds.length === 0}
  <p>This unchanged code section is available for reading.</p>
{:else}
  <section aria-label="Explanations" class="explanation-seam">
    <div role="tablist" aria-label="Explanation views" class="explanation-tabs">
      {#each kinds as kind}
        <button type="button" role="tab" id={`explanation-tab-${kind}`} aria-selected={activeKind === kind} aria-controls="explanation-panel" tabindex={focusedKind === kind ? 0 : -1}
          class="explanation-tab-btn" class:selected={activeKind === kind} onclick={() => activate(kind)} onkeydown={event => keydown(event, kind)}>{kindLabel(kind)}</button>
      {/each}
    </div>
    <div role="tabpanel" id="explanation-panel" aria-labelledby={`explanation-tab-${activeKind}`} tabindex="0" class="explanation-panel-seam">
      <h3 tabindex="-1" bind:this={explanationHeading}>{kindLabel(activeKind)} explanation</h3>
      {#if loadingPreferences}<p>Loading learning preferences…</p>{/if}
      {#if preferencesError}<p>{preferencesError}</p><button onclick={loadPreferences}>Retry learning preferences</button>{/if}
      {#if viewState.phase === 'checking'}<p>Checking saved explanation…</p>{/if}
      {#if viewState.phase === 'generating'}<p>Waiting for {kindLabel(activeKind)} explanation… You can keep reading.</p>{/if}
      {#if viewState.error}<p class="error-text">{viewState.error}</p>{/if}
      {#if viewState.displayed && !hideExplanations}
        <p>{viewState.displayed.cache_hit ? 'Loaded saved explanation.' : 'Generated explanation.'}</p>
        {#if !matching}<p>Preferences or an earlier explanation have changed. Generate again to update this view; the explanation below is unchanged.</p>{/if}
        {#if viewState.displayed.teaching}
          <h4>Purpose</h4><p>{viewState.displayed.teaching.purpose}</p>
          <h4>Walkthrough</h4><ol>{#each viewState.displayed.teaching.walkthrough as step}<li>{step}</li>{/each}</ol>
          <h4>Boundaries</h4><ul>{#each viewState.displayed.teaching.boundaries as boundary}<li>{boundary}</li>{/each}</ul>
          {#if viewState.displayed.teaching.example}<h4>Example</h4><p>{viewState.displayed.teaching.example}</p>{/if}
        {:else}<p>{viewState.displayed.deep ?? viewState.displayed.overview}</p>{/if}
        {#if annotations.length}
          <ol aria-label="Source annotations">
            {#each annotations as annotation, index}
              <li><h4>{annotation.title ?? `Annotation ${index + 1}`}</h4><a href={`#source-line-${annotation.start_line}`} onclick={event => goToSource(event, annotation.start_line)}>Lines {annotation.start_line}–{annotation.end_line}</a><p>{annotation.text}</p>
                {#if annotation.kind === 'concept' && annotation.title}
                  <button disabled={!inputMatches || !viewState.pending || busy} onclick={() => markConcept(annotation.title!, 'known')}>I know this already</button>
                  <button disabled={!inputMatches || !viewState.pending || busy} onclick={() => markConcept(annotation.title!, 'learning')}>Explain this term</button>
                {/if}
              </li>
            {/each}
          </ol>
        {:else if activeKind === 'concepts'}<p>No additional concepts needed.</p>{/if}
      {:else if !viewState.displayed && viewState.phase !== 'checking' && viewState.phase !== 'generating'}<p>No explanation loaded for this view. Generate it when you are ready.</p>{/if}
      {#if hideExplanations}<p>Explanations and annotations are hidden. Change this in Settings to show them.</p>{/if}
      {#if activeKind === 'deep'}<p>Generate Deep uses the matching {mode === 'file' ? 'Overview' : 'Changes'} explanation first. If it is missing, both are generated by this action.</p>{/if}
      <div class="explanation-actions">
        <button disabled={!selection || !!disabledReason} aria-disabled={busy || viewState.phase === 'checking'} onclick={generate}>{matching ? 'Regenerate' : 'Generate'} {kindLabel(activeKind)}</button>
        <button disabled={!selection || busy || viewState.phase === 'checking'} onclick={check}>Check saved explanation</button>
        {#if viewState.displayed}<button onclick={() => explanationHeading?.focus()}>Go to explanation</button>{/if}
      </div>
      {#if busy}<p>A request for this code section is already running.</p>{/if}
      {#if disabledReason}<p>{disabledReason}</p>{/if}
      {#if Object.keys(feedbackChanges).length}
        <section aria-label="Save concept feedback"><h4>Save feedback</h4><p>Temporary concept preferences are not yet saved to a profile.</p>
          <button disabled={feedbackSaving} onclick={chooseFeedbackProfile}>Choose profile for feedback</button>
          {#if feedbackInspection}
            <label for="feedback-profile">Save feedback to learning profile</label><select id="feedback-profile" bind:value={feedbackProfile}><option value="">Choose a profile</option>{#each Object.keys(feedbackInspection.config.profiles) as name}<option value={name}>{name}</option>{/each}</select>
            <button disabled={!feedbackProfile || feedbackSaving} onclick={saveFeedback}>Save feedback to profile</button>
            {#if Object.keys(feedbackInspection.config.profiles).length === 0}<p>Create a learning profile in Personalization first.</p>{/if}
          {/if}
          {#if feedbackError}<p class="error-text">{feedbackError} Your temporary preferences are retained. Choose profile for feedback again to review the latest saved preferences.</p>{/if}
        </section>
      {/if}
      {#if selection || viewState.displayed}
        <details>
          <summary>Explanation preferences</summary>
          {#if viewState.displayed}<p>Used for this explanation: {policyText(viewState.displayed.policy)}</p>{/if}
          {#if viewState.pending && inputMatches}
            {#if !matching}<p>For the next explanation: {policyText(viewState.pending.policy)}</p>{/if}
            <h4>Where these preferences come from</h4>
            <dl>{#each Object.entries(viewState.pending.provenance) as [field, source]}<dt>{field.replaceAll('_', ' ')}</dt><dd>{source}</dd>{/each}</dl>
          {:else}<p>Use Generate or Check saved explanation to see which preferences will apply.</p>{/if}
        </details>
      {/if}
    </div>
    {#if selection}<SectionCustomize {target} kind={activeKind} base={selection} onApplied={check} {onStatusChange} />{/if}
  </section>
{/if}
