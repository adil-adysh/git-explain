# git-explain architecture

This document describes the current implementation after the daemon milestones. Repository watching and automatic refresh remain intentionally deferred; multi-repository sessions are supported through the bounded daemon registry described below.

## Current entrypoint

`src/main.rs` is orchestration only:

```text
CLI/config/cache/debug handling ── direct path
        ↓ normal web mode
daemon discovery/start → authenticated repository open
        ↓
single RepositorySession
        ↓
RepositoryAnalyzer → AnalysisSnapshot
        ↓
session-aware Axum web routes
```

The existing commands remain one-shot:

- `git explain` discovers/starts the daemon, then registers the current repository working tree.
- `git explain REVISION` registers the current repository and asks the daemon to analyze the revision against its first parent, or the empty tree for a root commit.
- `--debug` prints the already-created snapshot and exits without starting the web server or calling a model.
- `config` and `cache` commands return before repository analysis as before.
- `git explain --direct` is an explicit fallback to the original one-shot server path.

## Snapshot domain

The current types are in `src/snapshot.rs`:

| Type | Meaning | Mutability/owner |
| --- | --- | --- |
| `SnapshotGeneration(u64)` | Typed version of the analyzed state. One-shot sessions start at `1`; changed active sessions increment it during atomic refresh. | Immutable value owned by the snapshot/server request boundary. |
| `SnapshotIdentity` | `WorkingTree { fingerprint }` or `Commit { oid }`. | Immutable snapshot identity. |
| `UnitId(String)` | SHA-256-derived opaque identity for one analyzed unit. | Immutable ID attached to `ExplainedUnit`. |
| `AnalysisSnapshot` | One coherent repository state: generation, identity, `AnalysisContext`, retained `FileChange`s, and ordered units. | Created by `RepositoryAnalyzer`; passed by value to the server. |

`AnalysisSnapshot` does not contain a model provider, cache, Axum state, watcher, or mutable explanation runtime. The model is never called while a snapshot is being created.

### Snapshot identity

Working-tree identity is:

```text
SHA-256(
  HEAD OID + configured diff_target +
  every parsed FileChange's path, kind, old path, ranges, and unified diff
)
```

This is a deterministic identity of the authoritative analysis inputs, not a claim that every filesystem byte was independently indexed. Commit identity is the resolved full commit OID, so aliases such as `HEAD`, a short SHA, and the full SHA converge on the same identity.

### Unit identity

`UnitId` hashes length-delimited values for the file path, source-unit kind, qualified name (or name), source range, and source text. Therefore the same unit in the same snapshot receives the same ID; changing source, path, kind, name, or range changes the identity. It is deliberately not semantic rename tracking and does not replace the content-addressed explanation-cache key.

Display order remains the `Vec<ExplainedUnit>` order produced by analysis. Runtime lookup is a `HashMap<UnitId, ExplainedUnit>`, so browser requests do not depend on vector position.

## RepositoryAnalyzer

`src/analyzer.rs` owns the deterministic pipeline:

1. Select Git inputs using the resolved `GitConfig`.
2. Build `FileChange`s.
3. Select `WorkingTreeSourceProvider` or `GitCommitSourceProvider`.
4. Build the historical/working-tree `AnalysisContext`.
5. Call `analysis_items` to discover source units, scoped diffs, and regions.
6. Assign stable `UnitId`s.
7. Return an `AnalysisSnapshot` with explicit identity and generation.

It does not start Axum, open a browser, call the LLM, manage cache entries, watch files, or run a daemon. `include_untracked` remains parsed/documented but unsupported in the current Git diff path. Staged behavior and `diff_target` are unchanged.

Commit analysis retains root-commit empty-tree behavior, first-parent merge behavior, commit subject, deleted-file context, and historical source reads.

## Server and request safety

The direct compatibility server in `src/server.rs` receives one snapshot. The daemon runtime in `src/daemon.rs` uses the same snapshot model inside a single `RepositorySession`. Its global state contains:

```text
immutable AnalysisSnapshot
Mutex<HashMap<UnitId, ExplainedUnit>>  // generated explanation overlay
provider, cache, and resolved model/readers/explanation config
```

The snapshot is the immutable source/diff/context boundary. Explanation fields are the mutable presentation overlay. Cache hydration and model completion update the overlay by `UnitId`.

Routes retain their shape but use opaque IDs:

```text
POST /api/sessions/{session-id}/units/{unit-id}/explain
POST /api/sessions/{session-id}/units/{unit-id}/deep
POST /api/sessions/{session-id}/units/{unit-id}/regenerate
POST /api/sessions/{session-id}/units/{unit-id}/deep/regenerate
```

The browser renders `data-unit-id` and `data-generation="1"`, and sends `{ "generation": 1 }` with each explanation request. The server rejects a request whose generation differs from its snapshot before looking up the unit or calling the model:

```json
{"ok":false,"stale":true,"error":"Repository snapshot has changed."}
```

Inference does not hold the item mutex. It copies the unit/request, awaits the provider, then performs a short write-back guarded by session identity, generation, and `UnitId`. If the session has been replaced, the result is discarded and cannot update the replacement repository.

The cache remains content-addressed by `ExplanationRequest` plus model/reader/explanation configuration. `UnitId` identifies a unit in a snapshot; it is not used as a cache-key replacement.

## CLI and web lifecycles

Normal working-tree flow:

```text
parse CLI → resolve repo → discover/start daemon → authenticated open request
→ daemon resolves config → analyzer working-tree snapshot → session URL
```

Commit:

```text
parse revision → discover/start daemon → authenticated open request
→ analyzer resolves full OID/selects first parent or empty tree → session URL
```

Web:

```text
GET /sessions/{session-id} → ordered units + source + existing explanation overlay
POST session explain/deep → validate session/generation → ID lookup → cache or model
→ short guarded write-back → JSON result

Daemon lifecycle:

```text
daemon start → spawn `git explain daemon run` → health probe → daemon.json
daemon status → metadata + loopback health probe
daemon stop → token-authenticated shutdown request
```

The daemon binds only to `127.0.0.1` on a dedicated stable port, `8192` by default. `GIT_EXPLAIN_DAEMON_PORT` may select another port. User-scoped metadata is stored beside the user config as `daemon.json`; it contains PID, port, start time, protocol version, and a random control token, but no source, explanations, API keys, or repository credentials. A create-new lock file prevents duplicate startup races and stale metadata is discarded after a failed health probe.

Control routes are separate from browser routes. `POST /api/repos/open` and `POST /api/daemon/shutdown` require the token in `x-git-explain-control-token`. Browser HTML never receives that token and can address only session/unit IDs. Loopback is treated as privileged local access, not as authentication; the token prevents a local webpage from registering arbitrary paths or stopping the daemon.
```

Normal/deep explanation behavior, retry policy, llama.cpp/OpenAI-compatible configuration, accessibility markup (`aria-live`, semantic headings, hide/show), and cache behavior remain outside deterministic analysis and are unchanged.

## Tests and invariants

The implementation tests stable unit IDs and working-tree fingerprint determinism. Existing fixture tests cover working-tree discovery, historical commit source correctness, root and merge commits, supported languages, cache, model HTTP behavior, and rendered accessibility behavior.

The key invariant is:

> Every browser/model request operates against an explicit immutable repository snapshot, identified by generation, and every changed code unit has a stable identity within that snapshot.

No model request is made by `RepositoryAnalyzer`.

## Migration plan and remaining daemon work

The daemon now maintains a bounded multi-repository session registry. Each session has an opaque ID, an immutable current snapshot, repository-scoped cache and in-flight inference state, and a cancellation signal. Opening a repository makes it active without invalidating other sessions; the registry evicts the least recently used session at its capacity limit and cancels its work. Refresh still targets only the active session, reuses `RepositoryAnalyzer`, compares snapshot identity, and replaces the snapshot atomically. Existing direct CLI/server mode remains available through `git explain --direct`.

Inference requests in the daemon are limited by a small process-wide semaphore. Each repository session deduplicates identical in-flight requests using the complete repository-scoped cache key. Replacing a session marks its cancellation state before the swap; waiting, queued, and active model requests then stop without writing into the replacement session.
