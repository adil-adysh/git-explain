# git-explain UI Architecture Review

## Executive Summary

The current browser UI is still a small server-rendered document with localized progressive enhancement. Its browser state consists of one page-wide explanation-visibility flag, two per-unit presentation toggles, transient explanation-request state, and a daemon snapshot-generation check. There is no client-side router, data model, shared store, cross-unit selection, optimistic update system, or component lifecycle.

The difficult part is server-side HTML construction in `src/web.rs`, not client-side state management. `render_for_session_at_generation` (lines 116–246) mixes document structure, conditional application state, CSS, JavaScript, URL construction, escaping, accessibility attributes, and API payload assumptions in one renderer. The JavaScript is compact, but the current source is hard to edit safely because the initial Rust-rendered markup and the JavaScript reconstruction of annotated source are parallel implementations.

Recommendation for the next implementation step: **B. Refactor into server-rendered semantic components plus separate CSS/JS, without a client framework.** Keep assets embedded so the binary remains self-contained. Split the Rust renderer into small render functions/view helpers, move the inline CSS and JavaScript into checked-in asset files (embedded with `include_str!` or equivalent), and centralize the small browser API/DOM helpers. Do not introduce React/Vue/Svelte or a Rust template dependency yet.

## UEX validation (2026-08-27)

The current UEX meets the project’s local-first, explanation-only philosophy, with the following evidence and limits:

| Criterion | Assessment | Evidence / remaining limit |
| --- | --- | --- |
| Inline explanation | Meets | Each changed unit has an overview plus source-adjacent annotation sections with explicit line ranges; the source is never modified. `tests/core.rs` covers source preservation, overlap handling, and annotation placement. |
| Inclusive accessibility | Strong baseline; not a certification | The page uses `lang`, landmarks, heading hierarchy, native buttons, a read-only text-code alternative, `aria-controls`, `aria-expanded`, `aria-busy`, live status, and alert feedback. Automated tests cover emitted semantics. A screen-reader audit and automated WCAG tool run are still required before claiming conformance. |
| Mental model and structure | Meets for the current scope | The page is organized as context → file → changed unit → overview → actions → code/annotations, with explicit line ranges and an optional indentation view. The global “Hide explanations” control and per-unit actions preserve the distinction between understanding and editing. |
| Cognitive load | Generally decreases; one tradeoff remains | Progressive disclosure keeps deep explanations, annotations, text mode, and indentation details optional. Multiple actions per unit and the global visibility toggle add choice; labels are intentionally explicit and the page keeps controls local to each unit. |
| Project alignment | Meets | The UI is server-rendered and progressively enhanced, binds locally, keeps source read-only, exposes model failures without leaking provider details, and treats stale snapshots as recoverable rather than silently applying outdated results. |

Error UEX is now explicit at the API boundary: model unavailable, timeout, authentication, rate limiting, configuration failure, generic generation failure, stale snapshot/session, missing unit, and empty source each have stable `code` values and actionable `error` text. Detailed provider errors remain in server logs. Browser requests disable the active control, expose busy state, enforce a client timeout, retain a per-unit alert, and restore the control for retry. A live browser/screen-reader pass could not be completed in this environment because the locally started daemon exited before its session endpoint was reachable; this is recorded as a verification gap, not as evidence of accessibility conformance.

This is not a recommendation to leave correctness work until later. Before or during that refactor, preserve and extend the existing source/annotation overlap and visibility tests. The current code has already addressed several accessibility and annotation-rendering concerns in the working tree, but those tests prove emitted strings and not actual browser behavior.

## Current Rendering Architecture

The actual current flow is:

```text
git explain / git explain REVISION
    ↓
src/main.rs: analyze_working_tree / analyze_commit, then daemon.open_repository
    ↓
src/daemon.rs: daemon discovery/start and authenticated /api/repos/open
    ↓
RepositorySession { AnalysisSnapshot, items, provider, cache, generation }
    ↓
GET /sessions/{id} in daemon.rs
    ↓
web::render_for_session_at_generation
    ↓
Html<String> containing semantic HTML, inline CSS, and inline JavaScript
    ↓
browser
    ↓
plain JavaScript in the generated page
    ├─ fetch explanation/deep-explanation JSON
    ├─ mutate one article's DOM
    ├─ toggle global explanation visibility
    ├─ switch rendered source/text source
    ├─ show indentation details
    └─ poll session snapshot generation and offer location.reload()
    ↓
Axum JSON routes in server.rs or daemon.rs
    ↓
runtime::result / runtime::apply and the session explanation overlay
```

The direct compatibility path is the same rendering boundary without a session:

```text
src/main.rs → src/server.rs::serve → GET / → web::render
```

`RepositoryAnalyzer` in `src/analyzer.rs` creates an `AnalysisSnapshot`; it does not render HTML, run Axum, or call the model. `ExplainedUnit` is the source-unit presentation object in `src/explain.rs`; `AnalysisSnapshot` and `SnapshotGeneration` are in `src/snapshot.rs`. `runtime.rs` hydrates cached explanations, builds model requests, applies results, and serializes the JSON result.

## Module and Responsibility Map

| Layer | Current owner | Browser-relevant responsibility |
| --- | --- | --- |
| CLI | `src/main.rs`, `src/cli.rs` | Select working tree/commit, direct mode or daemon mode, configuration and browser launch. |
| Analysis | `src/analyzer.rs`, `src/explain.rs`, `src/language/` | Produce `AnalysisSnapshot`, changed files, `ExplainedUnit`s, source ranges, and explanation regions. |
| Snapshot | `src/snapshot.rs` | Stable `UnitId`, snapshot identity, generation, ordered units, and context boundary. |
| Direct server | `src/server.rs` | Own one immutable snapshot plus mutable explanation map; expose `/` and direct explanation routes. |
| Daemon | `src/daemon.rs` | Own bounded `RepositorySession`s, session registry, generation refresh, inference cancellation/deduplication, session HTML route, and session API routes. |
| Runtime | `src/runtime.rs` | Cache hydration, model request construction, overlay mutation, and JSON response shape. |
| HTML renderer | `src/web.rs` | Escape data, render indentation/source/annotations, render page and unit markup, choose action labels/endpoints, embed CSS and JS. |
| Browser enhancement | Inline script emitted by `src/web.rs:231–244` | DOM toggles, API calls, annotation reconstruction, announcements, snapshot polling, and reload. |
| Tests | `tests/core.rs`, `src/daemon.rs` tests, `tests/daemon_lifecycle.rs` | HTML string assertions, server/session lifecycle and refresh assertions; no real browser execution. |

The daemon's control APIs use the control token, but browser HTML does not receive it. Browser requests use only opaque session/unit IDs and snapshot generation. The daemon binds to `127.0.0.1` in `src/daemon.rs:355–357`.

## HTML Rendering

`src/web.rs` contains the complete current HTML renderer. There are no other HTML, CSS, JavaScript, or static-asset files in the repository. `render` delegates to `render_for_session`, which delegates to `render_for_session_at_generation`.

### Rendered elements

| UI element | Current rendering code | Inputs/state | Initial vs client-rendered |
| --- | --- | --- | --- |
| Document/head | `render_for_session_at_generation:153–170` | Context-derived title and heading | Server-rendered. |
| Page header | `:165–169` | Working-tree or commit `AnalysisContext` | Server-rendered. |
| Commit context | `:122–146` | OID, parent OID, subject, merge-parent count | Server-rendered. |
| Deleted-file notice | `:171–173` | `context.deleted_files` | Server-rendered. |
| File section/header/count | `:174–179` | Ordered `ExplainedUnit`s grouped by `item.file` | Server-rendered. |
| Snapshot update notice | `:165`, `:242–244` | Session generation and snapshot endpoint | Notice is server-rendered hidden; visibility/message are client-mutated. |
| Global explanation action/status | `:165`, `:238` | Page-wide `explanationsHidden` JS flag | Server-rendered button/status; client toggles all `.ai-explanation`. |
| Unit article | `:225–226` | `ExplainedUnit`, generation, action-state decisions | Server-rendered. |
| Unit metadata | `:225–226` | Kind, language, source line range | Server-rendered. |
| Overview | `:180–184`, `:225–226` | `item.explanation.overview`, placeholder if empty | Server-rendered; paragraph text is client-updated after normal explanation. |
| Normal explanation action | `:190–204`, `:225–226`, `:239` | Whether overview is present | Server-rendered action state; label/data endpoint changes client-side. |
| Deep explanation action/section | `:185–189`, `:205–218`, `:225–226`, `:239` | Whether `deep_explanation` is present | Server-rendered hidden section/action; text/hidden state changes client-side. |
| Rendered source and inline annotations | `rendered_source:59–102`, used at `:226` | Source and normal annotations | Initial content server-rendered; later annotation replacement client-rendered. |
| Read-only text source | `:220–226` | Source text, generated IDs | Server-rendered hidden `<textarea>`; visibility toggled client-side. |
| Indentation details | `indentation_details:36–57`, used at `:226` | Source and starting line | Server-rendered hidden list; visibility/label/`aria-expanded` toggled client-side. |
| Status/live region | `:165`, `:232–233` | Transient messages | Server-rendered empty region; client sets `textContent`. |

The renderer performs HTML escaping through `escape` for model/source/path/context data. The browser-side `escapeHtml` is used before dynamic annotation/source fragments are inserted through `insertAdjacentHTML`.

The page does not render a separate client-side file/unit data model. The DOM itself carries unit ID, generation, endpoint, action, and source content through attributes and elements.

## CSS Architecture

CSS is embedded in the `<style>` block created inside `render_for_session_at_generation` at `src/web.rs:154–164`. There is no `.css` file, preprocessor, design-token package, or asset pipeline.

The stylesheet is approximately 11 physical source lines, but most rules are packed into long lines. It contains roughly 25–30 selector groups covering:

- page/root typography and colors;
- header/context/file/article layout;
- buttons and focus/hover borders;
- source/pre/textarea presentation;
- annotations and indentation details;
- `.sr-only` and live-region styling;
- one mobile breakpoint at 600px;
- system dark mode through `prefers-color-scheme`.

The CSS is component-like by class naming (`.file-section`, `.source-region`, `.annotation`, `.indentation-details`) but is not structurally isolated. It depends on the generated DOM for selectors such as `article`, `pre code`, and `h4`; this is manageable at the present scale. Responsive behavior is limited to narrower page width, article padding, and full-width action buttons. There is no evidence of CSS complexity that independently justifies a framework.

The useful CSS improvement is extraction to `assets/app.css` while preserving the existing class names and embedding it at compile time. That would improve reviewability without changing distribution.

## JavaScript Architecture

The JavaScript is emitted inline by `src/web.rs:231–244` and is approximately 14 physical source lines, with several very long single-line functions. It is plain JavaScript and has no package/runtime dependency.

Current software characteristics:

- one shared mutable global: `let explanationsHidden=false`;
- one page-level status helper: `announce` writes `#status.textContent`;
- direct per-article event listeners for mode and indentation buttons;
- direct page-level listener for the global explanation toggle;
- direct listeners for `.explain,.deep` buttons;
- one async request helper, `call(button)`, that uses button `data-*` values;
- one client-side annotation renderer, `renderAnnotations`;
- one session-only polling closure using `setInterval(...,5000)`;
- no client routing, virtual DOM, component lifecycle, event delegation, normalized data store, or cross-component subscription model.

This is progressive enhancement over server-rendered HTML, not a client-side application in the current sense. `call` does not implement aborts, request IDs, or last-write-wins handling. The server's generation/session validation is the primary consistency protection. The browser catches errors and announces a generic failure.

The most important architectural duplication is `rendered_source` in Rust (`src/web.rs:59–102`) versus `renderAnnotations` in JavaScript (`src/web.rs:236`). Both split source into lines, sort or process annotation ranges, emit source fragments and annotation sections, and escape dynamic content. They are not identical implementations: Rust renders initial annotations; JavaScript reconstructs the rendered source after a normal explanation response. The current working-tree code has aligned their overlap clamping and annotation line metadata more closely, and tests cover the emitted forms, but the duplication remains.

## Client-Side State Inventory

| State | Where stored | Survives DOM update | Survives reload/session refresh |
| --- | --- | --- | --- |
| `explanationsHidden` | JS variable in the page script | Yes for ordinary per-article mutations because `renderAnnotations` reads it; no general persistence beyond this document | No; reset to `false` on page reload. |
| Per-unit rendered/text source mode | `hidden` properties on `.rendered-source` and `.text-source`; button text | Yes unless the whole page reloads | No. |
| Per-unit indentation open/closed | `hidden` on `.indentation-details`, `aria-expanded`, button text | Yes unless the whole page reloads | No. |
| Normal action state | Button `data-action`, `data-endpoint`, and `textContent` | Yes for the current article | Re-derived from server overlay on reload. |
| Deep action state | Button `data-action`, `data-endpoint`, and `textContent` | Yes for the current article | Re-derived from server overlay on reload. |
| Normal explanation content | Overview paragraph and rendered-source DOM | Yes for current page | Server/session overlay and cache rehydrate it on reload. |
| Deep explanation content/visibility | Deep section paragraph plus `hidden` | Yes for current page | Server/session overlay rehydrates content; visibility resets hidden. |
| Snapshot generation | `data-generation` on `#snapshot-update` and each action button | Yes until reload | New page gets the current generation. |
| Snapshot update availability | Hidden state and message text in `#snapshot-update` | Yes | No; reload replaces the page. |
| Request-in-progress state | None beyond status text; button is not disabled | Not represented | Not persisted. |

The server-side state is more substantial but not browser-side UI state: `RepositorySession.snapshot` is immutable for a generation, while `items: Mutex<HashMap<UnitId, ExplainedUnit>>` is the mutable explanation overlay. Cache hydration happens before initial HTML. Refresh replaces the session snapshot and cancels/guards old inference work.

## Browser Interaction Inventory

| Trigger | Request | DOM mutation/focus/announcement | State transition |
| --- | --- | --- | --- |
| Generate normal explanation | `POST unitPath + unit ID + /explain` with `{generation}` | Overview `textContent`, source `replaceChildren`/`insertAdjacentHTML`, button dataset/text; announces start, ready, or generic failure | Normal button changes to regenerate. |
| Regenerate normal explanation | Same method with `/regenerate` | Same mutations | Remains regenerate. |
| Generate deep explanation | `POST .../deep` with `{generation}` | Deep section paragraph and hidden state; button dataset/text; announcements | Deep button changes to regenerate. |
| Regenerate deep explanation | `POST .../deep/regenerate` | Same mutations | Remains regenerate. |
| Hide/show explanations | No request | Sets `hidden` and `data-toggle-hidden` on every `.ai-explanation`; changes global button text and status | `explanationsHidden` changes. |
| Read code as text/show rendered code | No request | Swaps `hidden` between the two source regions; changes button text; announces mode | Per-article mode changes. |
| Show/hide indentation details | No request | Sets section `hidden`, button `aria-expanded`, button text; announces state | Per-article indentation state changes. |
| Snapshot polling | `GET /api/sessions/{session}/snapshot` every 5 seconds in session mode | If generation is newer, sets update message and removes hidden state | Update availability becomes visible. |
| Reload snapshot | No fetch of page data; `location.reload()` | Full document replacement | All ephemeral client state is lost and current server snapshot is rendered. |

The JavaScript does not move focus explicitly. It does not disable buttons during requests. The focused button remains in place for normal/deep updates, while a full `location.reload()` necessarily replaces the document. The status region is the only announcement mechanism.

## HTTP/API Boundary

### Browser-facing direct-mode routes (`src/server.rs:70–77`)

| Method/path | Payload | Response used by browser | Server mutation/validation |
| --- | --- | --- | --- |
| `POST /api/units/{id}/explain` | Optional JSON `{generation}`; browser sends it | `ok`, `overview`, `annotations`, `deep`, `mode` from `runtime::result` | Looks up `UnitId`, rejects stale generation, may read cache, calls provider, updates explanation overlay. |
| `POST /api/units/{id}/regenerate` | Same | Same | Same, bypassing cache lookup. |
| `POST /api/units/{id}/deep` | Same | Browser uses `deep || overview` | Same with `deep=true`. |
| `POST /api/units/{id}/deep/regenerate` | Same | Browser uses `deep || overview` | Same with `deep=true`, bypassing cache lookup. |
| `POST /api/deep/{id}` | Same | Same deep response shape | Additional direct deep route alias; the generated browser URL does not use this alias. |

Direct mode starts at generation 1 and has no browser snapshot polling endpoint.

### Browser-facing daemon routes (`src/daemon.rs:382–399`)

| Method/path | Payload | Response used by browser | Server mutation/validation |
| --- | --- | --- | --- |
| `GET /sessions/{session}` | None | HTML document | Reads the current session overlay in snapshot order. |
| `POST /api/sessions/{session}/units/{id}/explain` | `{generation}` | `ok`, `overview`, `annotations`, `deep`, `mode` | Validates session and generation, then cache/inference/update guarded by session identity. |
| `POST /api/sessions/{session}/units/{id}/regenerate` | `{generation}` | Same | Same, cache bypass. |
| `POST /api/sessions/{session}/units/{id}/deep` | `{generation}` | Browser uses `deep || overview` | Deep generation. |
| `POST /api/sessions/{session}/units/{id}/deep/regenerate` | `{generation}` | Same | Deep regeneration. |
| `GET /api/sessions/{session}/snapshot` | None | `ok`, `session_id`, `generation`, `identity` | Read-only status for polling. |

The control routes `/api/repos/open`, `/api/repos/refresh`, `/api/daemon/shutdown`, and `/api/health` are daemon/CLI infrastructure. They are not called by page JavaScript. Open, refresh, and shutdown require `x-git-explain-control-token`; the page does not receive that token.

Stale explanation requests return JSON with `ok:false`, `stale:true`, and an error. The current browser treats all errors alike and announces `Unable to generate explanation.`; it does not expose the stale-specific recovery path. The snapshot notice is a separate polling/reload mechanism.

## Server vs Client Rendering

| UI element | Initial server render | Client mutation | Requires reload |
| --- | --- | --- | --- |
| File/unit list | Entire list, grouping, metadata, source | None | Yes for new/deleted/reordered units. |
| Normal explanation | Existing cached/overlay explanation and annotations | Overview, annotations, action state | No for one unit's explanation. |
| Deep explanation | Existing content, hidden section, action state | Content, visibility, action state | No for one unit's deep explanation. |
| Code reader mode | Both source representations emitted; text source hidden | Swaps `hidden` | No. |
| Indentation details | Full list emitted hidden | Swaps `hidden`, label, `aria-expanded` | No. |
| Global explanation visibility | Button and all initial explanation elements | Hides/restores `.ai-explanation`, including generated annotations | No. |
| New snapshot | Current snapshot page | Notice only; reload uses a new server render | Yes to view changed source/unit structure. |
| Commit/deleted-file context | Fully server-rendered | None | Yes if context changes. |

The boundary is intentionally server-first. A browser arriving without JavaScript still receives source, cached explanations, controls, headings, and a read-only source textarea; the buttons and mode controls do not perform their enhanced actions without JavaScript.

## Accessibility Architecture

The source contains explicit semantic/accessibility markup:

- `html lang="en"`, one page `h1`, file `h2`, unit `h3`, section labels `h4`, annotation headings `h5`;
- `header`, `main`, `section`, and `article` landmarks/structural elements;
- native `<button>` controls rather than clickable generic elements;
- a read-only `<textarea>` with `spellcheck="false"`, `wrap="off"`, and a visible-to-assistive-technology `<label>` whose text includes the unit name and “read only”;
- `<pre><code>` for rendered source and `<code>` in indentation detail rows;
- `role="status" aria-live="polite"` for the snapshot notice and general status region;
- `aria-expanded` on the indentation details control;
- `aria-controls` on mode/indentation controls, with generated IDs in the current working tree;
- `.sr-only` text for indentation descriptions and blank-line descriptions;
- `aria-hidden="true"` on the visual placeholder for a blank indentation line.

The two source representations are emitted together, but the rendered source is visible and the textarea is initially `hidden`. JavaScript toggles the two `hidden` states so they are not both exposed at once. `tests/core.rs::code_modes_hide_duplicate_accessible_source` checks the emitted IDs, hidden source, and toggle script. This is implemented semantic intent; the repository contains no manual NVDA/JAWS/browser screen-reader test evidence, so screen-reader behavior should not be claimed beyond the source semantics.

JavaScript changes accessible names in several controls by changing button text, changes `aria-expanded`, and announces status through `textContent`. It does not move focus, set focus targets, or explicitly restore focus after reload. It also does not update `aria-busy` or expose a per-button loading state.

## DOM Mutation Model

There are five distinct mutation paths in the current script:

1. `announce`: `#status.textContent = message`.
2. `renderAnnotations`: `.replaceChildren()` followed by `insertAdjacentHTML` for source and annotation fragments.
3. Per-unit mode: `.hidden` on rendered/text regions and button `textContent`.
4. Per-unit indentation: `.hidden`, `aria-expanded`, and button `textContent`.
5. Global visibility: `.hidden`, `dataset.toggleHidden`, and global button text.
6. Explanation completion: paragraph `textContent`, deep section `hidden`, action dataset/text.
7. Snapshot availability/reload: message `textContent`, notice `.hidden`, then `location.reload()`.

The mutation scope is mostly localized to the closest `article` for explanations, mode, indentation, and generated annotations. The global toggle intentionally affects every `.ai-explanation` on the page. The snapshot reload affects the entire document. There is no generic render/mount abstraction.

Dynamic HTML insertion occurs only in `renderAnnotations`; it calls `escapeHtml` for source, annotation kind, and annotation text before concatenating markup. The static server-side path calls `escape` for dynamic values. The code is therefore deliberately avoiding raw model/source HTML at both insertion points, although the duplicated escaping/rendering logic deserves a single tested ownership boundary.

## Testing and Test Gaps

`tests/core.rs` currently covers:

- `escape` output;
- server-side annotation/source ordering and preservation;
- overlapping annotation source-line behavior;
- initial no-explanation action state;
- all four normal/deep action-state combinations;
- emitted JavaScript action transitions;
- generated annotation visibility after the global toggle;
- read-only textarea/source whitespace;
- hidden duplicate source representations and `aria-controls` IDs;
- session generation/update notice markup;
- commit parent/deleted-file context and escaping.

These are string assertions over `web::render`. They verify server output and the presence of JavaScript source text; they do not execute JavaScript in a browser or DOM implementation. Consequently, they do not prove that:

- fetch requests use the expected URL/body at runtime;
- a real response updates only the intended article;
- focus is preserved;
- `aria-live` announcements are delivered as intended;
- global hide/show behaves after multiple generated responses;
- mode and indentation controls work in a real browser;
- polling and reload behavior works across a refresh;
- concurrent normal/deep requests or a stale response cannot produce an undesirable visible state.

`tests/daemon_lifecycle.rs` exercises daemon startup, authenticated repository open, multiple sessions, session pages, snapshot endpoints, shutdown, and stale metadata. Internal daemon tests cover refresh replacement, generation increment, unchanged refresh, cancellation/deduplication-related behavior, and session state. They do not execute the page JavaScript. `tests/model_http.rs` tests model/provider HTTP behavior, not browser integration.

The most valuable future test addition before a framework decision would be a small browser/DOM integration test for the existing plain script, especially explanation completion, visibility preservation, source-mode toggling, and stale snapshot handling. That is a testing improvement, not evidence that a framework is required.

## Packaging and Build Constraints

The repository is entirely Rust-buildable today. `Cargo.toml` has Rust dependencies only; there is no `package.json`, npm/pnpm/yarn lockfile, `node_modules`, TypeScript, CSS preprocessor, bundler, or asset pipeline. `Taskfile.yml` runs `cargo fmt`, `cargo test`, `git diff --check`, and `cargo build`/`cargo build --release`.

The current browser assets are generated as strings in the executable at request time. There are no files read from disk by the server for CSS or JavaScript. This supports the local single-binary distribution and offline development model.

Option B can preserve that property with files such as `assets/app.css` and `assets/app.js` included through `include_str!` (or a small equivalent compile-time embedding). A Rust template/component library could also remain single-binary, but would add a Cargo dependency and a new rendering convention. A client framework would add a Node toolchain, package lock and dependency cache, a build step, generated asset handling, CI setup, Windows developer setup, and decisions about whether to embed generated assets or serve them from disk. It would also complicate offline development and release reproducibility unless the compiled assets were checked or embedded.

No external JS/CSS files are currently needed at runtime. Splitting source files for maintainability is therefore compatible with packaging; the runtime does not need to become a multi-file installation.

## Current Complexity Measurements

Measured from the current working tree:

| Area | Approximate size |
| --- | ---: |
| `src/web.rs` | 247 physical lines |
| `src/server.rs` | 200 lines, mostly API/server orchestration rather than UI markup |
| `src/daemon.rs` | 1,224 lines, including daemon/session/model lifecycle and browser routes |
| `src/runtime.rs` | 74 lines |
| `src/snapshot.rs` | 138 lines |
| `src/explain.rs` | 476 lines, primarily analysis/domain logic |
| UI-focused test file `tests/core.rs` | 352 lines |
| Inline CSS source | 11 physical lines, approximately 25–30 selector groups |
| Inline JavaScript source | 14 physical lines, approximately 10 named/inline behavior blocks; several lines are very long |

The renderer has 7 Rust functions in `src/web.rs`: `escape`, `indentation_description`, `indentation_details`, `rendered_source`, `render`, `render_for_session`, and `render_for_session_at_generation`. The large function is `render_for_session_at_generation:116–246` (about 131 lines), with responsibilities spanning context selection, API base URL construction, page shell, CSS, file/unit markup, action-state rules, and script embedding. The largest single generated HTML fragment is the unit article `format!` at lines 225–226. The largest JavaScript function is the one-line `renderAnnotations` at line 236; the one-line `call` function at line 239 also combines request orchestration and DOM state transitions.

These measurements indicate a readability/ownership problem in the renderer. They do not indicate a large client application.

## Near-Term UI Requirements

### CURRENT

- Working-tree and commit context rendering.
- Deleted-file notices.
- Changed file/unit list with source and metadata.
- Cached/loaded normal and deep explanations.
- Generate/regenerate normal explanation.
- Generate/regenerate deep explanation.
- Inline annotations and source/text reading modes.
- Indentation details.
- Global explanation visibility.
- Session generation polling and reload notice.
- Multiple daemon sessions, though each page addresses one opaque session ID.

### PLANNED / documented

`docs/architecture.md` explicitly documents daemon session registry, snapshot generation, atomic refresh, cancellation, inference deduplication, and the existing update-available/reload behavior. It explicitly says repository watching and automatic refresh remain deferred. It does not commit to a timeline, filtering UI, side-by-side diff, reading state, or multi-panel workspace.

### INFERRED possibilities

Filtering changed units, structural outlines, deterministic change facts, before/after views, persistent reading state, synchronized panes, and live repository watching are plausible future product directions, but they are not current source-level commitments. They should not drive a client-framework decision today.

## Framework Pressure Analysis

| Feature | Server-side complexity | Client state complexity | Framework pressure |
| --- | ---: | ---: | ---: |
| Current explanation actions | Medium | Low | Low |
| Snapshot update banner/reload | Low | Low | Low |
| Current source/text and indentation toggles | Low | Low | Low |
| Deterministic change facts rendered with units | Medium | Low | Low |
| Filtering changed units | Low–Medium | Medium | Low–Medium; plain JS remains viable initially |
| Structural outline/navigation | Medium | Low–Medium | Low–Medium |
| Before/after source views | Medium | Medium | Medium if synchronized scrolling/selection is required |
| Unit evolution timeline | High | Medium–High | Medium |
| Persistent reading state across units/snapshots | Medium | Medium–High | Medium |
| Live repository watching with incremental replacement | High | High | Medium–High |
| Multi-panel synchronized workspace across repositories | High | High | High |
| Optimistic, cross-unit collaborative editing/state | High | High | High, but not currently implied |

The only present cross-unit state is the intentional global explanation toggle. The server already owns snapshot identity and guards stale requests, which keeps browser state small. A future feature becomes framework-relevant when it requires several independently persistent, cross-unit views to coordinate without reload.

## Option A: Current Approach

**Rust-generated HTML, embedded CSS, embedded JavaScript.**

This preserves the smallest dependency and strongest single-binary story. It also keeps semantic HTML explicit and makes the initial page accessible without a runtime framework. Its weakness is concentrated maintainability: the large `format!` strings are difficult to read, CSS/JS changes are mixed with Rust logic, and source/annotation rendering is duplicated between Rust and JavaScript.

Option A is operationally sound for current behavior, but “unchanged” leaves the known renderer ownership problem in place.

## Option B: Server-Rendered Components + Plain JS

**Recommended next step.** Extract the current renderer into small Rust functions or modules, for example page shell, context header, file section, unit article, action controls, source reader, annotation renderer, and indentation renderer. Put CSS and JS in `assets/app.css` and `assets/app.js`, then embed them with `include_str!` to retain a single executable.

This solves the actual problem—reviewability, escaping boundaries, fragment reuse, and source ownership—without adding a client state architecture. The browser remains semantic HTML plus small progressive enhancement. It also creates a clean seam for a later template library or browser framework if concrete future requirements justify one.

## Option C: Rust Template/Component Layer

Askama, Maud, or a similar Rust-native approach could improve HTML readability, escaping, conditional markup, and component reuse. It would address the server-side construction pain more directly than a client framework and can preserve server-rendered HTML and single-binary packaging.

It is not required to solve the first problem. Small rendering functions plus external embedded assets provide most of the immediate benefit with no new dependency or template syntax. A template layer becomes more attractive if the number of conditional server-rendered views grows materially, if HTML escaping is repeatedly hand-maintained, or if component fragments become numerous enough that string assembly remains the bottleneck after Option B.

## Option D: Client-Side Framework

A client framework would provide component lifecycle conventions, state binding, and potentially stronger runtime testing/tooling. It is not inherently inaccessible; the current semantic DOM could be reproduced in any framework. However, it would move the application boundary from server-rendered HTML with local enhancements toward a client-managed view model. That adds build, packaging, dependency, and accessibility-synchronization surface before current requirements demand it.

The current UI has no client route, no large synchronized state graph, no optimistic updates, and no persistent cross-unit workspace. A framework would mainly reorganize a small amount of state while introducing a much larger toolchain and abstraction boundary.

## Comparison Matrix

Ratings are relative for this repository and current requirements: Low is favorable/low burden; High means greater risk or complexity for that criterion.

| Criterion | A: Current | B: Components + plain JS | C: Rust template layer | D: Client framework |
| --- | --- | --- | --- | --- |
| Accessibility risk | Low–Medium | Low | Low | Medium initially |
| Maintainability | Medium–High burden | Low–Medium burden | Low burden | Medium burden now |
| Testability | Medium | Low–Medium | Low–Medium | Medium–High after setup |
| Client state complexity | Low | Low | Low | Medium–High potential |
| Build complexity | Low | Low | Medium | High |
| Single-binary packaging | Low | Low | Low–Medium | Medium–High |
| Windows/offline workflow | Low | Low | Low–Medium | Medium–High |
| DOM control/predictability | High control | High control | High control | Medium control |
| Future large workspace fit | Low–Medium | Low–Medium | Low–Medium | High |
| Fit for current UI | Medium | High | Medium–High | Low–Medium |

Option B has the best current trade-off. Option C is a reasonable later server-rendering refinement. Option D is a future option only after the UI crosses concrete state/lifecycle thresholds.

## Current Risks and Technical Debt

1. **Renderer coupling — design risk.** `render_for_session_at_generation` mixes state decisions, HTML, CSS, JS, URLs, and accessibility attributes.
2. **Duplicated annotation rendering — design risk.** `rendered_source` and `renderAnnotations` independently reconstruct source/annotation fragments. Tests cover important overlap behavior, but the two implementations can drift.
3. **No runtime browser test — confirmed gap.** `tests/core.rs` asserts generated strings; no browser/DOM execution is present.
4. **Request state is not represented — design risk.** `call` does not disable or mark the initiating button, and concurrent requests have no client-side ordering policy. Server generation/session guards protect stale repository state, but not every possible user-visible ordering scenario within one current unit.
5. **Stale errors are generic — design risk.** The browser does not distinguish `stale:true` from model failure, so a changed snapshot does not receive a tailored recovery announcement.
6. **Ephemeral UI state resets on reload — intentional/current behavior.** Source structure must reload for a new snapshot, so mode, indentation, and explanation visibility are not persisted.
7. **Global selector scope — intentional coupling.** The global toggle affects every `.ai-explanation`, including overview, deep explanation, and annotations. This is currently the desired global behavior but should remain centralized.

No current source evidence establishes a confirmed browser bug in focus handling or screen-reader behavior. Those are test gaps/risks, not observed failures.

## Recommended Next Architecture

Choose **B: server-rendered semantic components + separate CSS/JS, no framework**.

Suggested shape:

```text
src/web/
    mod.rs          // public render API and asset embedding
    render.rs       // page/context/file/unit composition
    source.rs       // escaped source/annotation rendering
    accessibility.rs // IDs, labels, status/visibility helpers if useful
assets/
    app.css
    app.js
```

The exact module names are optional; the ownership boundaries are the important part. Keep `render_for_session_at_generation` as the public compatibility entry point initially, but have it compose smaller functions. Preserve `escape` as one tested escaping primitive. Keep the server API and `data-unit-id`/generation contract stable.

Use `include_str!` for `assets/app.css` and `assets/app.js` so release artifacts remain a single executable. Avoid introducing a runtime asset directory. In the JS file, retain plain DOM APIs, but centralize API calls, status announcements, article lookup, action-state updates, source rendering, and explanation visibility. If practical, make the source/annotation fragment format a single server-owned contract instead of maintaining two almost-identical renderers.

## Reassessment Triggers

Revisit a client framework when one or more of these concrete conditions becomes an actual requirement:

- client-side routing or deep-linkable application views beyond one server document;
- more than one independently persistent page-wide state domain that must coordinate across units;
- cross-unit selection, filtering, sorting, and synchronized panes that must update without reload;
- side-by-side or before/after views with synchronized scrolling, cursor/line selection, or linked annotations;
- timeline/live-update behavior that incrementally replaces units while preserving reading position and multiple local UI states;
- optimistic updates or cancel/retry workflows requiring explicit request identity and reconciliation;
- a shared data model consumed by several independently mounted views;
- browser code requiring lifecycle management beyond localized article mutations;
- a real browser test suite that reveals the current plain-DOM approach has become the dominant source of defects.

Today, none of the first seven conditions is present. Snapshot polling exists, but it offers a single notice followed by full reload, not a live client-side data synchronization system.

## Suggested Migration Sequence

1. Preserve the current working-tree behavior and tests; do not change API routes or snapshot-generation semantics.
2. Extract CSS and JS into source-controlled asset files and embed them at compile time.
3. Split the Rust page renderer into small functions for context, files, units, actions, source/annotations, indentation, and page shell.
4. Centralize generated IDs, endpoint construction, and action-state mapping in a small view model/helper layer.
5. Make the Rust and browser annotation rendering contract explicit; either keep one intentionally shared representation or add focused tests proving both paths agree on overlap/clamping/escaping.
6. Add a minimal browser/DOM integration test for the existing plain JavaScript behavior, especially explanation completion, hide/show preservation, source-mode exclusivity, and snapshot update handling.
7. Improve stale-response/error messaging and consider button busy/disabled semantics only if the desired interaction is defined; do not add a framework as a substitute for deciding those semantics.
8. Reassess Option C if server-rendered conditional markup becomes the next bottleneck. Reassess Option D only when the triggers above are concrete product requirements.

The smallest change that addresses current difficulty while retaining accessible, local, single-binary behavior is therefore: **refactor the server renderer and assets, keep plain JavaScript, and defer both template dependencies and client frameworks.**
