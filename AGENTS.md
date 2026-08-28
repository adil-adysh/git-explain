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

## Testing Guidelines

Tests use Rust’s built-in test framework plus fixture-based integration tests. Add focused regression coverage in `tests/` for parser, Git-history, daemon lifecycle, HTTP/model, snapshot, or rendered-UI changes as appropriate. Run `task check` before submitting; CI runs formatting, `cargo test`, and a debug build.

## Commits and Pull Requests

Use focused Conventional Commit messages, such as `feat: add parser support`, `fix: reject empty units`, or `test: cover daemon restart`. Keep unrelated changes separate. Pull requests should describe user-visible behavior, include relevant tests, document model or local-server assumptions, and call out configuration or security implications. Do not commit binaries, `target/` output, model files, credentials, or source copied into fixtures.

## Security and Configuration

The server must remain bound to `127.0.0.1`; daemon control routes require their local control token. Never log or commit API keys. Configuration precedence and model profiles are documented in `README.md`.
