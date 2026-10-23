# Validation, test matrix and release acceptance

> **Status:** Active test plan for default UI (`/` standalone, `/sessions/{id}` daemon; `/workspace` aliases; `/legacy` fallbacks). Manual NVDA testing and native zoom checks are explicitly deferred, not passed.
>
> Related: [screens](screens.md), [interaction contract](interaction-accessibility.md), [architecture](architecture-api.md), [phases](implementation-plan.md).

## Validation strategy

Three complementary layers are required:

1. **Rust unit/integration:** snapshot analysis, full section index, stable IDs, source detail, learning policy and cache identity, generation restrictions, concurrent config writes.
2. **Automated browser:** Playwright keyboard, routing/focus, DOM/ARIA, request spies, error simulations, network/large-data behavior. Include automated accessibility analysis (axe or equivalent) but do not treat it as complete accessibility coverage.
3. **Manual keyboard/NVDA:** NVDA browse-mode and focus-mode behavior, spoken announcement volume and meaning, source indentation, tab behavior, stale/generation transitions and visual high contrast.

Test standalone file, standalone diff and daemon session modes; APIs and capabilities differ.

## Critical scripted end-to-end journeys

| ID | Steps | Expected outcome |
| --- | --- | --- |
| E2E-01 | Open app with a many-file fixture | Small shell; repository context and file index; no source for unselected sections; no model requests |
| E2E-02 | Select `src/server.rs` | Lists all discovered code sections in source order; automatically selects first section (or restores last-read section in workspace), fetches only that section's source, focuses Reader heading once ready, adds one history entry with section; no model requests (empty files have no source; invalid/stale explicit section IDs error) |
| E2E-03 | Select `generate_file` section with keyboard | Only selected source fetched; Reader h2 focus once source ready; file identity, section name and position announced once; zero inference |
| E2E-04 | Select another function then Browser Back | Original function, view and valid cached source restored; zero inference |
| E2E-05 | Filter sections so current selection is hidden | Reader retains current section labeled outside filter; Next/Previous disabled; Clear restores list |
| E2E-06 | Use Previous/Next at file boundaries | Stops/disabled; does not switch file; label indicates intended target within same file |
| E2E-07 | Focus Overview tab; arrow to Deep; press Enter | Arrow focus only; Enter activates Deep in client state; zero network requests and no generation until Generate |
| E2E-08 | Generate Overview; navigate away during request | Current selection stays correct; result stored under original section/policy/snapshot; no duplicate |
| E2E-09 | Generate Deep without prerequisite; regenerate prerequisite | Documented prerequisite sequence under explicit action; invalidates dependent variants only, no cross-policy result loss |
| E2E-10 | Change section-wide language help; keep view-specific focus override | Field-wise precedence visible; only affected section/views change pending policy; existing explanation remains labeled with old policy |
| E2E-11 | Override one Deep view, return to Concepts | Concepts inherits section/profile except its own override; reset one scope does not remove another |
| E2E-12 | Save concept feedback for selected profile | Temporary category changes; only explicit Save writes; opposite list pruned, revision and UI state refreshed |
| E2E-13 | Simulate HTTP 409 while saving profile | Typed draft not lost; stale warning + resolution path; no false success |
| E2E-14 | Switch Personalization and Settings, then back to Explore | File/section/view retained; no model request and no unexpected source reload |
| E2E-15 | Refresh Git snapshot during old inflight source/result | Old response cannot overwrite new selection; stale state explained; safe reload |
| E2E-16 | Diff with deleted/renamed/unchanged code sections | Source side and change status accurate; only supported explanation actions enabled |
| E2E-17 | Source fetch fails/model endpoint unavailable | Source error has Retry without inference; model failure does not block source reading |
| E2E-18 | File mode: inspect Overview/Concepts/Deep | No Changes tab; Deep uses actual Overview prerequisite |
| E2E-19 | Daemon: close/reopen session then follow stale deep link | Explicit expired/invalid session handling; no mixing data across sessions |
| E2E-20 | Increase zoom/force colors and navigate complete workflow | No loss of functionality/focus, status not color-only, no horizontal page-level clipping |
| E2E-21 | Generate every available view, reload with Deep selected, then switch through all views | Every saved result is restored on section selection; tabs make zero requests and no additional model calls; file mode omits Changes |
| E2E-22 | Generate Concepts while the model is held, activate Overview, then release; fail Concepts regeneration | Completion preserves Overview selection and leaves its source unannotated; Concepts retains its result on failure; error stays scoped to Concepts |
| E2E-23 | Generate Changes/Concepts, follow an annotation, switch to Overview and back, then select an unchanged section | Annotation links focus the shared source; active view alone controls highlights; unchanged diff sections expose reading without generation |
| E2E-24 | Generate Changes, enable plain source, follow an annotation, open Git diff and previous source | Annotation focuses the plain-source field; current and previous code retain correct revisions; one active source container; no extra model calls |

## Explicit inference/cost assertions

Instrument the model provider or generation endpoint and assert **zero calls** for:
- Initial app/navigation/index requests.
- Select file / expand file / filter sections.
- Select section / Previous/Next / Back/Forward.
- Read/copy source / show Git diff.
- Change explanation tab, even to Deep (tab selection is client-side navigation with zero network requests and zero inference).
- Change learning profile, session or local overrides.
- Open/close Personalization, Settings or Help.
- Save feedback or learning config.
- Source prefetch / cached-result lookup.
- Change theme, font size or code wrapping.

Assert **exactly the expected authorized generation sequence** when Generate or Regenerate is activated, including any required prerequisite within that explicit operation. Record model-call target, kind and policy identity. Do not assume Deep always requires one network request.

## API contract tests

- Correct metadata index for file/diff/daemon; zero/one/1000+ sections.
- All discoverable sections appear once with stable IDs and correct source order, including unchanged and before-side/deleted where represented by analyzer.
- `GET index` does not carry full source blobs or generated explanation content.
- `GET section` returns only requested source and its snapshot; no arbitrary filesystem read/path traversal or cross-session access.
- Stale/unknown ID yields defined 404/409/410/422 semantics, not unrelated section data.
- Snapshot update between index/source fetch returns explicit conflict, never silently mixed source.
- Escaping of source, names, annotation titles and model output prevents HTML/script injection.
- Caching identities include source/snapshot, kind, resolved policy, model/config versions and Deep prerequisite ID.
- Saved result with different policy is displayed only with correct “used settings” annotation.
- Learning save uses same-origin token, revision conflict, shared writer lock, preserves comments/unrelated TOML, and retains draft on UI conflict.
- All read-only operations do not trigger inference or persistent configuration writes.

## Keyboard and screen-reader checklist

> **Note:** Manual NVDA testing is explicitly deferred (scheduled for later, NOT passed). Checkboxes remain unverified pending manual testing.

Manually test with **NVDA + Windows + Edge**, and keyboard-only without NVDA. Additional Chromium/browser checks recommended.

- [ ] Skip-to-main, application navigation and Explorer/Reader landmarks are discoverable.
- [ ] One main landmark and sensible headings; heading navigation reports context correctly.
- [ ] File entries announce name, change status, and number of sections.
- [ ] Selecting file automatically selects first section (or restores last-read section) and focuses Reader heading once source ready; initial app loads only metadata; zero inference.
- [ ] Selecting a section focuses named Reader heading exactly once; repeated async state changes do not steal focus.
- [ ] Source can be read line by line, including indentation and punctuation as configured in NVDA.
- [ ] Only one of highlighted/plain-text source representations is exposed at once.
- [ ] Previous/Next within file has discoverable labels, disabled state, and proper focus behavior.
- [ ] Explanation tablist works with Left/Right, Home/End, Enter/Space; arrow focus does not activate a view.
- [ ] Active tab and explanation panel have correct accessible names and selection state.
- [ ] Generate retains focus. Queued, completed and failed states announced briefly and only once.
- [ ] “Go to explanation” reliably moves focus to content heading on request.
- [ ] Customize disclosure and scope-specific fields are navigable with native keyboard controls.
- [ ] Saving a profile and HTTP 409 conflict do not discard keyboard focus or draft.
- [ ] Browser Back and destination changes restore appropriate navigation focus/section.
- [ ] Filter status not announced on every keystroke; no multi-status flooding.
- [ ] Error/empty/disabled states provide text explanations, not icon/color alone.
- [ ] No global shortcuts interfere with NVDA quick navigation or browser Find.

Capture test steps, NVDA spoken output summary, expected-vs-actual and severity for failures. Test with NVDA in both browse and focus modes. Do not assume passing automation proves screen-reader usability.

## Visual and responsive checklist

> **Note:** Native zoom checks and visual high-contrast manual verification are explicitly deferred, NOT passed.

- [ ] Light, dark, system, high contrast and forced-colors styles have readable contrast and visible focus.
- [ ] Focus outline and selected-item indicator are distinguishable.
- [ ] Added/modified/deleted/unchanged and cached/generating/error statuses include readable text.
- [ ] No hover-only controls; all buttons have visible labels or robust accessible names.
- [ ] 200% and 400% zoom maintain access to Explorer, Reader, Customize, tabs and Settings.
- [ ] Narrow viewport reflows to a logical stacked UI; no keyboard focus under overlay.
- [ ] Code can wrap or scroll without page-level horizontal overflow; scroll container is usable by keyboard.
- [ ] `prefers-reduced-motion` respected; loading feedback doesn't require animation.
- [ ] Component tree does not create multiple hidden focusable tabpanels or duplicate landmarks.
- [ ] Source/explanation content maintains reading order independent of CSS grid position.

## Performance benchmark specification

Test 10, 100, 1000 and 5000 sections. Include 1 KB and very long sections; at least one large function with long lines, and one file containing hundreds of functions. Record both warm/cold caches.

**Metrics must be split by stage** rather than reporting a single ambiguous “load time”:
- Rust analysis/session creation before HTTP readiness.
- HTML shell transfer size, JS/CSS uncompressed and compressed sizes.
- Time to usable app navigation after server ready.
- Index request bytes/time and client render time.
- Source detail request bytes/time and render time; cached section switching.
- API request counts by type on initial load and per navigation gesture.
- DOM node count and JS heap after idle, Explorer open, and repeated section switching.
- Cached-result lookup latency.
- Model calls and inference duration (reported separately; **never** included as frontend navigation latency).
- NVDA focus response under same scenarios.

**Initial engineering targets** (must be calibrated to actual benchmark hardware and fixtures):
- Shell usable within 1 s of server readiness.
- 1000-section navigation index available within 2 s.
- Cached section switching target <100 ms.
- Uncached local source target <200 ms.
- Zero model calls on navigation and settings operations.
- Zero initial all-section/per-view learning restore calls.
- No full-document source HTML generation on shell response.

Treat these as targets, not measurements or universal hard SLAs. Track regressions against Phase 0 baseline, including browser long tasks and memory growth. Latency measurement must include realistic slow-mode fixture if simulated.

## Automated test scaffolding

- Rust: pure analyzer/index projections, API handler tests, snapshot/policy concurrency tests, file mode and daemon mode test fixtures.
- Playwright: Chromium tests with keyboard `press` and focus assertions; intercept `fetch` to spy API route and model call counts; simulate delayed responses, stale snapshots and HTTP 409.
- Accessibility: axe checks in browser suite; Svelte compiler accessibility warnings are blocking unless narrowly documented and manually verified.
- Build: install pinned Node/npm once, `npm ci`, `svelte-check`, frontend build, Rust fmt/test/build, packaged-binary smoke with no Node runtime and no external asset dependency.
- CI artifacts: browser traces and accessible textual test report on failures; avoid publishing user source or credentials in logs.

## Release gate

The new UI is the default interface (`/` standalone, `/sessions/{id}` daemon; `/workspace` aliases). Legacy code and safeguards are retained at `/legacy` and `/sessions/{id}/legacy` until stabilization.

Release/cutover acceptance:
1. Every critical E2E scenario above passes, with any deferrals explicitly accepted.
2. No P0/P1 accessibility, source identity, policy identity or inference-trigger regression remains.
3. Manual NVDA/Edge main workflows and native zoom checks are explicitly deferred for later validation (not passed).
4. Target metrics or approved evidence-based alternatives pass on documented hardware; no unacceptable baseline regression.
5. Static assets are bundled, secure, and run from the distribution binary without Node/npm.
6. Existing file/diff/daemon modes and learning generation are behaviorally compatible.
7. Browser profile save does not destroy comments or lose concurrent config writes.
8. Feature inventory matches shipped screens and documentation.
9. Legacy renderer is retained as fallback at `/legacy` and `/sessions/{id}/legacy` until stabilization; safeguards and fallback pathways are verified.

If any gate fails, do not erase the old implementation solely to meet a migration date.
