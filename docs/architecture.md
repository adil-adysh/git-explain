# git-explain architecture

The product has two explicit analysis modes with one shared extraction boundary.

```text
SourceLoader
    ↓
SemanticExtractor
    ↓
SemanticDocument
    ├── file mode ─────────────→ snapshot explanation cache
    └── diff mode
            ↓
        SemanticMatcher
            ↓
        ChangeSet
            ↓
        change explanation cache
```

`SemanticDocument` describes source at one revision without assigning Git change meaning. File mode consumes it directly. Diff mode compares before and after documents and produces the canonical change hierarchy:

```text
ChangeSet
  └── FileChange[]
        ├── deterministic Git metadata
        └── SemanticUnitChange[]
```

File mode must never manufacture `Added` changes for units that merely exist in a snapshot. Diff mode must never ask a model to infer rename, copy, binary, or mode facts.

## CLI selection

Plain `git explain` is shorthand for the current working-tree diff.

```text
git explain diff                         configured working-tree comparison
git explain diff --staged                diff_target → index
git explain diff COMMIT                  first parent → COMMIT
git explain diff A..B                    A → B
git explain diff A...B                   merge-base(A, B) → B
git explain diff RANGE -- PATH...        filter complete Git file changes

git explain file -- PATH...              working-tree documents
git explain file REVISION -- PATH...     documents from REVISION
```

Working-tree discovery optionally adds untracked, non-ignored files when
`git.include_untracked` is enabled. Staged-only and revision/range analysis
never add untracked files.

File selectors accept files and directories. Working-tree directories expand through tracked and untracked, non-ignored files. Revision directories expand with `git ls-tree` from the resolved commit, not from the checkout. Results use stable path ordering.

Diff path filtering occurs after Git discovers the complete change set. A rename therefore matches either its old or new path without degrading rename metadata into a deletion or addition.

## Diff pipeline

`RepositoryAnalyzer` asks `git.rs` for raw, NUL-delimited file metadata and unified patches. Git owns file identity and status. Source providers load the before and after versions:

- working tree: configured base or index → working tree;
- staged: configured base → index;
- commit: first parent or empty tree → commit;
- explicit range: resolved left endpoint or merge base → resolved right endpoint.

Each textual side becomes a `SemanticDocument`. `SemanticMatcher` pairs units and classifies them as `Added`, `Modified`, `Deleted`, or `Preserved`. Renderers and inference consume only the resulting `ChangeSet`; there is no parallel flat unit collection.

Pure renames, exact copies, mode-only changes, binary changes, and preserved units require zero model requests. Added, modified, and deleted semantic units are explainable. Deleted units use before-source; added units use after-source; modified units retain both sides.

## File pipeline

File mode resolves selected paths at the requested snapshot, loads complete source, and produces `FileAnalysis { documents, explanations }`. Every extracted unit may be explained because the user explicitly requested the file snapshot. There is no `SemanticMatcher`, Git status, or fake before/after pair.

The file browser exposes Overview, Concepts, and Deep views on one shared source rendering. It intentionally has no Changes or Git-diff control because a file snapshot has no before/after diff. See `docs/explanation-ux.md` for the interaction contract.

## Cache boundaries

Both modes use the repository-local SQLite store, but their content-addressed keys have mandatory, non-overlapping scopes:

- `snapshot`: whole-file semantic unit at one snapshot;
- `change`: semantic unit change with Git context and diff.

`ExplanationKind` and the applicable normal/deep generation settings are key inputs. Overview, Changes, Concepts, and Deep therefore remain independent, as do model, provider, prompt version, reader settings, explanation settings, source, regions, diff, and prior explanation. A snapshot response can never satisfy a change request, or vice versa.

## Browser and daemon

Diff mode normally registers a `ChangeSet` with the loopback daemon. Sessions have immutable analyzed inputs, a mutable explanation overlay, bounded cache/in-flight state, opaque IDs, cancellation, and atomic refresh. `--direct` uses the one-shot diff server.

File mode uses the loopback snapshot server. Snapshot endpoints use snapshot cache keys and file-unit IDs. Diff endpoints use change cache keys and semantic-change IDs. Both browser experiences generate only from the explicit action and keep every `ExplanationKind` response distinct; selecting a view never starts inference.

The server always binds to `127.0.0.1`. Daemon control routes require the local control token. Repository configuration cannot define model endpoints or credential references.

## Rendering and accessibility

CLI, browser, and JSON render deterministic domain facts; they do not reanalyze source independently. CLI debug output is stable, line-oriented text with explicit status labels, paths, unit names, kinds, ranges, source, semantic counts, and model-request counts. It uses no cursor rewriting, spinner, color-only meaning, or visual-table dependency.

Browser output uses headings, file navigation, textual statuses, status regions, native buttons, and source text controls. Diff pages show file metadata above code-section cards. File pages show document metadata above code-section cards. Each card has one shared source view; Changes and Concepts redraw annotations on that view.

## Invariants

- Git determines file facts.
- `SemanticDocument` contains no change classification.
- `SemanticMatcher` is used only by diff mode.
- `ChangeSet` is the sole diff-domain result.
- File mode never creates fake changes.
- Deterministic metadata never requires inference.
- Snapshot and change cache identities never collide.
- Analysis contains no provider, server, or mutable runtime concerns.
