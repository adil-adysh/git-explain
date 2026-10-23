# Screen-by-screen UI/UX specification

> **Status:** Active default UI (`/` standalone, `/sessions/{id}` daemon; `/workspace` aliases). Legacy fallback retained at `/legacy` and `/sessions/{id}/legacy` until stabilization. Manual NVDA and native zoom checks are explicitly deferred, not passed. See [overview](README.md) and [interaction contract](interaction-accessibility.md).
>
> All example names and responses are illustrative; actual displayed values must come from the current repository snapshot.

## Screen inventory

| Screen / surface | Entry | Primary job | Exit / next action |
| --- | --- | --- | --- |
| 0. Workspace startup | Browser opens default standalone (`/`) or daemon (`/sessions/{id}`) URL | Establish repository, revision and usable navigation | Explore or an actionable recovery |
| 1. Explore / file list | Top-level Explore link | Choose file in repository context | Display chosen file's complete code-section index |
| 2. Explore / selected file | Choose file | Scan/filter **all discovered code sections** | Select one code section |
| 3. Reader / selected section | Choose section | Read selected source and diff | Choose explanation kind or another section |
| 4. Reader / explanation | Select Overview/Changes/Concepts/Deep | Read saved output or explicitly generate | Inspect annotation, change view, adjust preferences |
| 5. Reader / Customize | Activate Customize this section | Temporary per-section or per-view overrides | Apply, reset or cancel without inference |
| 6. Personalization | Top-level Personalization link | Choose reusable profile and session preferences | Explore, or profile editor |
| 7. Personalization / editor | Open defaults/profile | Create/edit/save profiles, languages, concepts | Save or discard draft |
| 8. Settings | Top-level Settings link | Control appearance, source presentation, view runtime information | Return to previous work |
| 9. Help / shortcuts | Help action or keyboard help link | Learn controls and status terminology | Return focus to opener |
| 10. Recovery states | Connection/snapshot/generation/save failures | Understand failure and safely retry | Resume prior task |

Top-level navigation uses **links**, not an ARIA tab widget. Only explanation-kind selectors use ARIA tabs.

## 0. Workspace startup

**Purpose:** Show the interface promptly, even if Git analysis has not completed. Default URLs are `/` for standalone and `/sessions/{id}` for daemon (`/workspace` and `/sessions/{id}/workspace` remain supported aliases; `/legacy` and `/sessions/{id}/legacy` provide fallback to the legacy UI).

**Structure:** Application header (name, selected repo/session, revision), top-level Explore / Personalization / Settings links; primary loading message; optional recent context for daemon; explicit help.

**Behavior:**
- Ship a small bundled HTML/JS/CSS shell from Axum; no external CDN, Node runtime or model endpoint required to display it.
- Request lightweight workspace metadata and the initial file index. Initial app loads only metadata, not source. Do not restore results or resolve policies for every section.
- When there is no valid session, show a named empty state and a path back to the daemon-supported open/reopen workflow; do not invent an in-browser repository chooser for standalone CLI mode.
- Show revision, working tree or comparison identifier only after it is known; do not substitute `main` or guess a commit.
- If repository analysis takes time, show a polite status, not an indefinitely spinning empty page. Error provides Retry and diagnostics.
- No auto-inference.
- **Focus:** on normal page load, browser focus remains natural; provide skip-to-main and a heading for the workspace. Do not steal focus asynchronously for background index completion.

**Acceptance:** small shell renders before large source/index work; no source bodies or cached results embedded in first HTML.

## 1. Explore — file list

**Purpose:** Pick a file and understand what is available.

**Structure (desktop):**

```text
[Git Explain: repo / revision]           Explore | Personalization | Settings
[Skip to main]
Explorer (navigation landmark)                Reader (main landmark)
  Files (N)
  [Search files/sections...]
  - src/server.rs — Modified
  - src/learning.rs — Modified
  - src/web.rs — Unchanged
  ...
  [Code sections for selected file]          [Code section source / diff]
```

**Behavior:**
- File list includes appropriate supported files for the mode and indicates added/modified/deleted/renamed/copied/unchanged as actual analysis permits. Unsupported or binary files get textual reason or are clearly excluded with counts; never promise a section list for unsupported content.
- Selecting a file automatically selects its first code section (or restores the last-read section for that file within the current immutable workspace); empty files have no source; invalid or stale explicit section IDs remain errors. Fetches only that selected section's source; no model generation on file selection or section restoration. Adds one history entry including the selected section.
- The Reader displays the selected code section's source; never display source from a different file under the new file heading. Empty files display an explicit empty state with no source.
- In daemon mode a refresh may update paths/status; preserve selection by stable identity when possible, otherwise select no section and explain.
- Files may be searched and filtered; show result count and Clear filter; no hidden navigation based solely on color.
- **Focus:** activating a file moves focus to the Reader section heading once source is ready. Manual NVDA verification is explicitly deferred.
- **Empty:** “No supported files”, “No matches”, “Not analyzed yet”, and “Failed to load files” are separate states.

**Accessible labels:** “src/server.rs, modified, 12 code sections” rather than icons only.

## 2. Explore — code sections of selected file

**Purpose:** View the file's structure before reading source.

**Structure:**

```text
Code sections in src/server.rs (12)
[Filter sections] [All | Changed | Unchanged]   (ordinary buttons/select)
1. index — function — lines 542–577 — modified
2. learning_results — function — lines 518–541 — unchanged
3. generate_file — function — lines 291–299 — modified
...
```

**Behavior:**
- **Show all code sections discovered for that file as metadata**, including unchanged sections when valid data is available. Never silently equate “explainable changes only” with “all sections”.
- Default order is ascending active-side source line, with a stable tie-breaker. A deleted-only unit uses the before-side source location, visibly labeled “deleted / old revision”. Identity used for navigation is backend-issued and snapshot scoped.
- Optionally group methods by parent symbol; default must remain a simple ordered list, navigable with native list/heading semantics.
- Selecting a section requests its source/diff details, updates URL and Reader; no inference. A deleted section shows old source with a clear label; an unchanged section in diff mode may be read-only for change explanations until explicitly supported by backend.
- If the semantic analyzer does not have a complete file-section index in the current diff snapshot, backend expansion is **required** before claiming UX-01 done.
- Search matches name, path and type, without fetching bodies. Filtering updates counts and the Previous/Next sequence.
- If selected section is removed by a filter, **keep its Reader visible** with “outside current filter” indication; Previous/Next is unavailable until the user chooses a visible result or clears filter. Never jump to a different section silently.
- **Focus:** section selection moves focus to Reader section heading once data is ready. A failure focuses a named error heading only on direct activation; Retry stays accessible.
- **Empty:** “No code sections”, “No sections match”, and “Unsupported language” are distinct states.

## 3. Reader — source and section navigation

**Purpose:** Study one section at a time while retaining structural context.

**Content order:** File breadcrumb/identity → section heading → kind/change/line range/position → Previous/Next section actions → Source → optional Git diff → explanation tabs and selected panel → contextual Customize.

**Source behavior:**
- Fetch selected section's source on demand; reuse immutable snapshot-keyed memory cache; optionally bounded idle prefetch of adjacent source only.
- Provide `pre > code` presentation with escaped text and optional non-disruptive syntax highlighting; a **Read source as text** mode supports copy/selection and reliable NVDA reading.
- Preserve indentation, newlines and full source; use overflow scrolling or line wrap preference; horizontal code scroll must not trap keyboard navigation.
- For changes, show old/new context and deterministic diff region accurately. Do not fabricate a diff for file mode.
- For a renamed/copied/deleted file, clearly show path and version. For file mode, Changes explanation tab does not exist.
- Metadata includes section name, file, language, line range, status, current mode, snapshot state, and “Section X of Y” relative to visible sections.
- Previous/Next only traverses visible sections **within the currently selected file**; disabled at boundaries. The destination is discoverable (“Next section: generate_file”); never switch file automatically.
- Navigating never invokes a model or discards a completed inference.

**Loading/error:** Skeleton/label “Loading source for [section]”; switch only after identity checked. Late responses for a prior selection are cached under original key but must not replace current Reader. Retry must not re-run model generation.

## 4. Reader — explanation views

**Purpose:** Pick the *kind* of help needed for the selected section.

**Views and scope:**
- **Overview:** explain complete section purpose concisely.
- **Changes:** explain actual Git changes; **diff mode only and only for an explainable change**.
- **Concepts:** targeted knowledge and annotations; valid empty result can state “No additional concepts needed”.
- **Deep:** detailed, source-grounded teaching. Prerequisite is actual Overview in file mode, actual Changes in diff mode.

**Component:** genuine ARIA tablist with Overview, Changes where eligible, Concepts, Deep. **No Previous/Next explanation buttons.** Manual activation: Left/Right, Home/End move focus only; Enter/Space activate. Selecting or activating a tab is strictly client-side navigation: it performs **no network request** and **no model inference**, switching views using local client state. Only explicit user activation of Generate or Regenerate initiates inference.

**Panel status machine:** not generated → checking saved result → ready/cached OR missing → user Generate → queued/generating → ready OR error; separate stale and policy-mismatch states. “Generated under different settings” preserves old explanation with its true policy instead of relabeling.

**Actions:** Generate when no valid result, Regenerate when an identical active-policy variant exists; clearly show when saved explanation belongs to other policy. Generation cost/inference is never implicit. A disabled Generate must explain whether policy is loading, prerequisite missing, inference already running, or stale source.

**Annotations:** Source/code and Concepts/Changes annotations must refer only to valid deterministic regions. Inline actions “I know this already” and “Explain this term” update pending learning preferences, not persistent profile, until explicit Save feedback.

**Focus/status:** activating tab retains focus; active panel exposes heading and can be reached by Tab/heading commands. Generate retains focus and uses one concise status announcement for queued/completed/failed. Provide a “Go to explanation” control; no automatic focus jump on completion.

**Generation races:** multiple requests may queue on a local GPU; moving to another section while one runs does not move or relabel the eventual result. Deep depends on exact matching prerequisite result identity.

## 5. Reader — Customize this section (contextual disclosure)

**Purpose:** Change the explanation for one function without navigating to Personalization.

**Visible closed state:**
```text
Learning: Rust learning profile
Effective: brief syntax • reasoning focus • detailed
[Customize this section]   (disclosure)
Displayed explanation: generated with [actual settings] / Pending: [new settings]
```

**On expansion:**
- First select scope: **This section (all explanation views)** or **This explanation view only**.
- Edit **language help** (automatic/explain/brief/skip), **focus** (balanced/syntax/execution/reasoning/architecture), **detail** (concise/balanced/detailed), and supported concepts lists if appropriate.
- Every field offers **Inherit** distinctly from explicit defaults; empty vs omitted concept lists retain existing inheritance semantics.
- Show exact target scope and what will be affected.
- Actions: **Apply temporary override**, **Reset this scope**, **Cancel**. Reset section scope does not delete a more-specific view override; reset view only deletes that view override. Consider “Clear all overrides for this section” separately with explicit label.
- A section override applies to all its views unless shadowed by a view-specific field. Existing per-section-per-view overrides migrate; section-level precedence/serialization is **new backend/interface work**.
- Existing result stays intact until the user explicitly generates; show “Pending settings differ from displayed explanation” with actual and effective policy.
- Changes survive navigation within session, keyed by session + snapshot + section (and view when relevant); not automatically written to TOML. Expired snapshot clears or disables incompatible overrides with explanation.
- When the editor has unsaved form changes, Cancel restores applied values. Avoid silent destruction of the draft on navigation.
- **Focus:** opening disclosure does not steal focus; keyboard follows native form semantics. Applying returns focus to Customize control with concise confirmation.
- **Security:** untrusted code/concept names never gain HTML, URL or script privileges.

## 6. Personalization — active profile and session choices

**Purpose:** Set reusable learning behavior in one predictable destination.

**Content:** Heading “Personalization”; active learning profile; repository defaults opt-in (explicit and labeled); temporary session preferences; current effective/provenance inspector; profile/defaults management entry; known/learning concepts and feedback. Explain distinction between **learning profiles** and **model profiles**.

**Behavior:** Selection and session changes can update effective policy previews, but do not infer or mutate already generated explanations. Returning to Explore restores file, section and view (and scroll position when sensible). Session reset must show exact scope: whether it clears session controls, per-section/view overrides, or both; prefer separate actions rather than a surprising global reset. Changing default saved profile requires explicit Save.

**Loading/conflict:** GET may fail; explain inability to edit but keep Explore available. Saving a stale revision shows conflict with Reload/Review choices, **preserving unsaved draft**.

## 7. Personalization — profile/default editor

**Purpose:** Create or edit persisted user defaults and named learning profiles.

**Controls:** target (user defaults/existing profile/new profile); language override (all or supported language ID); language help/focus/detail; known_languages/learning_languages; known_concepts/learning_concepts; inherited-vs-explicit list options; name field for Save as; visible provenance and validation.

**Buttons:** Save, Save as new profile, Make default, Cancel/Discard; Remove profile only if supported with safeguards against deleting active default. Require confirmation for destructive changes. Inline validation names exact offending field and preserves typed data.

**Persistence:** write only to user-level `[learning]`; repository defaults remain opt-in and read-only from browser. HTTP token/same-origin rules and revision conflict protection are mandatory. Handle concurrent ordinary config writes correctly before enabling browser editing.

**Unsaved changes:** navigate away → retain draft or present a clear confirmation before discard. Save success updates shared profile selection; never triggers inference.

## 8. Settings

**Purpose:** Configure interface behavior and see application/runtime settings without conflating them with learning.

**Sections:**
1. **Appearance:** system/light/dark, high contrast/forced colors, font size, code line wrap. Respect system preferences and browser zoom.
2. **Accessibility and reading:** plain-text source mode, any user-facing keyboard help, status behavior with sensible defaults; avoid a separate inaccessible “screen-reader mode”.
3. **Model / runtime:** show selected model profile, provider/endpoint name, connection/availability only if backend supports it. CLI/config instructions for changes in first release. **Do not present editable model credentials or model switching in UI until designed and secured.**
4. **Diagnostics:** version, session/revision, retry/reload, relevant safe errors, optional cache metadata.

**Behavior:** Appearance changes update presentation only. No inference. Any persistent UI appearance preference requires explicit storage/privacy decision; do not reuse `[learning]` for it. Runtime edit controls should be absent rather than broken placeholders.

## 9. Help and keyboard guide

**Purpose:** Make the mental model discoverable and reduce memory load.

**Content:** “Choose file → section → explanation”; navigation hierarchy; no-inference rule; Generate vs Regenerate vs Refresh definitions; previous/next section scope; explanation-tab keys; how to find settings/customization; feedback and caching; keyboard commands actually implemented.

**Dialog behavior (if modal):** native `dialog` or audited accessible dialog, labelled title, initial focus on heading/close, Escape to close, focus restored to opener. A nonmodal full help page with ordinary links is also acceptable.

## 10. Shared recovery and special states

| Condition | User-visible result | Recovery |
| --- | --- | --- |
| No supported files / empty file | Explanatory empty state with counts | Choose another file or context |
| Index/source request slow | Bounded “Loading…” state associated with actual request | Continue navigating; Retry on error |
| Source unavailable / deleted | Identify which revision/version is missing, or show before-side source if valid | Reselect/reload |
| Unsupported/binary file | Explicit non-support reason | Choose supported file |
| Empty Concepts output | Valid explanation outcome, not a network error | Change view or Regenerate |
| Model unavailable | Generation-only failure; existing source still readable | Check endpoint/profile, Retry |
| Stale Git generation | Explain snapshot mismatch; never attach old results to new section | Reload snapshot; restore selection if safe |
| Inference queued | “Waiting for model” and associated target | Keep reading; no duplicate send |
| HTTP/network error | Operation-specific explanation, no false success message | Retry operation |
| Learning save HTTP 409 | Warn about concurrent config change and preserve editor draft | Reload/compare/merge then explicit Save |
| Session missing/expired | Stable stale-session message | Reopen through supported CLI/daemon flow |
| No section matches filter | Count zero and Clear filter | Clear query/filter |

## Responsive presentation

Desktop: Explorer left, selected Reader right. Logical DOM order starts with global header and Explorer navigation, then main Reader. At narrow widths use a single-column sequence with explicit “Files and sections” and “Reader” navigational links; do **not** make hover, dragging or a permanently open side panel the only route. Optional visual split/resizable panes must preserve text reflow and reading order.

## Explicitly deferred

Manual NVDA verification and native zoom checks are explicitly deferred (scheduled for later, not passed). Continuous multi-section reader; cross-file Next/Previous; automatic scrolling highlight following source annotations; custom shortcuts; per-explanation model selection; in-browser credential editing; full IDE/editor. Legacy code and safeguards are retained at `/legacy` and `/sessions/{id}/legacy` until stabilization.
