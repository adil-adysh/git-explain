# git-explain

Local, screen-reader-first explanations of changed functions across Rust, Python, Go, Java, C#, TypeScript, JavaScript, C, and C++.

## Use

Build and install the Git subcommand:

```text
cargo install --path .
git explain
```

## Task automation

This project includes a [Task](https://taskfile.dev/) workflow for repeatable Windows builds and installation:

```text
task build              # development binary
task build-release      # optimized release binary
task check              # format check, tests, and diff check
task install            # build and install the release binary
task install-dev        # build and install the development binary
```

Installation targets `C:\Users\<user>\.local\bin\git-explain.exe`. If the local daemon is running, the installer stops it before replacing the locked binary and starts it again afterward. If graceful shutdown leaves the process busy, use:

```text
task install FORCE=true
```

The force path terminates only the `git-explain.exe` process running from the user-local install path before copying the verified binary.

For deterministic inspection without starting the browser:

```text
git explain --debug
```

To explain an existing commit instead of the working tree:

```text
git explain 699fdd6
git explain HEAD
git explain HEAD~1 --debug
```

Commit mode compares the selected commit with its first parent and retrieves source from the selected commit itself. Root commits are compared with Git's empty tree. Merge commits use the first parent and identify that choice in debug output and the web page. Deleted files are reported but do not receive annotated source explanations yet. Renames with content changes use the new committed path; pure rename/copy metadata without a textual diff may not produce a source symbol. Binary files are skipped and never sent to the model.

The model endpoint is OpenAI-compatible. For a quick one-off override, the existing environment variables remain supported:

* `GIT_EXPLAIN_BASE_URL`
* `GIT_EXPLAIN_MODEL`
* `GIT_EXPLAIN_API_KEY` (optional for local servers)
* `GIT_EXPLAIN_PROFILE`

Only changed supported-language functions and their relevant Git diff are sent to the configured model. The tool does not execute source, read `.git` internals, or write explanations into source files. The server binds to `127.0.0.1` only.

## Supported languages

The analyzer registry supports Rust (`.rs`), Python (`.py`), Go (`.go`), Java (`.java`), C# (`.cs`), TypeScript (`.ts`, `.tsx`), JavaScript (`.js`, `.jsx`), C (`.c`), and C++ (`.cpp`, `.cc`, `.cxx`, `.hpp`). A single change set may contain any mix of these files; unsupported files are ignored without preventing supported files from being explained. Python decorators, async functions, class methods, and nested functions are included where relevant. Go functions and pointer/value receiver methods are qualified by name, such as `Service.Authenticate`.

The daemon explanation page runs at `http://127.0.0.1:8192` by default. The explicit `--direct` fallback uses the configured one-shot page port (8081 by default). Set `GIT_EXPLAIN_BASE_URL` to the OpenAI-compatible model server you want to use; the model server must listen on a different port from the web page.

## Local daemon

Normal web-mode commands use a local loopback-only daemon automatically:

```text
git explain
git explain HEAD~1
git explain daemon status
git explain daemon refresh
git explain daemon stop
```

The daemon normally listens on `127.0.0.1:8192`. It starts idle, then opens a repository session when `git explain` runs; sessions receive opaque IDs and remain isolated from one another. The most recently opened session is active for `git explain daemon refresh`, while previously opened sessions remain available until the bounded registry evicts the least recently used session. Refresh deterministically reanalyzes the active repository, compares snapshot identity, and atomically replaces the snapshot only when the repository changed. It never triggers model inference; the previous snapshot remains active while analysis runs. An open daemon page checks its session snapshot generation periodically and shows an accessible reload action when a newer snapshot is available. For troubleshooting, run `git explain daemon run` in the foreground. `git explain --direct` remains an explicit one-shot fallback.

## Configuration

Initialize a user configuration and inspect the paths used by the loader:

```text
git explain config init
git explain config path
git explain config show
```

Configuration precedence is:

```text
CLI > environment > repository (.git/git-explain.toml) > user config > built-in defaults
```

The user file is stored in the platform-appropriate per-user configuration directory. `config init` never overwrites an existing file unless `--force` is supplied. `config show` redacts API key contents.

### llama.cpp profile

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

### Ollama profile

```toml
[profiles.ministral]
provider = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
model = "ministral-3:8b"
```

For the locally managed Unsloth llama.cpp preset used in live testing:

```text
git explain --profile unsloth35b
```

The generated example configuration includes the `unsloth35b` profile. It uses
`http://127.0.0.1:8083/v1`, model `git-explain-unsloth35b`, concise normal
generation, and a larger deep-mode output budget for separated reasoning.

Select a profile for one invocation without changing files:

```text
git explain --profile ministral
```

Reader context can be configured without changing the explanation workflow:

```toml
[reader]
experience = "experienced"
known_languages = ["python", "go"]
learning_languages = ["rust", "typescript"]
known_frameworks = ["fastapi"]
learning_frameworks = ["axum"]
```

`git.include_untracked` is parsed and shown but is not currently included in Git diffs; untracked-file analysis is intentionally not implied by the setting.

For the llama.cpp preset used during live testing:

```powershell
$env:GIT_EXPLAIN_BASE_URL = "http://127.0.0.1:8083/v1"
$env:GIT_EXPLAIN_MODEL = "git-explain-qwen35b"
git explain
```

The dedicated `git-explain-qwen35b` preset is defined in `D:\llama-cpp\models.ini` with reasoning disabled and `enable_thinking` disabled for concise JSON-compatible responses.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
