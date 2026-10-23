# Repository Guidelines

## Project Structure

`src/` contains the Rust application. CLI and configuration code live in `cli.rs` and `config.rs`; Git analysis is in `git.rs`, `diff.rs`, `analyzer.rs`, and `explain.rs`; `semantic.rs` produces revision-specific `SemanticDocument`s; `file_analysis.rs` owns file-mode snapshot state; `change_set.rs` owns diff-mode state. Supported language parsers are under `src/language/`; daemon, HTTP, and browser behavior is implemented in `daemon.rs`, `server.rs`, and `web.rs`. Shared identity and runtime logic is in `snapshot.rs` and `runtime.rs`.

## Build, Test, and Development Commands

Use Rust stable with `rustup`.

```text
task check              # fmt check, all tests, and git diff whitespace check
task build              # debug binary
task build-release      # optimized release binary
task install            # build and install release binary
task install-dev        # build and install debug binary
task install FORCE=true # stop/replace a busy installed daemon forcefully
```

Equivalent direct commands are `cargo fmt -- --check`, `cargo test`, and `cargo build`. Run `git explain --debug diff` or `git explain --debug file -- <path>` for deterministic analysis without opening the browser. Diff mode accepts a commit, `A..B`, or `A...B`; file-mode selectors accept files or directories. The normal diff daemon uses loopback port 8192; `git explain --direct diff` is the one-shot diff fallback.

## Coding Style and Naming

Format Rust with `cargo fmt`; CI treats formatting failures as errors. Use idiomatic Rust 2021, four-space indentation, `snake_case` for functions/modules, `PascalCase` for types, and descriptive names for snapshot, unit, and repository boundaries. Keep analysis deterministic and avoid putting model, web-server, cache, or mutable runtime concerns into `RepositoryAnalyzer`. `SemanticDocument` has no change classification; only diff mode invokes `SemanticMatcher`. Snapshot and change cache identities must remain disjoint.

## Explanation UX Contract

Treat [docs/explanation-ux.md](docs/explanation-ux.md) as the product contract when changing model requests, explanation state, HTTP routes, cache identity, or browser rendering.

- Optimize for a local GPU with about 8 GiB VRAM. Keep inference serialized unless a measured design explicitly replaces that constraint.
- Generate explanations only after the user presses the visible Generate/Regenerate action. Selecting a tab is navigation and must never start inference.
- Keep each available view—Overview, Changes, Concepts, and Deep—as a distinct `ExplanationKind` request with independent state and cache keys. File mode intentionally omits Changes. Regenerating one must not invalidate unrelated kinds; replacing a prerequisite may invalidate Deep when required.
- Render one shared source view per code section. Changes and Concepts may replace annotations on that view; do not create parallel source copies, injected concept panels, or mode-specific executable scripts.
- In diff mode, Changes and Concepts use deterministic changed regions while retaining the complete active code section as model context. In file mode, Overview and Concepts use the complete deterministic code section. The model may reference only regions supplied by the application.
- Keep unsupported files out of the browser experience. Use Git and code terminology in visible copy; do not expose internal terms such as semantic unit, snapshot, inference, provider, or cache.
- Preserve loopback-only transport and existing control-token requirements.

## Testing Guidelines

Tests use Rust’s built-in test framework plus fixture-based integration tests. Add focused regression coverage in `tests/` for parser, Git-history, daemon lifecycle, HTTP/model, snapshot, or rendered-UI changes as appropriate. Run `task check` before submitting; CI runs formatting, `cargo test`, and a debug build.

For explanation UX changes, verify request kind and deterministic regions, cache-key independence, one source view per rendered code section, server-rendered controls/state, absence of dynamically injected panels, and that tab selection performs no network request. Use browser validation when interaction behavior changes.

## Commits and Pull Requests

Use focused Conventional Commit messages, such as `feat: add parser support`, `fix: reject empty units`, or `test: cover daemon restart`. Keep unrelated changes separate. Pull requests should describe user-visible behavior, include relevant tests, document model or local-server assumptions, and call out configuration or security implications. Do not commit binaries, `target/` output, model files, credentials, or source copied into fixtures.

## Security and Configuration

The server must remain bound to `127.0.0.1`; daemon control routes require their local control token. Never log or commit API keys. Configuration precedence and model profiles are documented in `README.md`.

## Antigravity CLI Delegation

Use `agy` for delegation when the user requests it. Prefer small, explicit tasks: one isolated fix, focused regression, verification command, or documentation section. The primary agent owns integration, security and policy decisions, and qualitative model-output review.

- Verify available models with `agy models` and options with `agy --help`. The verified Flash model is `gemini-3.8-flash-medium`; do not assume other accounts provide the same models or `--mode auto`.
- Use noninteractive execution with `--mode accept-edits --print <prompt> --output-format stream-json --print-timeout 15m`. Do not combine an explicitly tiered model with a conflicting `--effort` value. Never run multiple tasks that edit the same files concurrently.
- Every prompt must specify the task, allowed edit paths, relevant symbols, acceptance checks, exact authorized commands, and reporting destination. Require preservation of existing work; prohibit unrelated edits, commits, configuration changes, installs, and model requests unless the task explicitly authorizes them. Prefer targeted reads and `rg`; `git grep` omits untracked files.
- Command permissions must be configured explicitly. Bare command-name rules did not authorize argument-bearing commands in the tested Windows CLI; use exact commands or narrowly scoped argument patterns. Do not use `--dangerously-skip-permissions`. On denial, report the exact command and resolve its authorization before resuming; do not evade it with another tool or spelling.
- Preserve machine-specific permissions in the user's Antigravity settings, not this repository. Never copy credentials, account information, or unrelated WSL infrastructure permissions into project files. Scope temporary environment workarounds to the child process; for the observed Windows sccache crash, clear `RUSTC_WRAPPER` only for that run if needed.
- For matching failures in native text edits, inspect line endings. Normalize only the assigned files to LF when necessary, preserving content and avoiding concurrent edits. Do not repeatedly rewrite whole files to work around a failed match.
- For bug fixes, require an observed failing regression before production edits, then focused passing checks. Existing-behavior coverage needs meaningful assertions but no artificial production change to force a failure. Settings and navigation must retain the explanation UX contract above.
- Monitor the actual process/session and streamed tool events. A quiet period is not proof of termination. A CLI result marked `SUCCESS` with an empty response or nonempty `denied_actions` is not task completion. Resume the same conversation with `--conversation <id>` when useful after resolving a blocker.
- Store prompts, logs, reports, and review snapshots under ignored `target/learning-evaluation/agy-tasks/`. Review the diff against a task-start snapshot because the worktree may already contain unrelated changes. Inspect actual command outputs and exit codes, not just the agent's summary. Run the relevant acceptance checks before accepting the result.
- Coordinate shared build artifacts as well as edit paths. Avoid overlapping Cargo builds with browser runs that use the same executable; prefer `cargo test --lib <filter>` for isolated library regressions. Copy a freshly built evaluation executable to the ignored artifact directory before long model runs so later builds do not replace it.
- A CPU-offloaded local model can leave too little Windows commit capacity for parallel Rust compilation. If diagnostics show paging-file error 1455 and cascading metadata/std resolution failures, retry with child-local `CARGO_BUILD_JOBS=1`; this recovered the verified full check. Do not infer a broken toolchain from those cascading resource errors or change machine-wide settings to hide them.
- Distinguish automated response validation from teaching quality. Mock-browser tests do not prove local-model accuracy; report whether a fresh live-model run occurred and keep unresolved factual or consistency failures explicit.
- For delegated local-model evaluations, prefer one reviewed runner command as the first substantive action and a 30-minute CLI timeout when serialized generation needs it. Preserve frozen executables and prior artifacts; record actual source/policy identities, prompt/contract versions, settings, prerequisite results and raw usage logs. Grade correctness independently of JSON validity, response length or latency. Flash's assessment is preliminary: the primary must verify boundary counterexamples, checked-operation semantics and claims about unseen helpers before acceptance.
- Bound evaluation review to named artifacts, the targeted source and the comparison candidate; request a concise report instead of broad report discovery. Check recommendations against application constraints: a whole-section annotation is correct when that is the supplied deterministic region. Verify reported executable names against their recorded hashes.
- Prefer Flash for execution and mechanically verifiable run metadata. Primary review must distinguish source evidence from actual output: do not credit a response with details merely present in supplied code. Any claimed quality improvement needs an exact output example checked against the artifact.
