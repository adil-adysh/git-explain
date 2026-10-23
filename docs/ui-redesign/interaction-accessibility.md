# Interaction, focus and accessibility contract

> **Status:** Active default contract (`/` standalone, `/sessions/{id}` daemon; `/workspace` aliases). Legacy fallback retained at `/legacy` and `/sessions/{id}/legacy` until stabilization. Manual NVDA and native zoom checks are explicitly deferred, not passed. Applies to every screen in [screens.md](screens.md), every responsive layout and both standalone and daemon modes.

## Non-negotiable rules

1. **Navigation is not generation.** File/section selection, filters, browser history, view tabs, appearance/profile changes, cache lookups and source prefetch must not contact the model endpoint. Selecting or activating an explanation tab is strictly client navigation and performs no network request and no inference. Only an explicit Generate/Regenerate gesture may start inference (Deep may perform its documented prerequisite sequence as part of that one explicit action).
2. **Focus is not selection.** For files, focus can travel without selecting; for explanation tabs, arrow keys move focus while Enter/Space activates.
3. **Exactly one selected file and zero or one selected section.** Reader must never display a different section under the active file heading. Selecting a file automatically selects its first code section (or restores the last-read section for that file within the current immutable workspace); empty files have no source.
4. **No silent context switch.** Previous/Next sections stop at the current file boundary. Filter changes do not jump to a different section. Changing top-level destinations preserves Explore state.
5. **Native semantics first.** Navigation via links, actions via buttons, fields with labels, summaries via headings/lists. Avoid invented keybindings and custom ARIA tree.
6. **One concise live status region.** No hundreds of `role=status` elements; permanent error messages are discoverable on their own headings.
7. **Content and identity are immutable by request.** Late response for old selection can fill its cache but must not replace current Reader or update current announcement as if it were selected.
8. **No mouse-only functionality.** Visual themes, icons and animations cannot alter what a keyboard or screen-reader user can accomplish.

## Semantic page outline

```text
<a href="#main">Skip to main content</a>
<header> Git Explain / repository and revision / help </header>
<nav aria-label="Application">Explore | Personalization | Settings</nav>
[Explore destination:]
<nav aria-label="Files and code sections">
  <h2>Files</h2><ul><li><a ...>...</a></li></ul>
  <h2>Code sections in src/server.rs</h2><ol>...</ol>
</nav>
<main id="main">
  <h1>Explore</h1>
  <h2>src/server.rs — generate_file</h2>
  <nav aria-label="Code section navigation">Previous section | Next section</nav>
  <section aria-labelledby="source-title"><h3 id="source-title">Source</h3>...</section>
  <div role="tablist" aria-label="Explanation views">...</div>
  <div role="tabpanel" aria-labelledby="active-explanation-tab">...</div>
  <details><summary>Customize this section</summary>...</details>
</main>
<p id="app-status" role="status" aria-live="polite" aria-atomic="true"></p>
```

This is illustrative DOM, not code to copy verbatim. Heading levels must reflect actual container structure. There must be only one `main` landmark, even if Explorer and Reader use responsive panels. Hidden destinations should not leave duplicate main landmarks or focusable controls.

## Keyboard matrix

| Context | Input | Behavior |
| --- | --- | --- |
| Anywhere | Tab / Shift+Tab | Normal forward/backward focus order |
| Links / file and section entries | Enter | Navigate/activate the target |
| Native buttons | Enter / Space | Activate |
| Explorer search | Typing, Esc if opted in | Filter; Esc may clear only when documented and must not break expected browser behavior |
| Explanation tablist | Left/Right Arrow | Move focus to previous/next tab, wrapping, with **no activation** |
| Explanation tablist | Home / End | Move focus to first/last tab |
| Explanation tablist | Enter / Space | Activate focused tab in client state; network request and model call forbidden |
| Explanation tablist | Tab | Leave tablist for active tab panel / first focusable control |
| Source block | Standard selection and scroll keys | Read/copy text; no trapped arrows or custom editor mode |
| Customize disclosure | Enter / Space | Native `summary` open/close |
| Settings and profile forms | Native controls | Change draft values only; Save/Apply explicit |
| Modal Help, if used | Esc | Close and restore focus to trigger |
| Browser | Back / Forward | Navigate to previous/next app route, restore selection; no inference |

**Do not ship global letter shortcuts** such as E/S/N/P until tested against NVDA browse-mode single-letter navigation and browser controls. If future shortcuts are added, require modifier combinations, user documentation, conflict review and a disable preference. No essential workflow may depend on them.

## Focus transition table

| Trigger | Immediately after | On completion | On failure |
| --- | --- | --- | --- |
| Open app | Normal document/skip link | No automatic movement due to index completion | Error accessible by heading/live alert when necessary |
| Select file | Automatically selects first section (or restores last-read section in workspace); loads metadata and only selected source | Focus moves to Reader section heading once source is ready; adds one history entry with section | Focus stays recoverable; Retry present; invalid/stale explicit section ID yields error |
| Select section | Keep a stable focus reference while loading; Reader can show loading state | Focus selected Reader heading **once**, for explicit selection | Focus error heading once, with Retry, not a dead control |
| Previous/Next | Same as section selection | Heading says section name, position and file | Focus error heading |
| Activate explanation tab | Focus stays on selected tab | Panel updates; focus stays on tab | Status announced; tab stays focused |
| Generate/Regenerate | Focus stays on button | Announce completion once; provide Go to explanation | Announce actionable error once; Retry focusable |
| Open Customize | Focus remains on native summary | Controls available in document order | Not applicable |
| Apply temporary override | Apply state, close disclosure if user chooses | Return focus to Customize summary; announce “Preferences applied; regenerate to use” | Keep form, show validation error |
| Save profile | Focus stays on Save; disable duplicate submit only for pending write | Announce Saved, preserve route/draft context | Show field/conflict error and retain draft |
| Navigate top-level destinations | Route state changes | Focus destination h1 after explicit activation; Explore prior selection retained | Focus error area |
| Browser Back | Restore previous route | Focus destination or heading reflecting historical route, but do not refocus for every background request | Display explicit expired/missing state |

**Implementation note:** moving focus to a heading requires a programmatically focusable target, typically `tabindex="-1"`, with a visible focus indication for keyboard use. Focus movement should occur only for **user-initiated navigation** and only when the destination is the current route. An aborted or superseded request must not steal focus.

## File vs section selection

- Initial app load retrieves only metadata (file and section index), not source.
- Selecting a **file** automatically selects its first code section, or restores the last-read section for that file within the current immutable workspace. Empty files have no source and display an empty state. Invalid or stale explicit section IDs remain errors.
- Selecting a file loads only that selected section's source; it triggers zero model generation and adds one browser history entry including the selected section. Once the source is ready, focus moves to the Reader heading once.
- Selecting an explicit **section** loads only that section's source; zero inference.
- If a filter excludes the selected section, keep its Reader visible and label “Not in current filter”. Previous/Next disabled until the user selects a visible section or clears filter.
- When no filters exclude it, Previous/Next traverses the visible current-file list only. Disable at both ends. Label section destination via accessible name or `aria-describedby`.
- The underlying source order and list membership are defined by the backend snapshot. Avoid sorting by translated/localized displayed labels.

## Explanation tabs: APG manual activation

- `role="tablist"` with accessible name “Explanation views”.
- Tabs are buttons with `role="tab"`, `aria-selected`, `aria-controls`, roving `tabindex`. Active panel has `role="tabpanel"` + `aria-labelledby`.
- Arrow keys only move focus; Enter/Space activates. This prevents network latency from hijacking arrows.
- `Changes` is omitted or disabled with explanation in contexts where it is not supported; file mode never presents a working Changes tab.
- Selecting or activating a tab is strictly client navigation and performs **no network request** and **no model inference**; views switch using in-memory state.
- Explain that changing preferences does not mutate existing generated text and may cause another cache variant to become relevant.
- Keep panel content in the normal reading order, with a focusable panel when required by the pattern.
- No Previous/Next explanation buttons, and no auto-slide/carousel model.

References: [APG Tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/), [manual tabs example](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/examples/tabs-manual/).

## URLs, navigation and persistence

**Default routes, supported aliases and legacy fallback:**
- Standalone default: `/` (supported alias: `/workspace`; legacy fallback: `/legacy`).
- Daemon default: `/sessions/:id` (supported alias: `/sessions/:id/workspace`; legacy fallback: `/sessions/:id/legacy`).
- Deep links / sub-routes:
  - Standalone: `/?file=<opaque-key>&section=<opaque-key>&view=overview`
  - Daemon: `/sessions/:id?file=<opaque-key>&section=<opaque-key>&view=overview`
  - Top-level destinations: `/personalization`, `/settings` (standalone) or `/sessions/:id/personalization`, `/sessions/:id/settings` (daemon).
- Legacy renderer code and safeguards are retained until stabilization.

Do **not** place raw local filesystem paths, source code, credentials, control tokens, unsaved preferences or model prompts into shareable URL parameters. Encode/decode stable opaque identifiers. Validate any incoming route against the current snapshot. Selecting a file adds one history entry (`pushState`) including the selected section. Use browser `pushState` for deliberate section/destination navigation; use `replaceState` for non-historical changes like filter query when appropriate. Tab selection history behavior must be deterministic: prefer `replaceState` for transient view selection so Back traverses code sections and destinations rather than every explanation tab; preserve deep-link capability.

State lifetime:
- In URL: destination, selected file/section, active view when safe.
- In session client state: current filter, reading position, temporary section/view overrides, source caches, in-flight request tracking.
- In backend/user config: saved learning profiles/defaults, verified source/model/cache snapshots.
- In UI appearance storage: only after explicit choice of persistence mechanism.
- Never treat URL or client cache as an authority for policy identity or source validity.

## Live announcements and errors

One `role="status" aria-live="polite" aria-atomic="true"` for important changes:
- “src/server.rs, generate_file, section 3 of 12 loaded” (do not double-announce if heading focus already speaks this).
- “Deep explanation queued” / “Deep explanation ready” / “Deep explanation failed: …”.
- “Your Git snapshot changed. Reload to inspect the latest code.”
- “Preferences applied. Regenerate to use them.”
- “Learning profile saved.” or “Settings changed elsewhere; your draft is preserved.”

Do not announce each filter keystroke or every internally cached view. Search result count may use a *debounced* status with concise output. Use `role="alert"` selectively for blocking errors, not routine statuses. Include errors in visible text and associate with forms using native validation/`aria-describedby`.

## Source and annotations

- Raw source must remain available without requiring an editor dependency. Provide a plain-text `textarea readonly` or equivalent selectable view, with accurate language/section label, alongside optional syntax-highlighted `pre><code`.
- Syntax-highlighting markup must be escaped and must not corrupt selection/copy/reading order. Avoid duplicate source in simultaneously exposed accessibility trees: hide inactive representation properly.
- Explain annotations in a separately navigable list (“Annotation 2 of 5: … lines …”) and link to valid source regions where supported.
- Mark changed lines with text (“added”, “removed”) as well as color. Avoid color-only diff and noisy per-line focus.
- Support high-contrast system settings, forced-colors and reduced-motion. Clear focus outlines and active states must remain distinct.

## Accessible responsive behavior

Visual two columns must remain a coherent DOM sequence: header → Explorer → Reader → status. At high zoom/narrow view, stack rather than overlap. Skip links allow jumping to main, Explorer and selected section. Keep functionality without hover, resizer, drag-and-drop or icon identification. Test 200%/400% browser zoom, keyboard focus not covered, text size changes and narrow viewport with NVDA.

## Screen-reader test baseline

Manual matrix: NVDA + Windows + Edge (Chromium), plus at least one additional Chromium-based browser; test browse mode and focus mode. Review keyboard-only without screen reader separately. Manual NVDA and native zoom checks are explicitly deferred for later execution, not passed. Automated axe/Playwright checks cannot prove spoken announcements, manual-tab timing, familiar source reading or focus stability. See [test plan](test-plan.md).

## Accessibility references

- [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [APG](https://www.w3.org/WAI/ARIA/apg/)
- [Svelte accessibility warnings](https://svelte.dev/docs/svelte/compiler-warnings)
