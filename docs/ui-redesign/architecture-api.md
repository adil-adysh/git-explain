# Frontend architecture and Rust API contracts

> **Status:** Proposed contracts, subject to validation against current Rust types and the authoritative [explanation UX](../explanation-ux.md). **No endpoint described here has been implemented merely by documenting it.**
>
> Companion documents: [screens](screens.md), [interactions](interaction-accessibility.md), [phases](implementation-plan.md).

## Current baseline (reviewed 2026-10-23)

- `src/web.rs` creates full HTML for all supported files and code-section cards, embedding source, views and saved state.
- `src/web.js` initializes state for each article and performs per-section/per-view `learning/resolve` and `learning/results` lookups.
- `src/server.rs` provides separate standalone diff and file routers. Diff `GET /api/changes` returns metadata (`ChangeSet::json`); standalone file mode lacks an equivalent lightweight index endpoint.
- `src/daemon.rs` provides `GET /api/sessions/{session}/changes` and `GET /api/sessions/{session}/snapshot` plus generation and learning endpoints.
- Existing `ChangeSet::json` includes change IDs, identity, language and start/end lines **but not full unit source**; it may not represent the required full unchanged-section index for a selected diff file.
- Existing generation routes are `POST` and include session/learning context. Do not rewrite provider, cache identity or serialization logic just for the frontend.

## Default entry points

The default shell is served at `/` for standalone file/diff and `/sessions/{id}` for daemon sessions. The `/workspace` and `/sessions/{id}/workspace` aliases remain supported. Legacy rendering is available only through explicit `/legacy` and `/sessions/{id}/legacy` fallback routes. The daemon route parser recognizes both default and alias paths before any API request.

Selecting a file chooses its first section or the last-read valid section in that file. This memory is workspace-local and cleared when reloading the index, so stale section IDs are never silently remapped across source identities. One navigation entry contains the file and selected section; initial app startup still fetches metadata only. Empty files have a textual no-sections state and no source request. User-requested cutover proceeds with manual NVDA/zoom validation deferred and recorded in the acceptance ledger.

## Responsibility split

| Rust/Axum owns | Svelte owns |
| --- | --- |
| Git analysis, file/source extraction, snapshots | App shell, destination/section navigation and view state |
| Stable section identities and source/diff metadata | Explorer filtering, displayed counts and selection |
| Authoritative learning-policy resolution and provenance | Form drafts and local override intent |
| Cache identity and verified result restoration | Read-through client caches and result display |
| Model execution, queue/serialization and prerequisite handling | Explicit Generate/Regenerate controls and progress |
| User config persistence, origin/token checks and conflict detection | Save prompts, validation display and conflict recovery |
| Strict source/annotation schema and escaping boundaries | Safe text rendering, syntax styling, accessible interactions |

Do **not** duplicate business logic that decides whether an explanation is valid in Svelte.

## Svelte 5 layout (proposal)

```text
frontend/
  package.json, package-lock.json, vite.config.ts, tsconfig.json
  src/
    main.ts
    App.svelte
    lib/
      api/
        client.ts
        types.ts
        adapt-standalone.ts
        adapt-daemon.ts
      state/
        navigation.svelte.ts
        learning.svelte.ts
        explanations.svelte.ts
        source-cache.ts
      components/
        AppNavigation.svelte
        WorkspaceHeader.svelte
        FileExplorer.svelte
        SectionList.svelte
        SectionReader.svelte
        SectionNavigation.svelte
        SourceViewer.svelte
        DiffViewer.svelte
        ExplanationTabs.svelte
        ExplanationPanel.svelte
        AnnotationList.svelte
        SectionCustomize.svelte
        PersonalizationPage.svelte
        LearningProfileEditor.svelte
        SettingsPage.svelte
        StatusRegion.svelte
        HelpPage.svelte
      styles/
        tokens.css
        global.css
```

Use Svelte 5 runes (`$state` for mutable state, `$derived` for pure computed data, effects sparingly for browser integrations). Prefer shared typed services and isolated components over DOM-wide query selectors. Avoid an additional framework/router/state library until its benefit is measured. All external dependencies must be locked and audited.

Top-level navigation can use a lightweight client-side router; selected routes use standard browser history. No SvelteKit server is required.

## Backend API: versioning strategy

Introduce explicit read-only JSON endpoints for **workspace metadata**, **full file/section index**, and **selected section detail**. Keep established POST learning/generation endpoints initially; add typed adapter modules so standalone and daemon share a frontend view model. Consider `/api/ui/v1/...` namespace to avoid changing existing contracts unintentionally.

Use the following proposed logical operations and map them to mode-specific paths:

| Operation | Standalone example | Daemon example | Side effects |
| --- | --- | --- | --- |
| Workspace summary | `GET /api/ui/v1/workspace` | `GET /api/sessions/{id}/ui/v1/workspace` | None |
| File/section metadata | `GET /api/ui/v1/index` | `GET /api/sessions/{id}/ui/v1/index` | None |
| Selected section source/diff | `GET /api/ui/v1/sections/{opaque_id}` | `GET /api/sessions/{id}/ui/v1/sections/{opaque_id}` | None |
| Resolve effective preferences | Existing `POST /api/learning/resolve` | Existing session learning resolver | No inference |
| Restore one saved explanation | Existing `POST /api/learning/results` | Existing session learning results | Cache only |
| Generate/Regenerate | Existing `POST /api/(file-)units/{id}/...` | Existing session generation routes | Explicit inference |
| Save learning profiles | Existing authenticated `PUT /api/learning` | Existing authenticated session PUT | Writes user config |

The paths are **proposals**; implement routes that match modes cleanly and document final behavior in the code. Do not introduce route duplication without adapters/fixtures.

### Metadata payload proposal

```json
{
  "schema_version": 1,
  "mode": "diff",
  "session_id": "opaque-session-or-null",
  "generation": 17,
  "snapshot_id": "opaque-snapshot",
  "revision_label": "working-tree-vs-HEAD",
  "capabilities": {
    "changes_view": true,
    "learning_profiles": true,
    "section_override": true
  },
  "files": [
    {
      "id": "opaque-file-id",
      "path_label": "src/server.rs",
      "old_path_label": null,
      "status": "modified",
      "language": "rust",
      "section_count": 2,
      "sections": [
        {
          "id": "opaque-section-id",
          "name": "generate_file",
          "kind": "function",
          "start_line": 291,
          "end_line": 299,
          "source_side": "after",
          "change_status": "modified",
          "capabilities": ["read_source", "overview", "changes", "concepts", "deep"]
        },
        {
          "id": "opaque-other-id",
          "name": "learning_results",
          "kind": "function",
          "start_line": 518,
          "end_line": 541,
          "source_side": "after",
          "change_status": "unchanged",
          "capabilities": ["read_source"]
        }
      ]
    }
  ]
}
```

Notes:
- Names, sample line numbers, opaque strings and available capabilities above are **illustrative only**, not a serialization guarantee.
- **All discovered sections** includes unchanged ones when the source analysis can represent them. This may require a new analyzer index beyond `ChangeSet::explainable_changes()`. Do not lie about completeness by returning only changed units.
- For deleted-only/renamed/copied units, carry before/after path, source side and location explicitly. Do not merge old vs new source identity or show unsupported explanation actions.
- Support large indexes without embedding source or explanation. If metadata alone proves too large, add incremental pagination/chunking with an explicit total and complete-list affordance; never silently truncate.
- `snapshot_id` and `generation` are distinct: generation can advance without a user intentionally selecting the new state. Endpoints must validate snapshot consistency.
- Do not expose secrets in status payloads. Filesystem path display is limited to already authorized local repo context.

### Selected section detail proposal

```json
{
  "schema_version": 1,
  "snapshot_id": "opaque-snapshot",
  "generation": 17,
  "file_id": "opaque-file-id",
  "section_id": "opaque-section-id",
  "language": "rust",
  "source_side": "after",
  "source": "fn generate_file(...) { ... }",
  "old_source": null,
  "diff": null,
  "line_range": {"start": 291, "end": 299},
  "valid_annotation_regions": [],
  "capabilities": ["read_source", "overview", "changes", "concepts", "deep"]
}
```

- A detail fetch is strictly **non-generative**. Return immutable raw source and validated diff regions, not trusted HTML.
- Where meaningful, old/new versions may be separate fields; never collapse deleted/renamed state into an unlabeled source blob.
- Return an explicit unsupported/empty detail state, not fabricated source.
- Validate `section_id` against the current session and snapshot. Client must not construct arbitrary filesystem read paths.
- Use consistent HTTP errors: 404 unknown section, 409 stale snapshot/generation, 410 expired session where appropriate, 422 unsupported operation, and 5xx internal error. Standardize JSON error body with stable code, human-readable message, retryable flag and current snapshot hint where safe.

### Interaction with current explanation responses

Retain the authoritative backend fields for requested kind, policy ID, result ID, actual policy, cached/restored marker, annotations and Deep prerequisites. Build a TypeScript adapter that never treats frontend-selected settings as proof of the persisted/generated policy. Display “Pending settings” and “Used for displayed result” separately.

**Deep prerequisite:** file mode uses **Overview**, diff mode uses **Changes**. If explicit Deep generation internally needs a missing prerequisite, make this visible as one user-authorized work sequence; never request Deep just because the tab was opened. Regenerating the prerequisite invalidates only appropriately dependent Deep variants.

## Learning override contract

Existing preference hierarchy documented in `docs/personalized-learning-design.md` includes application → user defaults → opted-in repository → selected profile → language override → session → per-section/per-view temporary override.

Extend it with a true **whole-section scope**, *without collapsing existing view scope*:

```text
effective = application
          ⊕ user defaults
          ⊕ opted-in repository defaults
          ⊕ selected learning profile
          ⊕ language preferences
          ⊕ session overrides
          ⊕ selected-section overrides        (NEW)
          ⊕ selected-view overrides           (EXISTING)
```

`⊕` means field-wise overwrite **only for explicitly set fields**. For arrays, preserve omitted vs explicitly empty behavior. Show effective values and provenance. Section-specific overrides should be keyed to `(session, snapshot, section_id)`; view overrides additionally to `kind`.

**Persistence:** local section and view overrides are temporary by default and never write config TOML automatically. Separate explicit actions save reusable preferences or concept feedback to a named learning profile. A screen crossing or reload must not silently promote a temporary preference to global scope.

**Compatibility:** current `LearningSelection` and policy serialization need audit to determine if section + view field-wise merge can be done client-side by constructing one resolved per-request selection or needs new typed Rust fields. Choose a single authoritative Rust resolution path. Add negative tests for leakage across units, policies and snapshots.

**Critical precondition from prior audit:** fix potential comment loss from TOML reserialization and the ordinary config writer vs learning writer shared-lock race, and add concurrent write tests before exposing expanded browser profile-save flows.

## Client cache and concurrency

Minimum identity for source detail: `(session or standalone context, snapshot_id, file_id, section_id, source_side)`.

Explanation cache identity: use backend's complete identity, including section/source, kind, effective policy, model/generation settings, response/prompt contract version and Deep prerequisite result ID. Frontend cache **cannot redefine** that identity; use backend returned keys and bounded lookup/restore. Never persist source content or secrets to `localStorage` by default.

- On section selection, abort **obsolete navigational GET requests** using `AbortController`; optionally keep fulfilled data in bounded cache.
- A background generation POST, once explicitly submitted, must not be aborted merely because the user navigates. Maintain request/job association to its target.
- Responses require request ID and snapshot comparison before updating selected UI.
- No eager `all sections × all views` learning resolves/results. Resolve only active section/current view and memoize repeated policy resolution where semantics permit.
- Rapid file switching, filters, session refresh and overlapping queued generation must not produce mixed state.
- Source/diff never appear as raw unescaped `innerHTML`; model explanation/feedback text is also untrusted.

## Static asset and build strategy

- Svelte/Vite build is a **development/build dependency**, not a runtime network dependency. Rust serves the locally built files.
- Production install must include fingerprinted JS/CSS assets. Define deterministic asset bundling (build.rs embedding or packaged asset directory), avoid “binary works only when frontend/node_modules exists”.
- Support SPA fallback for actual app routes **without** hijacking `/api` error responses.
- Keep loopback binding by default, same-origin protections, write capability tokens; review CSP and local resource access, including model-provided URLs.
- CI adds Node/npm reproducible install, TypeScript/Svelte checks, production Vite build, Rust binary build, interaction/browser tests, and a packaged-build smoke test.
- Commit `package-lock.json`; verify final `cargo build --release` and the documented install path use the correct prebuilt assets.

## Visual performance limits

- Small shell + metadata-only initial response; no full source or bulk explanation payload.
- A selected section may fetch source and one result policy/lookup, but it must not fan out to all views.
- Prefetch adjacent **source only**, after current section and under bounded CPU/memory/network budget.
- For long files, source rendering must remain accessible; progressive source chunks are **deferred** until measured and independently designed.
- Capture shell load, index parse/paint, source fetch/paint, cached-result lookup, and generation wait **separately**.

See [test plan](test-plan.md) for budgets and verification. Backend analysis may still be expensive; fast frontend paint is not a claim that Git analysis finished.

## References and detailed code to inspect during implementation

- `src/change_set.rs`: `ChangeSet::json`, `SemanticUnitChange`, before/after projections.
- `src/file_analysis.rs`: `FileAnalysis`, semantic document units and unit keys.
- `src/server.rs`: standalone file/diff routes and learning restore/generation.
- `src/daemon.rs`: session snapshots, active session and stale-git handling.
- `src/learning.rs`, `src/learning_runtime.rs`: selection shape, provenance, policy and settings token.
- `src/runtime.rs` and explanation cache modules: source selection and immutable identities.
- [Svelte runes](https://svelte.dev/docs/svelte/what-are-runes), [Svelte effects guidance](https://svelte.dev/docs/svelte/%24effect).
