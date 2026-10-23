# Explanation UX Contract

This document defines the durable product and implementation contract for explanation generation. It is intentionally narrower than the general architecture documentation.

For the proposed reader-preference model, scenarios, and delivery sequence,
see [Personalized explanation and learning design](personalized-learning-design.md).
That proposal does not change the implemented controls or the invariants here.

## Product constraints

- Local inference is the primary deployment target, including machines with approximately 8 GiB VRAM.
- Run at most one model inference at a time by default.
- Never infer on page load or tab selection. A visible Generate or Regenerate action is the only browser trigger.
- Cache and restore generated results so switching views does not repeat inference.
- Keep the HTTP server on loopback and preserve daemon control-token checks.

## Explanation views

| View | User question | Diff-mode scope | File-mode scope | Source annotations |
| --- | --- | --- | --- | --- |
| Overview | What does this code do? | Complete active code section | Complete code section | None |
| Changes | What changed in Git? | Deterministic changed regions | Not shown | Change annotations |
| Concepts | What knowledge helps me understand this code? | Deterministic changed regions, with the complete active section as context | Complete code section | Zero or more bounded concept annotations |
| Deep | Teach me this in detail | Active changed code section plus prior explanation when available | Complete code section plus prior explanation when available | None |

A Concepts request is per code section, not per individual concept. It may return several useful concepts up to the configured annotation limit. Trivial code may return none.

## Interaction model

1. The server renders the available view tabs and any already-generated state.
2. Selecting a tab only displays its current state. It does not call an endpoint.
3. The action button says Generate when that view has no result and Regenerate when it does.
4. Generation updates only the requested view. If the reader switches tabs while it runs, completion preserves their selected tab.
5. Returning to a generated view displays its in-memory or server-restored result immediately.
6. Changes and Concepts redraw annotations on the same source container. The source remains spatially stable.

Use accessible tab semantics, selected state, keyboard focus, explicit action labels, and status announcements. Show clear not-generated, generating, ready, and error states.

Keep the Generate action disabled for a code section while its request runs;
tabs remain available for reading. Left/Right arrows and Home/End move between
tabs without making requests. Keep errors scoped to their requested view and
offer a reload action when the Git changes are outdated. Hide explanations
applies to annotations created after the toggle, and showing explanations
must not reveal an inactive Deep view.

File links and local filtering by path or code section name are navigation.
They must not generate explanations or discard generated results. Display a
clear no-match message and provide a way to clear the filter.

## Implementation invariants

- `ExplanationKind::{Overview, Changes, Concepts, Deep}` is part of request identity.
- Each kind has an independent state slot and cache identity in both file and diff modes.
- Snapshot and change cache namespaces remain disjoint.
- Deterministic regions are selected by the application. The model cannot return arbitrary source line numbers.
- Render exactly one active source view per code section. A hidden plain-text representation may support accessibility and client-side redraw, but must not become a second visible source panel.
- Use one shared executable browser script. Server-rendered inert state is allowed; dynamically injected concept panels and per-mode scripts are not.
- Regeneration is scoped to the selected kind. If a prerequisite result changes, invalidate only genuinely dependent state such as Deep.

## User-facing language

Use terms people encounter in Git and source code: file, code section, working tree, commit, range, changes, previous path, source, and Git diff.

Do not expose implementation vocabulary such as semantic unit, snapshot, provider, inference, cache key, hydration, or deterministic region. “Overview,” “Concepts,” and “Deep” are product labels, not implementation details.

Exclude unsupported files from file lists and rendered sections. Supported files with no explainable code sections may remain visible with a clear empty state.

## Verification checklist

- Request tests prove the correct `ExplanationKind` and region scope for every mode.
- Cache tests prove all four kinds have different keys.
- Renderer tests prove one source container per code section and server-rendered controls/state.
- Source inspection or tests prove there are no injected concept panels and only one executable UI script.
- Browser tests prove selecting a tab makes no generation request, the action generates only the selected view, cached views switch without inference, and Changes/Concepts replace annotations on the shared source.
- `task check` and `git diff --check` pass.
