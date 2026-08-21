# git-explain

Local, screen-reader-first explanations of changed functions across Rust, Python, and Go.

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

The model endpoint is OpenAI-compatible and configurable with:

* `GIT_EXPLAIN_BASE_URL` (default `http://127.0.0.1:8000/v1`)
* `GIT_EXPLAIN_MODEL` (default `local-model`)
* `GIT_EXPLAIN_API_KEY` (optional for local servers)

Only changed supported-language functions and their relevant Git diff are sent to the configured model. The tool does not execute source, read `.git` internals, or write explanations into source files. The server binds to `127.0.0.1` only.

## Supported languages

The analyzer registry supports Rust (`.rs`), Python (`.py`), Go (`.go`), Java (`.java`), C# (`.cs`), TypeScript (`.ts`, `.tsx`), JavaScript (`.js`, `.jsx`), C (`.c`), and C++ (`.cpp`, `.cc`, `.cxx`, `.hpp`). A single change set may contain any mix of these files; unsupported files are ignored without preventing supported files from being explained. Python decorators, async functions, class methods, and nested functions are included where relevant. Go functions and pointer/value receiver methods are qualified by name, such as `Service.Authenticate`.

The local explanation page runs at `http://127.0.0.1:8081` by default. Set `GIT_EXPLAIN_BASE_URL` to the OpenAI-compatible model server you want to use; the model server must listen on a different port from the web page.

For the llama.cpp preset used during live testing:

```powershell
$env:GIT_EXPLAIN_BASE_URL = "http://127.0.0.1:8083/v1"
$env:GIT_EXPLAIN_MODEL = "git-explain-qwen35b"
git explain
```

The dedicated `git-explain-qwen35b` preset is defined in `D:\llama-cpp\models.ini` with reasoning disabled and `enable_thinking` disabled for concise JSON-compatible responses.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
