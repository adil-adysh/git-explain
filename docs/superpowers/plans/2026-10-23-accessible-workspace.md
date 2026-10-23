# Accessible Svelte workspace implementation plan

**Goal:** Implement docs/ui-redesign end to end using bounded agy tasks and primary integration review.
**Architecture:** Rust owns immutable source, identity, policy, generation and persistence. A bundled Svelte 5 application owns navigation and drafts. Retain explicit legacy fallback routes during stabilization. User requested new UI default with manual testing deferred.
**Tech stack:** Rust/Axum, Svelte 5, TypeScript, Vite, Playwright.
**Spec:** docs/ui-redesign/README.md and its five companion contracts; docs/explanation-ux.md remains authoritative.

## Constraints and rulings
- Explicit Generate/Regenerate only; serialized generation; loopback and existing tokens preserved.
- Each explanation kind has independent backend identity; source representations share one active view.
- Authoritative explanation-ux forbids tab-selection network requests. Resolve current policy/results on explicit section or preference actions; tab activation itself remains local. Do not implement the looser proposed tab cache-fetch permission.
- User explicitly requests implementation of the supplied design and phases: use those as the approved design/plan; no duplicate design approval gate.
- Work in the existing checkout on codex/svelte-workspace; preserve task-start clean state and agy artifacts under ignored target/learning-evaluation/agy-tasks/ui-redesign.
- Do not claim manual NVDA validation or live-model quality from automation. Keep legacy available if release gates remain unverified.

## Execution sequence
- [x] 0. Inventory: audit complete-section feasibility, routes/security, feature parity; add reproducible baseline fixtures/measurements and acceptance ledger.
- [x] 1a. Add src/workspace.rs shared typed metadata/detail projection from FileAnalysis and ChangeSet; snapshot-scoped opaque IDs, source sides, capabilities, no source in index. Regression cases: unchanged/deleted/renamed, unsupported/empty, identity collisions, unknown/stale sections.
- [x] 1b. Expose workspace/index/sections routes in src/server.rs and src/daemon.rs. Read only; require expected snapshot on detail. Verify HTTP 404/409 and no inference with fixture servers.
- [x] 1c. Unify config writer locking and format-preserving TOML changes in src/config.rs/src/learning.rs; regression first for comments and ordinary-versus-learning writes.
- [x] 2. Add frontend Svelte/TypeScript/Vite build, bundled assets and explicit new UI route; CI/check/install integration. Small offline shell, links/history, single status, themes and landmarks.
- [x] 3. Explorer/Reader: metadata lists, filters, bounded snapshot-keyed lazy source, race rejection, URL/history, focus and file-boundary navigation. Browser tests E2E-01 through 06, 14 through 17 and 19.
- [x] 4. Explanation adapter and independent view states: manual ARIA tabs, explicit generation, backend policy/result identity, annotations, Deep prerequisites, in-flight navigation. E2E-07 through 09 and 18.
- [x] 5. Rust-authoritative section/view field-wise override scopes plus contextual editor, personalization and feedback with preserved drafts/conflict recovery. E2E-10 through 13.
- [x] 6. Settings/help: themes, text/wrap/source display, safe runtime diagnostics, keyboard guide. E2E-20.
- [x] 7. Full tests/build/package smoke, 10/100/1000/5000-section benchmarks, security review and acceptance report. User explicitly authorized default cutover with manual NVDA/zoom checks deferred. Retain legacy fallback until stabilization.

## Review focus
Snapshot changes during source/results; deleted/renamed identical symbols; stale config revisions and comments; off-screen model completion; very long source and narrow/high-contrast reading.

## Per-task method
Read exact assigned symbols; add focused failing regression before production edits; observe failure; implement; run focused passing checks; primary inspect diff and command outputs. agy prompts specify allowed files, exact commands and report path; inspect denied_actions and actual sessions. Do not overlap Cargo operations or shared edits. No commits or external publishing without a concrete reason.

## Acceptance
Run task check, frontend type/build checks, browser matrix, debug/release package smoke. Record performance separately from model latency. Record manual NVDA/Edge and fresh live-model runs honestly as performed or pending.

## User steering after initial acceptance

Manual NVDA and native zoom testing are explicitly deferred. New UI is requested as default for standalone and daemon entry points. File selection opens first or workspace-local last-read section, with explicit-only explanation generation unchanged. Regression verification and release smoke are being refreshed for these changes.

Final acceptance: task check and release build passed; full browser suite59 passed; packaged default routes passed12 benchmark cases plus3 long-function cases, zero navigation generation calls. User-approved manual NVDA/zoom deferral and retained legacy fallback are recorded in acceptance-record.md. No installed global binary replacement, commit or publication was performed.
