# Repository Guidelines

## Project Structure

`src/` contains the Rust application. CLI and configuration code live in `cli.rs` and `config.rs`; Git analysis is in `git.rs`, `diff.rs`, `analyzer.rs`, and `explain.rs`; supported language parsers are under `src/language/`; daemon, HTTP, and browser behavior is implemented in `daemon.rs`, `server.rs`, and `web.rs`. Shared snapshot and runtime logic is in `snapshot.rs` and `runtime.rs`. Integration tests and fixtures are in `tests/`. Design notes are in `docs/`, installation automation is in `scripts/`, and CI is defined in `.github/workflows/ci.yml`.

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

Equivalent direct commands are `cargo fmt -- --check`, `cargo test`, and `cargo build`. Run `git explain --debug` for deterministic analysis without opening the browser. The normal daemon uses loopback port 8192; `git explain --direct` is the one-shot fallback.

## Coding Style and Naming

Format Rust with `cargo fmt`; CI treats formatting failures as errors. Use idiomatic Rust 2021, four-space indentation, `snake_case` for functions/modules, `PascalCase` for types, and descriptive names for snapshot, unit, and repository boundaries. Keep analysis deterministic and avoid putting model, web-server, or mutable runtime concerns into `RepositoryAnalyzer`.

## Testing Guidelines

Tests use Rust’s built-in test framework plus fixture-based integration tests. Add focused regression coverage in `tests/` for parser, Git-history, daemon lifecycle, HTTP/model, snapshot, or rendered-UI changes as appropriate. Run `task check` before submitting; CI runs formatting, `cargo test`, and a debug build.

## Commits and Pull Requests

Use focused Conventional Commit messages, such as `feat: add parser support`, `fix: reject empty units`, or `test: cover daemon restart`. Keep unrelated changes separate. Pull requests should describe user-visible behavior, include relevant tests, document model or local-server assumptions, and call out configuration or security implications. Do not commit binaries, `target/` output, model files, credentials, or source copied into fixtures.

## Security and Configuration

The server must remain bound to `127.0.0.1`; daemon control routes require their local control token. Never log or commit API keys. Configuration precedence and model profiles are documented in `README.md`.
