# git-explain

Local, screen-reader-first explanations of changed functions across Rust, Python, Go, Java, C#, TypeScript, JavaScript, C, and C++.

## Use

Build and install the Git subcommand:

```text
cargo install --path .
git explain
```

For deterministic inspection without starting the browser:

```text
git explain --debug
```

The model endpoint is OpenAI-compatible. For a quick one-off override, the existing environment variables remain supported:

* `GIT_EXPLAIN_BASE_URL`
* `GIT_EXPLAIN_MODEL`
* `GIT_EXPLAIN_API_KEY` (optional for local servers)
* `GIT_EXPLAIN_PROFILE`

Only changed supported-language functions and their relevant Git diff are sent to the configured model. The tool does not execute source, read `.git` internals, or write explanations into source files. The server binds to `127.0.0.1` only.

## Supported languages

The analyzer registry supports Rust (`.rs`), Python (`.py`), Go (`.go`), Java (`.java`), C# (`.cs`), TypeScript (`.ts`, `.tsx`), JavaScript (`.js`, `.jsx`), C (`.c`), and C++ (`.cpp`, `.cc`, `.cxx`, `.hpp`). A single change set may contain any mix of these files; unsupported files are ignored without preventing supported files from being explained. Python decorators, async functions, class methods, and nested functions are included where relevant. Go functions and pointer/value receiver methods are qualified by name, such as `Service.Authenticate`.

The local explanation page runs at `http://127.0.0.1:8081` by default. Set `GIT_EXPLAIN_BASE_URL` to the OpenAI-compatible model server you want to use; the model server must listen on a different port from the web page.

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
