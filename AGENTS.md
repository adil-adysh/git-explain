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
