<script lang="ts">
  import type { LearningSelection, Preferences } from '../learning';
  import { explanations, targetKey, type Target, type Kind } from '../explanations.svelte';
  import PreferencesFields from './PreferencesFields.svelte';
  let { target, kind, base, onApplied, onStatusChange }: { target: Target; kind: Kind; base: LearningSelection; onApplied: () => void; onStatusChange: (message: string) => void } = $props();
  let scope = $state<'section' | 'view'>('section');
  let drafts = $state<Record<string, Preferences>>({});
  const sectionKey = $derived(targetKey(target));
  const viewKey = $derived(`${sectionKey}:${kind}`);
  const draftKey = $derived(scope === 'section' ? sectionKey : viewKey);
  const applied = $derived(scope === 'section' ? explanations.sections[sectionKey] ?? base.section ?? {} : explanations.overrides[viewKey] ?? base.overrides ?? {});
  const draft = $derived(drafts[draftKey] ?? applied);
  let summary: HTMLElement;
  function update(value: Preferences) { drafts[draftKey] = value; }
  function apply() {
    if (scope === 'section') explanations.sections[sectionKey] = JSON.parse(JSON.stringify(draft));
    else explanations.overrides[viewKey] = JSON.parse(JSON.stringify(draft));
    onApplied();
    summary.focus();
    onStatusChange('Preferences applied. Generate or Regenerate to use them.');
  }
  function reset() {
    if (scope === 'section') delete explanations.sections[sectionKey];
    else { delete explanations.overrides[viewKey]; delete explanations.feedback[viewKey]; }
    delete drafts[draftKey];
    onApplied(); summary.focus();
    onStatusChange('This preference scope was reset. Existing explanations are unchanged.');
  }
  function cancel() { delete drafts[draftKey]; summary.focus(); }
</script>
<details class="section-customize">
  <summary bind:this={summary}>Customize this section</summary>
  <p>Temporary preferences stay with this code section. Saving reusable defaults is a separate action in Personalization.</p>
  <label for="customize-scope">Apply to</label>
  <select id="customize-scope" bind:value={scope}><option value="section">This section (all explanation views)</option><option value="view">This explanation view only</option></select>
  <PreferencesFields value={draft} onChange={update} idPrefix="section-customize" labelPrefix="" />
  <button onclick={apply}>Apply temporary override</button>
  <button onclick={reset}>Reset this scope</button>
  <button onclick={cancel}>Cancel</button>
</details>
