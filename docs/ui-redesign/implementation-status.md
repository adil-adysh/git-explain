# Implementation and acceptance ledger

The redesign is being implemented on `codex/svelte-workspace`. The user requested the new UI as default and explicitly deferred manual testing. Legacy remains available through explicit fallback routes. The specifications describe the target; this ledger records implementation evidence separately.

## Decisions verified against the existing implementation

- Diff mode lists files touched by the selected Git comparison. Every discovered section within those files is included, including preserved sections. `populate_semantics` already extracts complete before/after documents; a second analyzer is unnecessary. Completely untouched files remain a file-mode workflow.
- Deleted sections use before-side source and labels. Preserved diff sections are readable but do not offer explanation generation. Unsupported and binary files are excluded; supported empty files remain discoverable.
- The authoritative explanation contract forbids requests when a tab is selected. Tab activation stays local. Any saved-result lookup must occur on section/preference selection or a separately explicit action, never on tab activation.
- The new UI is at `/workspace` in standalone mode and `/sessions/{id}/workspace` in daemon mode. Destinations and opaque selection IDs use query parameters. Default entry points `/` and `/sessions/{id}` serve the new UI; legacy fallback is `/legacy` and `/sessions/{id}/legacy`.
- Built frontend assets are included in the Rust executable. The checked-in `frontend/dist` bundle allows Rust-only builds; frontend changes require rebuilding that bundle. HTML fingerprints asset URLs and the server provides ETags, a restrictive same-origin CSP, and `nosniff`.

## Backend API

Standalone prefix: `/api/ui/v1`. Daemon prefix: `/api/sessions/{id}/ui/v1`.

| Read-only operation | Response |
| --- | --- |
| `GET /workspace` | Version, mode, generation, source identity, capabilities and file count |
| `GET /index` | Complete file/section metadata, no source/diff/explanation bodies |
| `GET /sections/{id}?snapshot_id=…` | Selected immutable source, source side, old source where applicable, Git diff and valid annotation regions |

Missing source identity returns 400, unknown section 404, stale identity 409, and unavailable daemon session 410. Section IDs are opaque and scoped to source identity; daemon sessions additionally have separate ID namespaces. These routes do not resolve policies, restore results, generate explanations or write configuration.

Existing generation and learning routes retain their security and cache identities. Whole-section temporary learning preferences are now represented by `LearningSelection.section`, between `session` and the existing per-view `overrides`. Rust resolves values field by field, including omitted versus explicitly empty lists, and returns scope provenance.

## Persistence correctness

Ordinary model/config writers and learning saves share the existing `.learning.lock` transaction lock. Writers hold it across read, validation and atomic replacement. Learning contention remains a retryable conflict. TOML updates preserve comments and formatting on retained keys rather than reserializing the whole document.

Observed regression failures preceded the fixes: custom comments disappeared, ordinary writes bypassed the lock, and the section preference field was rejected. Focused passing checks: 32 configuration tests, 17 learning tests, and 7 workspace projection tests. Real-process HTTP tests cover standalone file/diff and daemon source retrieval, unchanged/deleted sections, source-free indexes and identity errors.

## Feature parity and remaining gates

| Capability | Target surface | Acceptance evidence |
| --- | --- | --- |
| Full section metadata and selected source | Explorer / Reader | Rust projection + real-process HTTP tests |
| Navigation, filters, history and focus | Explore | Browser tests: lazy navigation, history, focus, race rejection, boundaries and errors |
| Overview / Changes / Concepts / Deep | Reader | Manual tabs, independent results, explicit generation and Deep prerequisites tested |
| Plain source / Git diff / annotations | Reader | Shared source, plain text, Git diff and bounded annotation links implemented |
| Temporary section / view settings | Customize | Field-wise Rust resolution and section/view scope browser tests |
| Profiles, language preferences, lists and feedback | Personalization | Profiles, language inheritance, draft conflicts and explicit feedback save tested |
| Theme, font, wrapping and keyboard guide | Settings / Help | Theme/font/wrap/plain-source controls, retained selection and axe checks |
| CLI/debug and daemon control | Existing CLI / routes | Full task check and 59 browser tests passed |

## Performance and validation status

The reproducible runner is `scripts/workspace-benchmark.mjs`. HTTP and Chromium runs cover 10, 100, 1000 and 5000 sections in standalone file/diff and daemon modes, without model requests. A second 1000/5000-section run measures cached click-to-frame directly to separate browser response from Playwright locator/actionability overhead.

Measured on Windows x64, Intel Core Ultra 9 275HX, Node 24.18.0, using the frozen debug executable from the workspace browser acceptance build. These are single-run fixture measurements, not release-binary guarantees or NVDA measurements. Each fixture section is approximately 1 KiB. Logs and binary hashes are under ignored `target/learning-evaluation/agy-tasks/ui-redesign/`.

| Mode | Sections | Index HTTP ms | Usable shell ms | Uncached selection ms* | Cached click-to-frame ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| File | 1000 | 9.0 | 53.5 | 112.4 | 9.0 |
| File | 5000 | 56.1 | 144.6 | 417.0 | 34.3 |
| Diff | 1000 | 20.3 | 101.1 | 93.0 | 5.7 |
| Diff | 5000 | 48.2 | 53.2 | 290.9 | 20.5 |
| Daemon | 1000 | 23.6 | 51.6 | 99.1 | 8.6 |
| Daemon | 5000 | 59.8 | 77.7 | 215.8 | 37.0 |

*Uncached selection includes Playwright actionability, focus and scrolling. At 5000 sections it exceeds the initial 200 ms engineering target; further measurement and optimization remain a release consideration. Cached event-to-frame meets the 100 ms target in these fixtures. Daemon analysis/session creation is separate: approximately 394 ms at 1000 sections and 5.76 s at 5000. The shell is 550 bytes without any source bodies; the daemon 5000-section index is 1.57 MB and selected detail 2.67 KB. The same daemon run recorded about 30,229 DOM nodes and 61 MB JS heap. Large metadata lists remain an optimization opportunity.

All benchmark cases recorded zero initial source requests, zero initial policy/result POSTs, and zero generation requests. The workspace suite now has 24 passing cases; it covers all serving modes, navigation, independent generation, Deep, conflicts, scopes, source errors, queued jobs, stale/expired sessions, responsive controls and axe checks. This is automated evidence only. The final task check passed, including frontend validation, Rust tests, formatting and whitespace checks. The full legacy + workspace browser suite passed 59 tests after the default cutover, automatic selection and Generate focus fixes, including the model-error/retry case. The release build and copied-binary Chromium smoke passed in file, diff and daemon modes. Renamed old/new path labels and moved-source association additionally passed the three real-process HTTP tests.

A long-source run adds 2500 comment lines and a roughly 20 KiB line to the first function in a 10-section fixture. Selected detail was about 229 KB. File/diff/daemon usable-shell times were 78/85/83 ms, uncached selection 104/109/110 ms and cached click-to-frame 16.3/16.3/17.2 ms. No model requests occurred. This is Chromium evidence for a long function, not NVDA spoken-source validation.

## Remaining release gates

Manual NVDA + Edge and native zoom testing were explicitly deferred by the user, who also requested default cutover. No spoken-output result has been reported yet. Final code review findings were checked against backend policy identities. The reported feedback-navigation race did not reproduce; its delayed-save regression passes unchanged. Deep uses the backend-required same-policy prerequisite and retains independent policy variants. A separate Generate focus defect was observed, fixed and verified. No fresh live-model teaching-quality evaluation has been performed for this migration. Automated model fixtures cannot establish factual teaching quality.

Full regression, release packaging smoke, long-source measurements and scoped security review are recorded in acceptance-record.md. Native browser zoom, manual NVDA workflows and performance target calibration still require release acceptance. The user accepted the manual-test deferral and requested the new default. Keep the legacy renderer at explicit fallback routes during stabilization.

## Release timing follow-up

A separate release-binary run (`release-event-benchmark.json`) instruments the actual section-link click through the first source-rendered animation frame, including the selected source GET. It measures uncached section 1, then cached section 0; unlike the Playwright wall timings above, it excludes locator search/actionability overhead across thousands of section links. Both measurements are retained to avoid conflating harness latency with UI response. Synthetic DOM click timing does not establish keyboard or NVDA responsiveness.

Release binary SHA256: `2654430f7b1677dbb0e4ff92308763daf4bbbff773a4ca9b4789364c1c461e1b`.

| Mode | Sections | Uncached click-to-source-frame ms | Cached click-to-source-frame ms | Usable shell ms |
| --- | ---: | ---: | ---: | ---: |
| File | 1000 | 24.3 | 8.1 | 52.1 |
| File | 5000 | 66.6 | 33.4 | 93.7 |
| Diff | 1000 | 24.1 | 12.0 | 80.6 |
| Diff | 5000 | 62.0 | 33.6 | 91.8 |
| Daemon | 1000 | 23.1 | 8.1 | 80.5 |
| Daemon | 5000 | 63.5 | 33.5 | 87.3 |

These event-to-frame results meet the initial local-source/cached-switch targets on this hardware. They do not erase the recorded end-to-end automation timings, metadata DOM size or analysis cost. This remains a single-run synthetic fixture measurement; manual screen-reader response is still a separate gate.

## Default cutover follow-up

The user requested the new UI as default with manual testing deferred. The default entry points and retained fallback routes are covered by real-process HTTP tests. File selection now loads first/last-read code with heading focus, a single history entry and zero generation. Existing explicit invalid/stale section errors are unchanged. A full 59-case browser run passed; the refreshed task check passed. Release packaging passed for this final behavior. A clean pinned frontend install reproduced the same built JavaScript.

## Current default release verification

The final default-cutover binary SHA256 is `2be91ec9b4a895cf12e5db9a818f8be72abc2f78b6b3919271ef4923bb1be8e6`. `default-release-benchmark.json` records all 12 combinations of 10/100/1000/5000 sections and file/diff/daemon, opening the default entry points. Legacy payload measurements now explicitly use the fallback route. Initial app navigation made zero source requests, zero policy/result POSTs and zero model calls in every case. Selecting a file subsequently loads its first source automatically.

Cold section click-to-frame was 5.8–53.5 ms; cached switching was 3.1–36.6 ms. Usable shell was 76.6–94.3 ms. The complete first-file gesture, including rendering its section list and first source, was 68.6–862.4 ms. At 5000 sections this combined gesture is dominated by the full metadata list; it is distinct from subsequent source-switch latency. These single-run Chromium fixture measurements do not establish NVDA timing.

The 2500-line/approximately 20 KiB-line long-function follow-up also passed all three modes. Source detail was approximately 229 KB; first-file-to-source was 93.0/111.5/137.7 ms for file/diff/daemon. All runs generated zero explanations. The final bundle is 113.75 KB JS (38.05 KB gzip) plus 17.85 KB CSS (3.50 KB gzip), embedded in Rust.

Current verification artifacts under the ignored evaluation directory: `default-full-browser.log` (59 passed), `default-final-check.log` (task check passed), `default-release-build.log` (release build passed), `default-release-benchmark.json` and `default-long-source.json`. Manual NVDA/Edge and native zoom remain explicitly deferred by the user; legacy fallback is retained during stabilization.
