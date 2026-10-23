# Workspace acceptance record

Date: 2026-10-23. Branch: `codex/svelte-workspace`. This record distinguishes automated implementation evidence from manual release approval. The user requested the new UI as default and deferred manual NVDA/zoom testing. Legacy rendering remains at explicit fallback routes.

## Scenario evidence

The scenario IDs refer to `test-plan.md`. Names below identify actual tests in `tests/browser/workspace.spec.mjs`, with backend evidence in `tests/workspace_http.rs` and `src/workspace.rs` tests. Fixture names differ from the illustrative product names in the specification.

| Scenario | Evidence | Remaining scope |
| --- | --- | --- |
| E2E-01 | Lazy file/section navigation in file/diff/daemon; source-free HTTP index; benchmark request counts | NVDA main navigation |
| E2E-02 | Complete metadata projection; selecting file loads only first/last-read section source | Manual section-list discovery |
| E2E-03 | Selected source and focused Reader heading assertions | Spoken name/position exactly once |
| E2E-04 | Browser history restores selected source without generation | NVDA history focus |
| E2E-05 | Filters retain Reader and disable section navigation outside filter | Manual filter announcements |
| E2E-06 | Previous/Next file-boundary assertions | Manual disabled labels |
| E2E-07 | Manual tabs and zero activation requests, stricter than proposed cache-fetch permission | NVDA browse/focus modes |
| E2E-08 | Explicit jobs survive navigation; model work serialized; original-target result association | Spoken completion while offscreen |
| E2E-09 | File/diff Deep prerequisites and same-policy dependency invalidation | Real-model teaching quality not inferred |
| E2E-10 | Section/view precedence, pending settings and actual used policy assertions | Manual policy reading |
| E2E-11 | Independent scope reset and Concepts policy assertions | Manual editor keyboard workflow |
| E2E-12 | Explicit feedback save, opposite-list pruning; delayed save across navigation passes unchanged | Manual save announcements |
| E2E-13 | HTTP 409 preserves draft; review latest and explicit save; profile collision protection | Manual conflict focus |
| E2E-14 | Destinations preserve selection and appearance changes issue no generation | NVDA destination return |
| E2E-15 | Late source response rejected; daemon stale comparison disables Generate and reloads safely; legacy refresh concurrency tests | Manual reload announcement |
| E2E-16 | Rust + HTTP unchanged/deleted projection, renamed old/new labels and moved-source association, old source, opaque IDs; browser mode capabilities | Manual deleted-source reading |
| E2E-17 | Source failure/retry browser case; existing model-error backend/legacy tests | New-pane model-error/retry and retained Generate focus test passes |
| E2E-18 | File capabilities omit Changes; Deep test uses Overview | Manual file-mode tabs |
| E2E-19 | Daemon expired/stale response tests; cross-session section IDs rejected by HTTP | Manual stale deep link |
| E2E-20 | Narrow viewport, forced colors, reduced motion, theme controls and axe checks | Native browser 200%/400% zoom and NVDA |

## Commands and artifacts

- `task check`: refreshed run exit 0, `target/learning-evaluation/agy-tasks/ui-redesign/default-final-check.log`.
- `npm run test:browser`: default-cutover run reports 59 passed in `default-full-browser.log`, including legacy/workspace tests and the new model-error/retry focus regression. The subsequent repository check completed successfully; the release rebuild and all-mode packaged benchmark smoke completed successfully.
- Focused delayed feedback save: 1 passed without production changes. Despite its intended RED-stage filename `feedback-navigation-red.log`, it records existing behavior passing, not a reproduced defect.
- Benchmark runner: `scripts/workspace-benchmark.mjs`; full 10/100/1000/5000 matrix, six direct cached-event cases, and long-source cases. Measurements and limits are in `implementation-status.md`.
- agy read-only review: completed with nonempty response and no denied actions, conversation `a5f25a90-f081-4d6a-8de3-9f8ca9f8d3fc`; primary adjudication is retained beside the frozen review snapshot. Preliminary findings are not accepted solely on Flash's assessment.

## Explanation parity validation — 2026-10-23

Ten additional browser cases cover all-view reload restoration, completion after tab changes, view-scoped regeneration errors, annotation focus/highlights on one shared source, unchanged-section reading, and plain-source access to Git diff and previous source. Restoration, completion/error isolation, and annotations run in standalone file, standalone diff, and daemon modes; file mode omits Changes. The scripted journeys are E2E-21 through E2E-24 in `test-plan.md`.

An observed defect preceded the fix: all three reload cases failed because only the active view was restored, leaving previously generated views showing Generate after a reload. `ExplanationPane` now checks saved results for every available view when selecting a section or changing preferences. Tab activation remains local and never generates or loads results. No generation, policy identity, transport, or security routes were changed.

Fresh verification: `task check` exited 0 (Svelte/TypeScript, embedded bundle rebuild, Rust formatting, complete non-opt-in Rust tests and whitespace). The first workspace run passed 30 cases after the fix; the final `npm run test:browser` exited 0 with **69 passed**, including **34 workspace cases** and 35 legacy cases. `frontend/dist/workspace.js` was rebuilt for embedding. Existing Rust unused/dead-code warnings remain. This run used the deterministic mock model through real application processes and Chromium; no fresh live-model teaching-quality evaluation or manual NVDA/native-zoom test occurred.

## Manual test record

User initially agreed to run NVDA + Windows + Edge, then explicitly deferred manual testing and requested the new UI as default. File and diff previews use isolated generated repositories, mock generation, and a frozen executable. No manual results have been reported yet. The checklist in `test-plan.md` remains unverified until actual observed behavior is recorded. Do not infer NVDA usability from axe or focus assertions.

No fresh live-model quality evaluation occurred. Release build, distribution smoke and performance measurements are recorded below and in implementation-status.md. Manual testing remains unverified; default cutover is an explicit user-approved exception, with legacy fallback retained. No commit, PR, publishing or legacy removal has occurred.

## Security review scope

Primary inspected the current workspace handlers and embedding code. Workspace/index/section are read-only projections; source access is restricted to opaque IDs in the current immutable workspace, with required snapshot and daemon-session namespace checks. Runtime projection exposes repository basename, app version and model name, not model endpoint or credentials. Source and explanation text render through escaped Svelte expressions, not injected HTML. Assets use same-origin CSP with no inline/eval permissions, `nosniff`, ETags and content fingerprints. No new external origin is configured. Existing loopback binding, daemon-control token checks and same-origin/token learning save checks remain in their original paths; tests exercise unchanged security behavior and cross-session/stale source rejection. This is a scoped implementation review, not a claim of a comprehensive penetration test.

The native disabled Generate focus failure was reproduced by the new model-error test, repaired with guarded `aria-disabled`, then passed with serialized jobs (2 tests, focus-green.log). Full browser/repository/release verification was refreshed after this last product change, as recorded below.

Final refreshed verification: final-check-focus.log completed successfully; release-build-focus.log completed release build successfully. Copied release executable packaged-workspace-focus.exe passed all three mode smoke cases in package-smoke-focus.json. The renamed-file fixture and source association passed all three tests in rename-http.log after the full check. Manual results and native zoom remain pending.

## User-approved cutover update

The user explicitly requested new UI default and deferred manual testing. Standalone `/` and daemon `/sessions/{id}` now serve the new shell; existing `/workspace` aliases remain supported. Legacy fallback is `/legacy` and `/sessions/{id}/legacy`. File selection loads first/last-read section with one history entry and no generation. Empty files have an explicit No code sections state. First-selection and default-route regressions failed before implementation; the resulting full browser suite passed 59 cases, including legacy fallback, selection/history, empty files and source error retry. Manual checks remain deferred rather than passed. No installed global executable has been replaced.

Final default-cutover artifacts: `default-final-check.log` exit 0; `default-full-browser.log` 59 passed; `default-release-build.log` exit 0; `default-release-benchmark.json` all12 cases; `default-long-source.json` all3 long-function cases. Each benchmark asserted zero initial source/policy POSTs and zero navigation generation requests. The default preview was opened and actual file selection loaded its source and focused the Reader h2. Manual NVDA and native zoom remain accepted deferrals. Legacy removal is deferred until stabilization, with tested explicit fallback retained.
