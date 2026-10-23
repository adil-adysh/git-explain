# Phased Svelte 5 implementation plan

> Implementation note (2026-10-23): the user explicitly requested the new UI as default and deferred manual NVDA/zoom testing. Default cutover therefore proceeds as an accepted exception to the original manual release gate. Legacy remains available at explicit fallback routes during stabilization. See [current evidence](acceptance-record.md). Historical task checkboxes below describe the original plan; current completion is tracked separately.


> **Status:** Proposed execution plan, 2026-10-23. No implementation has been started by this documentation change.
>
> Follow [screen specs](screens.md), [interaction contract](interaction-accessibility.md), [API/architecture](architecture-api.md) and [test plan](test-plan.md). Deliver **small, independently reviewable PRs**. Do not deploy the entire redesign as one large rewrite.

## Release definition and order

**Release objective:** Replace the current eager all-sections browser page with the accessible Svelte 5 workspace **without regression** in file mode, diff mode, daemon sessions, generation/caching, learning, feedback, or CLI/debug operation.

**Non-negotiable:** No navigation/inference coupling. Every phase that touches UI must include keyboard behavior, error states and tests. Do not remove `src/web.rs`/`src/web.js` until final cutover.

**Sequencing:** Baseline → architecture/correctness foundations → app shell → index/source → Explorer/Reader → generation/view parity → personalization/settings → full validation/cutover.

Feature flags or a route switch should permit side-by-side new/legacy UI until acceptance. A “fallback to old UI” must not accidentally create two competing config write paths.

## Phase 0 — Baseline, inventory and feasibility gate

**Purpose:** Know precisely what must be retained and where source/index data comes from.

Tasks:
- [ ] Record every existing browser, CLI and daemon capability; link it to a target screen and an automated/manual test.
- [ ] Benchmark standalone file, standalone diff, and daemon sessions at 10/100/1000/5000 sections. Record server analysis time separately from HTML size, browser first paint, section selection, network request count, RAM and model requests.
- [ ] Capture NVDA/Edge keyboard walkthrough and focus/announcements of the current interface.
- [ ] Inspect `SemanticDocument`, `FileAnalysis`, `ChangeSet`, `SemanticUnitChange`, before/after and source-region identity. Prove whether complete *unchanged + changed* diff-file section index is possible from existing data; specify analyzer/API additions if not.
- [ ] Audit all file/diff/generation/cache endpoints and existing Playwright tests; record invalidation and security assumptions.
- [ ] Freeze UI contract examples and agree on selected-file behavior (file selection lists sections, does not fetch a default section), previous/next stopping at current file boundary, tabs' manual activation, temporary override scopes.
- [ ] Decide final frontend route namespace and initial fixture corpus; use stable test data rather than guessing section counts.

**Artifacts:** measured baseline, fixture inventory, proposed API JSON examples verified against Rust, feature-parity checklist.

**Exit:** all 3 serving modes have a documented path, analyzer/index completeness proven or scheduled explicitly, regression cases identified.

## Phase 1 — Correctness and API foundation

**Purpose:** Make metadata-only retrieval trustworthy; eliminate known persistence risks.

Tasks:
- [ ] Refactor code-section index projection into reusable Rust service for file/diff/daemon modes, explicitly handling unchanged sections, deleted/before-side sections, renames/copies, unsupported languages and empty files.
- [ ] Add versioned workspace metadata, lightweight index and source-detail routes; never embed all source in index.
- [ ] Define snapshot/version checks, safe opaque IDs, HTTP error contract, capability flags and safe text escaping.
- [ ] Test source-detail responses cannot read arbitrary filesystem paths and reject IDs from another repo/snapshot.
- [ ] Preserve current generation routes and serialized GPU execution.
- [ ] Fix/reconcile shared config writer locking and format-preserving TOML edits before expanding browser learning saves; add concurrent stale-revision and comment-preservation tests.
- [ ] Add Rust API fixtures for 0/1/many sections, different source modes, unavailable endpoints, stale IDs.
- [ ] Ensure read-only GETs do not cause model inference or mutate learning config.

**PR boundaries:** (1a) shared analyzer projection + tests; (1b) APIs + schema tests; (1c) persistence correction + race tests. Split if risky.

**Exit:** metadata is complete without bulk source; selected source can be fetched and validated; config writes no longer lose unrelated changes/comments; existing Rust tests green.

## Phase 2 — Svelte 5 and local asset foundation

**Purpose:** A fast, independently navigable shell delivered by Rust.

Tasks:
- [ ] Add minimal Svelte 5 + TypeScript + Vite app; lock dependencies.
- [ ] Add clear development and production build workflow, accessible `skip` links, named landmarks, status outlet, error boundary and help skeleton.
- [ ] Build Explore / Personalization / Settings as top-level **links** with active location and browser history; preserve Explore state when leaving.
- [ ] Implement design tokens and light/dark/system themes with keyboard focus styles.
- [ ] Bundle assets into/package with Rust binary without a runtime Node dependency; route fallback must not swallow `/api` errors.
- [ ] Update CI for `svelte-check`, TypeScript build, Vite build and packaged binary smoke test.
- [ ] Keep legacy UI available behind an explicit development route/feature switch; do not change production default yet.

**Exit:** packed local binary displays accessible shell rapidly with zero model calls, fully offline once model-independent assets are available; destination navigation supports keyboard and Back.

## Phase 3 — Explorer and lazy source (first complete end-user slice)

**Purpose:** Deliver the central user mental model before migrating secondary controls.

Tasks:
- [ ] Build Files list and **complete code-section index for selected file** from Phase 1 APIs; include unchanged sections and accurate source status as supported.
- [ ] Implement section search, filters, result counts, Clear filter and empty/unsupported states.
- [ ] Render one selected section with source on demand; plain-text and syntax-highlighted reading; optional selected-section diff. Avoid heavyweight editor libraries.
- [ ] Add Previous/Next section within visible filtered **current-file** sequence, disabled at boundaries; selected-outside-filter behavior explicit.
- [ ] Implement URL, Back/Forward, direct link and selected-source memoization; abort obsolete navigation fetches and reject stale data.
- [ ] Implement focused section-heading navigation and no keyboard traps with NVDA.
- [ ] Measure initial response payloads and request counts; absolutely no `sections × views` initialization.
- [ ] Cover file/diff/daemon contexts with Playwright and API integration tests.

**Exit:** user can select a file, read all its section *names* and rapidly open any selected source without generating or loading every source body. NVDA navigation passes the core journey.

## Phase 4 — Explanation views and inference parity

**Purpose:** Restore full end-to-end explanation functionality in the Reader.

Tasks:
- [ ] Implement manual-activation ARIA tabs: Overview, Changes when supported, Concepts, Deep. No next/previous explanation controls.
- [ ] Support targeted cached-result lookup, results from previous policies, correct generated-vs-cached labeling, per-view loading and empty states.
- [ ] Integrate explicit Generate and Regenerate with existing endpoints. Preserve model queueing, responses for off-screen sections, no duplicate submits and persistent result association.
- [ ] Implement source annotation rendering, concept feedback entry points, valid region linking and robust plain-text source accessibility.
- [ ] Preserve exact Deep prerequisites: Overview for file, Changes for diff; test invalidation when prerequisite is regenerated.
- [ ] Add "Go to explanation", single live status and operation-specific errors including stale source.
- [ ] Prove tab focus navigation does not send model calls; switching tabs may only check saved results.

**Exit:** all explanation views functional in both modes, feature parity with current generation/cache behavior, no stale/policy cross-contamination.

## Phase 5 — Contextual customization and Personalization

**Purpose:** Enable both local and reusable learning customization without clutter.

Tasks:
- [ ] Add **Customize this section** disclosure in Reader, default collapsed; current resolved policy/provenance visible.
- [ ] Preserve existing per-section **per-view** overrides and concept feedback; add **whole-section** temporary override with explicit field-wise precedence and reset semantics.
- [ ] Implement inherited vs explicit values, saved-result settings vs pending settings, no auto-regenerate.
- [ ] Build Personalization route, profile selection, repository opt-in, session preferences, language overrides, known/learning lists, profile editor and feedback Save.
- [ ] Make drafts survive validation failures and HTTP 409; no silent profile mutation or cross-repo credential changes.
- [ ] Ensure configuration writes use corrected locking/revision behavior from Phase 1.
- [ ] Test profile vs section vs view precedence, arrays, language cases, reset scope, multiple open sessions and source refresh.

**Exit:** every current learning feature supported, new section scope verified, keyboard-complete forms, no inference on setting changes, no destructive config update.

## Phase 6 — Settings, visual polish and responsiveness

**Purpose:** Deliver a cohesive UI without hiding advanced functionality.

Tasks:
- [ ] Build Settings route with appearance, accessibility/code display, model/runtime status and diagnostics.
- [ ] Keep model endpoint/credential management CLI-first unless a separately reviewed secure UI workflow is implemented. No decorative nonfunctional controls.
- [ ] Implement light/dark/system and forced-colors behavior, high contrast, reduced motion, scalable text, wrapping, responsive stacked layout, and distinctly visible active vs focus states.
- [ ] Ensure navigation to Settings and back restores Reader context and does not generate.
- [ ] Add discoverable keyboard help and clear action names.
- [ ] Audit every user-visible legacy feature against migration inventory; identify anything deferred openly.

**Exit:** visual and keyboard design tests pass at mobile/narrow, desktop, high zoom and forced colors; all shipped controls operate correctly.

## Phase 7 — Full accessibility/performance QA and cutover

**Purpose:** Prove outcomes, not just component completeness.

Tasks:
- [ ] Run the [test matrix](test-plan.md): manual NVDA/Edge browse/focus mode, keyboard-only, Chromium/Playwright, axe checks, automated Rust API/invariant tests.
- [ ] Validate all three modes, stale snapshots, browser history, concurrent queued inference, policy switching, error and draft conflict scenarios.
- [ ] Re-run 10/100/1000/5000 section performance fixtures. Compare to baseline and investigate regressions, not just target thresholds.
- [ ] Confirm zero model calls from navigation/filter/view/profile operations and no O(sections × views) network explosion.
- [ ] Verify packaged/release binary includes assets and can run without Node/npm installation.
- [ ] Set Svelte workspace as default only after all release criteria pass; keep fallback for a short stabilization interval if operationally justified.
- [ ] Remove `src/web.rs` rendering, `src/web.js`, `src/web.css` only after parity and rollback evidence; update README/help/docs/tests/Taskfile and prune unused dependencies.
- [ ] Run security review for loopback serving, file paths, CSP, HTML escaping, same-origin/token on writes and secrets.

**Exit:** release checklist signed off, issues categorized/closed or explicitly accepted; old UI safely removable; support and troubleshooting docs current.

## Acceptance gates at each merge

Every implementation PR should state:

```text
- Which UX requirements and screen(s) it implements
- Mode coverage: standalone file / standalone diff / daemon
- Keyboard: focus entry, navigation, back behavior and Escape where relevant
- Screen reader: accessible labels/roles/status and manual verification notes
- Loading/error/empty states
- No-inference proof for navigation paths
- Snapshot/policy/cache identity impact
- Unit/API/browser tests added or changed
- Before/after payload, request count, latency (when relevant)
- Security and rollback considerations
- Screenshots for visual review AND textual steps for screen-reader review
```

Do not accept only screenshots as proof of accessibility.

## Risk register and mitigation

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Incomplete diff-file index | “All code sections” may be false if only changed units are available | Phase 0 proof and Phase 1 analyzer projection |
| New frontend causes model request during navigation | Unexpected quota/cost and slow UX | Explicit-only generation contract + request spy tests |
| Lost policy/source identity | Incorrect explanation associated with wrong context | Rust authority, typed IDs, fixture/concurrency tests |
| Expanded settings persistence damages TOML | Existing comment loss and concurrent writer race | Phase 1 correctness gate before new profile editor |
| NVDA regression from reactive rendering | Focus loss, noisy announcements, unreadable source | Stable keyed components, single live status, manual NVDA per phase |
| Frontend build breaks single-binary workflow | Installed app stops working without Node | Bundled assets and release smoke test |
| Large metadata index becomes slow | Source lazy loading alone is insufficient | Measure metadata size, pagination only when needed and accessible |
| Unsupported file/unchanged/deleted behavior | Confusing missing action or wrong version | Backend capability flags and explicit labels |
| Scope confusion: model vs learning vs local override | Wrong settings or unexpected inference cost | Distinct screens, scope labels and generated-vs-pending policy |
| Aggressive virtualization | Invisible elements to assistive tech, unstable focus | Avoid until measured and validated |
| Feature loss during rewrite | Old browser actions disappear unnoticed | Frozen inventory + per-phase parity checklist + staged cutover |

## Suggested first three implementation PRs

1. **test/bench: capture UI baseline and feature parity** — fixtures for 10/100/1000 sections and NVDA route/focus checklist.
2. **feat(api): complete section index and source-detail endpoints** — all modes, versions, stale identity and no-bulk-source tests.
3. **feat(ui): Svelte shell and accessible Explorer/Reader skeleton** — local bundling, keyboard navigation, loading/recovery, no inference.

The implementation should then follow Phase 4 and beyond. These are proposed work items; **no implementation PRs are created by this docs-only change**.


## Annotation target repair (2026-10-23)

- [x] Reproduce whole-function targets in Changes and Concepts on the reported section.
- [x] Delegate the focused implementation to agy; preserve its prompt, logs, initial diff and report under ignored `target/learning-evaluation/agy-tasks/annotation-regions/`.
- [x] Review the delegated diff independently; reproduce and repair deleted-source target mismatch, malformed-source fallback, overlapping range normalization and nested expression splitting.
- [x] Share deterministic statement targets between model requests and the source reader; retain original changed-block coverage and independent explanation keys.
- [x] Run `task check` and focused prompt tests.
- [x] Complete all 81 browser interaction checks in one fresh run; rebuild/install and verify the installed daemon exposes targets 1–6, 7, 8, 9 and 10–12 for the reported 12-line function.
- [x] Review fresh local-model Changes, Concepts, Overview and Deep results after restoring the endpoint: all generated successfully with independent identities; Changes and Concepts each used three distinct valid targets. Remaining output issues: one Changes annotation mentions the adjacent retryability assertion; Deep uses illustrative error representations as concrete values. See ignored `target/learning-evaluation/agy-tasks/annotation-regions/live-review.md` for exact evidence. Manual browser screenshot remains unavailable because browser security policy rejected access.

`task install` continues to install locked frontend dependencies, rebuild embedded frontend assets and rebuild the release binary before replacing the installed application.


## Grounded explanations and one-request generation

Design: keep the existing per-view request and shared source reader. Each model annotation must return a nonempty `source_quote` of at most 240 Unicode characters copied exactly from its selected region. Validate evidence before accepting or caching a result; preserve quotes as validation-only data rather than adding a second source panel. Exact substring matching deliberately rejects whitespace-normalized or neighboring-region quotes. Supply up to eight copyable line excerpts per target (160 characters each) and constrain source_quote choices in the schema, so multiline code need not be reformatted by the model. Added/Deleted request patches contain only their selected section; unrelated file-wide changes cannot establish replacement relationships. A valid quote proves location, not truth of the accompanying prose.

Annotation instructions must restrict every claim to the selected region while allowing surrounding source as context. Deep must use visible expressions for causal explanations, avoid invented object layouts, distinguish unseen-helper behavior from asserted expectations, and omit examples that require guessing. Keep its existing structured shape.

Each Generate action makes at most one generation attempt. Local validation, context preflight and metadata discovery do not generate text; invalid output surfaces a recoverable error and requires an explicit new action. Existing automatic output/context retries are removed from the generation entry point. No inference concurrency is added.

Acceptance tasks:
- [x] Observe failing grounding and one-request regressions, implement schema/prompt/local validation and user-facing errors. Pair each region ID with its own quote choices in schema branches; preserve local exact-match validation. Deep generates directly without silently generating a prerequisite. First-attempt failures retain history/context-pressure telemetry; Ollama loading returns no generated text.
- [x] Update mock contracts and regression/browser expectations for explicit retries; cover missing, blank, oversized, neighboring, invented and valid multiline quotes.
- [x] Compare fresh baseline and candidate outputs across adjacent assertions, multiline expressions, deleted code, guards and unseen helpers; record raw outputs, validity, factual/target accuracy, latency and request counts. Review prose independently of validators. Latest candidate: 13/13 accepted, 28/28 quotes matched selected regions, one generation request each; median 19.52 seconds versus baseline 15.60 seconds in an ordered sample with different warm conditions. No general performance or accuracy claim follows.
- [x] Run task check and browser suite, update evidence, then task install and verify installed behavior.

Evidence and known limits (2026-10-24): ignored `target/learning-evaluation/grounding/report.md` contains the primary source/output review, executable identities and raw comparison references. `check-shipping.log` records the passing full check; `browser-reviewed.log` records 85 passing browser tests. Independent review caught failure-history and generating-warmup regressions, both repaired with failing-then-passing coverage. `install.log` records frontend dependency installation, frontend/release rebuild, daemon stop/install/restart. Installed and evaluated executables differ only in verified PE build timestamps/GUID, with all remaining bytes identical (`installed-verification.json`). The installed daemon exposes the reported function's targets 1–6, 7, 8, 9 and 10–12. A fresh live Concepts result (25.61 seconds, cache_hit false) correctly targets the error chain, equality assertion and retryability assertion separately at lines 7, 8 and 9; see `installed-section.json` and `installed-concepts.json`.

Quote validity is not a factual guarantee. The v19 comparison loses equality precision in one Changes explanation, speculates about custom field/helper semantics in Concepts, and uses an unconditional-return phrase in a Deep purpose despite correctly explaining panic/divergence in its walkthrough. Existing macro-punctuation sentence truncation can clip an Overview. These were explicit quality limits, not passing teaching assertions; the source-facts follow-up below records the newer comparison. Manual access to the user's page remains blocked by browser security policy; no manual screenshot is claimed.

## Tree-sitter source facts (2026-10-24)

Approved implementation: [source-facts plan](../superpowers/plans/2026-10-24-source-facts.md). Delegate extraction and existing-request integration to agy, then independently review the task-start diff and test outputs. Require real coverage for all supported languages (Rust, Python, Go, Java, C#, JavaScript, TypeScript, C++, C), including actual code-section fragments. Facts describe observed syntax with exact spans and revision provenance; they do not resolve unknown helper behavior or inferred types. Keep the existing one-request-per-action, response shape and shared-source UI. Bound prompt evidence and include its policy/content in cache identity. Artifacts: ignored `target/learning-evaluation/agy-tasks/source-facts/`.

- [x] Reviewed implementation and all-nine-language extraction coverage
- [x] File/diff/deleted source identity, target/context membership and cache checks
- [x] Full regression and browser checks
- [x] Fresh serialized live output review against retained baseline
- [x] Rebuild/install and verify the installed daemon

Acceptance evidence: `check-final.log` records the passing full check and 14 independent source-facts contract tests; `browser.log` records 85 passing isolated browser tests. The candidate live run accepted 13/13 fresh results with 28 valid quotes and one generation request per action. Sources, region catalogs and settings match the retained v19 baseline. Reported prompt tokens rose from 20,631 to 23,351 (~13.2%); median latency was 19.43 versus 19.52 seconds in ordered runs with different warm conditions, so no general performance claim follows. The guard Changes explanation now preserves `>=` exactly. Helper prose is more cautious but still contains unsupported “valid value” wording; Deep can repeat syntax rather than explain causes. The deleted-tail “return statement” terminology and Overview macro-punctuation truncation also remain. See ignored `target/learning-evaluation/agy-tasks/source-facts/report.md` for per-case primary review and raw evidence, not just validator success.

`task install` rebuilt locked frontend dependencies, bundled assets and the release binary, then restarted the daemon. Installed SHA256 is `c9ac67e4dd5c87453e6aac4adc4c592feedcc513166367099bcb1290e8020709`; evaluated and installed bytes match except parsed PE build timestamps/GUID. Fresh installed Concepts on the reported function completed in 26.93 seconds with cache_hit false, targeting lines 7, 8 and 10–12 separately. That result still describes the message field as a string from its contains call; the local syntax evidence does not prove that type. Facts improve the supplied evidence and location reliability; they do not guarantee model prose correctness. All nine languages have extraction coverage, while the controlled live teaching comparison is Rust-only.
