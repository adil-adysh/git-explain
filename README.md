# git-explain

`git-explain` is a local, screen-reader-first Git subcommand that explains changed functions and methods. It analyzes Git changes, sends selected source context to a configured OpenAI-compatible model, and presents the result in an accessible local web page.

## Why git-explain

Use it when a change is unfamiliar, when reviewing a commit, or when you want a concise explanation of control flow and concepts around changed code. It focuses on changed code units instead of sending or explaining an entire repository.

## Quick start

From a Git repository with a reachable configured model endpoint:

```text
cargo install --path .
git explain
```

The normal command starts the local daemon when needed, creates a repository session, and opens the explanation page. To explain an existing commit:

```text
git explain HEAD
```

To inspect Git analysis without starting a browser or calling the model:

```text
git explain --debug
```

Use `git explain -h` for options and examples. Git reserves `git explain --help` for external command documentation lookup.

## What happens when it runs

```text
Git changes
  -> supported source files
  -> changed functions, methods, and declarations
  -> selected source unit plus relevant diff
  -> configured OpenAI-compatible model endpoint
  -> accessible explanation page
```

The analyzer does not normally send the whole repository. It sends the complete selected source unit and its relevant diff hunk, together with metadata such as language, unit name, changed regions, and Git context.

## What it explains

Working-tree analysis uses Git's diff against the configured target, with staged changes included by default. Untracked files are currently excluded.

Commit analysis compares the selected commit with its first parent. A root commit is compared with Git's empty tree. Merge commits use the first parent, and the selected commit supplies the source text. Renames with content changes use the new path. Deleted files are reported but do not receive annotated source explanations. Binary files and unsupported files are skipped and are never sent to the model.

Clean trees, unsupported-only changes, and supported files with no detectable changed unit are successful no-op states; the command reports them instead of opening an empty page.

## Installation

### From a checkout

```text
cargo install --path .
```

On Windows, the Taskfile installs the binary into:

```text
C:\Users\<user>\.local\bin\git-explain.exe
```

Supported Taskfile commands:

```text
task build
task build-release
task check
task install
task install-dev
```

The installer gracefully stops the same user-local daemon before replacing a locked binary, verifies the copied binary's SHA-256 hash, and restarts the daemon when appropriate. If graceful stop leaves it busy, use `task install FORCE=true`.

## Usage

### Working tree

```text
git explain
```

This is the normal workflow. Staged and unstaged changes are combined when `git.include_staged` is enabled, which is the default. Untracked files are not included.

### Existing commits

```text
git explain HEAD
git explain HEAD~1
git explain <commit-or-revision>
```

Revision expressions use Git revision resolution. The page identifies the selected commit, subject, parent, and first-parent behavior for merges.

### Debug mode

```text
git explain --debug
git explain HEAD~1 --debug
```

Debug mode performs deterministic Git and source analysis, prints detected changes and units, and exits without starting the browser or requesting model explanations. It is useful for checking revision selection, diff interpretation, parser discovery, deleted files, and changed regions.

### Direct mode

```text
git explain --direct
git explain HEAD --direct
```

Direct mode bypasses the background daemon and runs a one-shot web server using the configured server port, `8081` by default. It is a fallback for daemon troubleshooting. It opens the browser when `server.open_browser` is enabled; if opening fails, it prints the URL for manual use.

`--profile <name>` selects a profile for one invocation. `--port <port>` overrides the port used when starting the relevant server. Place these options before a subcommand, for example `git explain --profile unsloth35b`.

## Local daemon

Normal web-mode commands use a loopback-only daemon at `http://127.0.0.1:8192` by default:

```text
git explain daemon status
git explain daemon refresh
git explain daemon stop
git explain daemon run
```

`daemon status` reports whether the daemon is running, its address, process ID, protocol version, and whether an active repository session exists. `daemon refresh` reanalyzes that active repository, reports whether the snapshot changed, and does not perform model inference. When the repository is unchanged, the current snapshot remains active.

Sessions are repository-isolated and can coexist. The most recently opened session is active for refresh; the bounded registry evicts the least recently used session when full. `daemon run` keeps the server in the foreground for troubleshooting startup and bind failures.

## Configuration

Use these commands to inspect configuration:

```text
git explain config init
git explain config path
git explain config show
```

The user file is stored in the platform-appropriate application configuration directory. Repository configuration, when present, is stored at `.git/git-explain.toml`. Values are resolved in this order:

```text
CLI profile and flags
  > environment variables
  > repository configuration
  > user configuration
  > built-in defaults
```

`config init` does not overwrite an existing user file unless `--force` is supplied. `config show` displays configuration paths, the active and available profiles, resolved model/server settings, and a redacted API-key status.

### Profiles and llama.cpp

Profiles describe different model servers and generation settings. This is a valid llama.cpp-compatible profile:

```toml
[model]
profile = "qwen35b"

[profiles.qwen35b]
provider = "llama_cpp"
base_url = "http://127.0.0.1:8081/v1"
model = "qwen36-35b-a3b"
api_key_env = "GIT_EXPLAIN_API_KEY"

[profiles.qwen35b.normal]
reasoning = false
max_tokens = 500
temperature = 0.2

[profiles.qwen35b.deep]
reasoning = true
max_tokens = 2500
temperature = 0.3
```

The normal profile controls the initial concise explanation; deep controls the optional detailed explanation. `git-explain` does not start llama.cpp.

### Ollama

Ollama is used through its OpenAI-compatible endpoint, not its native API:

```toml
[profiles.ollama]
provider = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
model = "ministral-3:8b"
```

### Environment overrides

Supported variables are `GIT_EXPLAIN_BASE_URL`, `GIT_EXPLAIN_MODEL`, `GIT_EXPLAIN_API_KEY`, and `GIT_EXPLAIN_PROFILE`. The API key may be omitted for local servers. CLI profile selection takes precedence over `GIT_EXPLAIN_PROFILE`; environment URL, model, and API-key values override profile values.

### Reader context

Reader settings shape assumed background and terminology. They do not change static analysis:

```toml
[reader]
experience = "experienced"
known_languages = ["python", "go"]
learning_languages = ["rust", "typescript"]
known_frameworks = ["fastapi"]
learning_frameworks = ["axum"]
```

## Supported languages

- Rust: `.rs`
- Python: `.py`
- Go: `.go`
- Java: `.java`
- C#: `.cs`
- TypeScript: `.ts`, `.tsx`
- JavaScript: `.js`, `.jsx`
- C: `.c`
- C++: `.cpp`, `.cc`, `.cxx`, `.hpp`

Python decorators, async functions, class methods, and nested functions are handled where relevant. Go receiver methods can be reported with qualified names such as `Service.Authenticate`.

## Privacy and security

`git-explain` binds its web server to loopback only, does not execute source code, does not read Git internals directly, does not modify source files, skips binary files, and sends only selected changed source units, relevant diff context, and explanation metadata to the configured endpoint.

If that endpoint is remote, the selected source and diff context leave the machine. Local hosting by `git-explain` does not make a remote model service local.

## Current limitations

- Deleted files are reported but are not source-explained.
- Pure rename or copy metadata without a textual diff may not yield a source unit.
- Unsupported files and binary files are skipped.
- Untracked files are not analyzed.
- `git.include_untracked` is parsed and displayed but is not implemented in Git diff collection.
- Explanations are model-generated and should be checked against the displayed source and diff.

## Troubleshooting

For a clean tree or unsupported changes, run `git explain --debug`; these are successful no-op states. For model or profile failures, run `git explain config show` and verify the configured OpenAI-compatible endpoint and model name. For daemon or port problems, use:

```text
git explain daemon status
git explain daemon stop
git explain daemon run
```

Unknown profiles list available names. Malformed configuration is reported with the affected file. If the browser does not open, use the URL printed by the command. The daemon binds to loopback and stale metadata is removed when its recorded process is no longer healthy.

Errors go to stderr. Exit status `0` means success or a legitimate no-op; `2` means a Git, revision, configuration, or usage problem; `3` means model connectivity or inference failure; `4` means a daemon/process failure; and `1` is reserved for unexpected failures.

## Development

The project uses Rust stable. Common commands are:

```text
task check
task build
task build-release
task install-dev
```

Equivalent commands are:

```text
cargo fmt -- --check
cargo test
cargo build
```

Tests use temporary Git repositories and local fakes where needed. Do not commit binaries, `target/` output, model files, credentials, or machine-specific configuration.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
