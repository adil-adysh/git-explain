import type { SectionDetail } from './api';
import { learningBase, resolveLearning, type LearningSelection, type Policy, type ResolvedLearning, type Preferences } from './learning';

export type Kind = 'overview' | 'changes' | 'concepts' | 'deep';
export const kindLabel = (kind: Kind) => kind[0].toUpperCase() + kind.slice(1);
export interface Annotation { title?: string; start_line: number; end_line: number; kind: string; text: string }
export interface ExplanationResult {
  ok: boolean; mode: Kind; cache_hit: boolean; overview: string; deep?: string;
  teaching?: { purpose: string; walkthrough: string[]; boundaries: string[]; example?: string };
  annotations: Annotation[]; policy: Policy; policy_id: string; result_id: string; prerequisite_id?: string | null;
}
export interface Target { session: string | null; mode: string; detail: SectionDetail }
export interface ViewState {
  phase: 'idle' | 'checking' | 'generating' | 'ready' | 'error';
  error?: string; input?: string; pending?: ResolvedLearning; displayed?: ExplanationResult;
  variants: Record<string, ExplanationResult>; invalidated?: boolean;
}
export const targetKey = (target: Target) => `${target.session ?? 'standalone'}:${target.detail.snapshot_id}:${target.detail.section_id}`;
const viewKey = (target: Target, kind: Kind) => `${targetKey(target)}:${kind}`;

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  const value = await response.json();
  if (!response.ok || value.ok === false) throw new Error(value.error ?? `This explanation request failed (${response.status}).`);
  return value as T;
}

/** Per-page request association outlives the selected Reader. Navigation never
 * aborts an explicit generation or turns it into an operation on another section. */
export class Explanations {
  activeTarget = $state<string | null>(null);
  policyRevision = $state(0);
  views = $state<Record<string, ViewState>>({});
  busy = $state<Record<string, boolean>>({});
  sections = $state<Record<string, Preferences>>({});
  overrides = $state<Record<string, Preferences>>({});
  feedback = $state<Record<string, Record<string, 'known' | 'learning'>>>({});
  private requests = new Map<string, number>();

  invalidatePolicies() {
    for (const [key, view] of Object.entries(this.views)) {
      view.pending = undefined; view.input = undefined;
      this.requests.set(key, (this.requests.get(key) ?? 0) + 1);
      if (view.phase === 'checking') view.phase = 'idle';
    }
    this.policyRevision++;
  }

  read(target: Target, kind: Kind): ViewState {
    return this.views[viewKey(target, kind)] ?? { phase: 'idle', variants: {} };
  }

  state(target: Target, kind: Kind): ViewState {
    const key = viewKey(target, kind);
    if (!this.views[key] && Object.keys(this.views).length >= 256) {
      const oldest = Object.keys(this.views).find(candidate => !candidate.startsWith(`${this.activeTarget}:`) && !this.busy[candidate.slice(0, candidate.lastIndexOf(':'))] && !['checking', 'generating'].includes(this.views[candidate].phase));
      if (oldest) { delete this.views[oldest]; this.requests.delete(oldest); }
    }
    if (!this.views[key]) this.views[key] = { phase: 'idle', variants: {} };
    // Read back through $state so async updates use Svelte's proxy, rather than
    // mutating the unproxied assignment value after a request completes.
    return this.views[key];
  }
  selection(base: LearningSelection, target: Target, kind: Kind): LearningSelection {
    return { ...base, section: this.sections[targetKey(target)] ?? base.section ?? {}, overrides: this.overrides[viewKey(target, kind)] ?? base.overrides ?? {} };
  }
  matches(state: ViewState): boolean {
    return !!state.pending && !!state.displayed && !state.invalidated && state.pending.policy_id === state.displayed.policy_id;
  }
  private remember(target: Target, kind: Kind, result: ExplanationResult): void {
    const state = this.state(target, kind);
    const previous = state.variants[result.policy_id];
    state.variants[result.policy_id] = result;
    state.displayed = result;
    state.invalidated = false;
    if (kind === (target.mode === 'file' ? 'overview' : 'changes') && previous && previous.result_id !== result.result_id) {
      const deep = this.state(target, 'deep');
      for (const [policy, variant] of Object.entries(deep.variants)) {
        if (variant.prerequisite_id === previous.result_id) delete deep.variants[policy];
      }
      if (deep.displayed?.prerequisite_id === previous.result_id) deep.invalidated = true;
    }
  }
  private async saved(target: Target, kind: Kind, selection: LearningSelection): Promise<ExplanationResult | null> {
    const result = await post<ExplanationResult & { result?: null }>(`${learningBase(target.session)}/results`, { unit_id: target.detail.generation_id, language: target.detail.language, kind, learning: selection });
    if (result.result === null) return null;
    if (result.mode !== kind || !result.result_id || !result.policy_id) throw new Error('This saved explanation could not be verified. Try generating it again.');
    return result;
  }
  async check(target: Target, kind: Kind, selection: LearningSelection, force = false): Promise<void> {
    const state = this.state(target, kind);
    const input = JSON.stringify(selection);
    if (!force && state.input === input && state.pending && state.phase !== 'error') return;
    if (this.busy[targetKey(target)]) return;
    const key = viewKey(target, kind);
    const request = (this.requests.get(key) ?? 0) + 1;
    this.requests.set(key, request);
    state.phase = 'checking';
    state.error = undefined;
    state.input = input;
    try {
      const pending = await resolveLearning(target.session, target.detail.language, selection);
      const result = await this.saved(target, kind, selection);
      if (this.requests.get(key) !== request) return;
      state.pending = pending;
      if (result) this.remember(target, kind, result);
      else if (state.displayed?.policy_id === pending.policy_id) state.invalidated = true;
      state.phase = 'ready';
    } catch (error) {
      if (this.requests.get(key) !== request) return;
      state.phase = 'error';
      state.error = error instanceof Error ? error.message : 'Saved explanations are unavailable. Try again.';
    }
  }
  private async run(target: Target, kind: Kind, selection: LearningSelection, regenerate: boolean): Promise<ExplanationResult> {
    const root = target.session ? `/api/sessions/${encodeURIComponent(target.session)}/units` : target.mode === 'file' ? '/api/file-units' : '/api/units';
    const action = kind === 'overview' ? (target.mode === 'file' ? (regenerate ? 'regenerate' : 'explain') : `overview${regenerate ? '/regenerate' : ''}`)
      : kind === 'changes' ? (regenerate ? 'regenerate' : 'explain') : `${kind}${regenerate ? '/regenerate' : ''}`;
    const result = await post<ExplanationResult>(`${root}/${encodeURIComponent(target.detail.generation_id)}/${action}`, { generation: target.detail.generation, learning: selection });
    if (result.mode !== kind || !result.result_id || !result.policy_id) throw new Error('The returned explanation did not match this view. Try again.');
    this.remember(target, kind, result);
    return result;
  }
  async generate(target: Target, kind: Kind, selection: LearningSelection, regenerate: boolean, notify: (message: string) => void): Promise<void> {
    const key = targetKey(target);
    if (this.busy[key]) return;
    this.busy[key] = true;
    const state = this.state(target, kind);
    this.requests.set(viewKey(target, kind), (this.requests.get(viewKey(target, kind)) ?? 0) + 1);
    state.phase = 'generating';
    state.error = undefined;
    notify(`Waiting for ${kindLabel(kind)} explanation.`);
    const revision = this.policyRevision;
    try {
      const pending = await resolveLearning(target.session, target.detail.language, selection);
      if (revision === this.policyRevision) { state.pending = pending; state.input = JSON.stringify(selection); }
      if (kind === 'deep') {
        const prerequisite = target.mode === 'file' ? 'overview' : 'changes';
        const saved = await this.saved(target, prerequisite, selection);
        if (saved) this.remember(target, prerequisite, saved);
        else {
          notify(`Preparing ${kindLabel(prerequisite)} before Deep.`);
          await this.run(target, prerequisite, selection, false);
        }
      }
      await this.run(target, kind, selection, regenerate);
      state.phase = 'ready';
      notify(`${kindLabel(kind)} explanation ready.`);
    } catch (error) {
      state.phase = 'error';
      state.error = error instanceof Error ? error.message : 'The explanation failed. Try again.';
      notify(`${kindLabel(kind)} explanation failed: ${state.error}`);
    } finally {
      this.busy[key] = false;
    }
  }
}

export const explanations = new Explanations();
