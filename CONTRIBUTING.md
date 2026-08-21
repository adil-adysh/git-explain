# Contributing to git-explain

Thanks for helping improve `git-explain`.

## Development

Install Rust through [rustup](https://rustup.rs/), then run:

```text
cargo fmt -- --check
cargo test
cargo build
```

The project is intentionally local and single-user. Keep changes focused on helping readers understand changed source code. Avoid adding repository-wide indexing, embeddings, automated code review, or unrelated UI frameworks without first discussing the scope.

## Pull requests

Please include:

- a short explanation of the user-facing behavior;
- tests for deterministic parsing, rendering, or model-boundary changes;
- any model or local-server assumptions needed to reproduce the change.

Do not commit model files, generated build output, API keys, or repository source copied into test fixtures.

Use Conventional Commit-style messages, for example:

```text
feat: add Rust method annotations
fix: preserve source lines around annotations
test: cover malformed model output
```

