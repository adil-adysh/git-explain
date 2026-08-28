# git-explain

`git-explain` explains changed source code in a local browser. It analyzes Git changes locally, then asks the model endpoint selected by the active profile for an explanation.

## Quick start

From inside a Git repository, create the documented starter configuration:

```text
git explain config init
```

Choose one local model-server preset. For llama.cpp running on its default port:

```text
git explain profile add qwen35b --preset llama-cpp --model qwen3.5-35b
git explain profile test qwen35b
git explain profile use qwen35b
git explain
```

If your llama.cpp server uses a different port, override only that port when
creating the profile:

```text
git explain profile add qwen35b-9000 --preset llama-cpp --model-port 9000 --model qwen3.5-35b
```

For Ollama, use its OpenAI-compatible endpoint instead:

```text
git explain profile add ollama --preset ollama --model qwen3:8b
git explain profile test ollama
git explain --profile ollama
```

Replace the example model names with the model loaded by your local server. `profile test` is a safe connection check: it never sends repository source.

For a hosted OpenAI-compatible endpoint, set the credential in your shell first, then keep only its environment-variable name in the profile:

```text
git explain profile add cloud --base-url https://api.example.com/v1 --model example-model --api-key-env CLOUD_API_KEY
git explain profile test cloud
git explain --profile cloud
```

Use `--profile` for a one-off run; `git explain profile use <name>` makes a profile the default. `GIT_EXPLAIN_PROFILE` can select a profile when an explicit command-line selection is not provided.

Useful first-run commands:

```text
# See the changed source units without contacting a model or opening a browser.
git explain --debug

# Inspect the effective configuration and available profiles.
git explain config show
git explain profile list

# Use a different local git-explain web-server port for this run.
git explain --port 9000

# Create repository-scoped preferences; it can select a trusted user profile,
# but cannot define an endpoint or credential.
git explain config init --repo
git explain profile use qwen35b --repo
```

Known presets provide local model-server defaults: `llama-cpp` uses `http://127.0.0.1:8080/v1`, and `ollama` uses `http://127.0.0.1:11434/v1`. Use `--model-port` to change only a preset endpoint's port, for example `git explain profile add local --preset llama-cpp --model-port 9000 --model your-model`. Use `--base-url` for a complete custom model endpoint; it cannot be combined with `--model-port`. Generic profiles require `--base-url`.

The global `git explain --port <PORT>` flag changes the git-explain local web-server port. It does not change the model endpoint. Profile `--model-port <PORT>` changes the model-server port.

## Profiles

A profile is a trusted user-level description of one model endpoint. It contains the protocol, optional preset, endpoint, model, optional environment-variable name for a credential, and optional normal/deep generation settings. All current profiles use `provider = "openai_compatible"`. `llama_cpp` and `ollama` are presets, not providers.

The generated configuration documents every supported user TOML setting, including a fully commented profile example, but deliberately creates no placeholder profile. Add a real endpoint before selecting it. A profile looks like:

```toml
[model]
profile = "local"

[profiles.local]
provider = "openai_compatible"
preset = "llama_cpp"
base_url = "http://127.0.0.1:8080/v1"
model = "your-local-model"

[profiles.local.normal]
max_tokens = 500
temperature = 0.2

[profiles.local.deep]
max_tokens = 2500
temperature = 0.3
```

Profile fields can be configured without editing TOML. Model fields are `--provider`, `--preset`, and `--model`; endpoint fields are `--base-url` and `--model-port`; authentication uses only `--api-key-env`. Normal and deep generation each support `--<mode>-reasoning true|false`, `--<mode>-max-tokens`, and `--<mode>-temperature`. Use the corresponding `--clear-<mode>-...` options when editing to restore an unspecified value.

For example:

```text
git explain profile add local --preset llama-cpp --model qwen3.5-35b \
  --normal-max-tokens 700 --normal-temperature 0.2 \
  --deep-reasoning true --deep-max-tokens 3000 --deep-temperature 0.3
git explain profile edit local --clear-normal-temperature
```

Generation fields are optional. An omitted field is omitted from the HTTP request, allowing cloud-compatible services to accept only the settings they support. Credentials are always read from `api_key_env`; profile files never contain secret values. Profile and config display commands redact credentials.

## Context management

Before each explanation, git-explain serializes the full prospective inference payload (system and user messages, roles, schema, template options, source, diff, and metadata) and estimates its input tokens. It then calculates `required = estimated input + configured max response tokens + 96 protocol tokens + max(384, 1/12 of the pre-margin total)`. It uses a deterministic conservative estimator for generic OpenAI-compatible endpoints. If the first focused prompt cannot fit, it makes one concise re-plan; if that also cannot fit, it stops before inference with the available and required budgets instead of sending an oversized request.

`context_window` is an optional profile cap for git-explain's own budgeting; it never changes a model server's allocation. Set or clear it with:

```text
git explain profile edit local --context-window 32768
git explain profile edit local --clear-context-window
```

For Ollama, `git explain profile test ollama` also reads native `/api/show` metadata for the model's theoretical maximum and `/api/ps` for the context currently allocated to the loaded model. Because git-explain sends OpenAI-compatible `/v1/chat/completions` requests, that loaded runtime allocation is the hard bound: a model that supports 131072 tokens but is loaded with 4096 is budgeted as 4096.

Ollama's OpenAI-compatible `/v1/chat/completions` API cannot increase context per request. Configure Ollama itself (for example, `OLLAMA_CONTEXT_LENGTH` before `ollama serve`, or a Modelfile with `PARAMETER num_ctx <tokens>`), reload the model, and run `git explain profile test ollama` again. Larger windows use more memory; choose one that fits the model and hardware.

llama.cpp is likewise controlled when its server starts, with `--ctx-size`; its OpenAI-compatible endpoint receives no invented context-size field. Generic OpenAI-compatible profiles are treated conservatively unless a dedicated adapter has verified a request-scoped context option. `git explain --debug` and `git explain profile test <name>` show which control mode applies.

Repository configuration at `.git/git-explain.toml` may select a logical profile name and repository-safe explanation and Git preferences:

```toml
[model]
profile = "work"
```

It cannot define profiles, endpoints, credential environment variables, or authentication. The `work` profile must be defined in the user configuration. This prevents a repository from silently redirecting source to an unexpected endpoint.

Create the restricted repository template with `git explain config init --repo`, then select a trusted user profile with `git explain profile use work --repo`. It documents every repository-safe application setting but never profile definitions, endpoints, models, presets, or credential references. `git explain config path` prints both configuration locations, and `git explain config show` identifies where the selection came from.

## Application configuration

Profiles own model endpoints and inference behavior. `config` owns reader preferences, explanation behavior, cache, the local server, Git analysis settings, and the selected profile. Every application setting has both a command and an accessible plain-text editor:

```text
git explain config show
git explain config edit
git explain config reader --experience intermediate --add-known-language Rust
git explain config explanation --depth deep --annotation-limit 20
git explain config cache --enabled false
git explain config server --port 9000 --open-browser false
git explain config git --include-staged true
git explain config model --profile local
```

List settings use `--add-*`, `--remove-*`, and `--clear-*` operations. `config edit` uses numbered menus and writes only after review and confirmation. Repository editing (`--repo`) can change application settings and select an already trusted user profile, but can never define profiles, endpoints, providers, or credentials.

`git explain --port 9000` is a runtime-only server override; `git explain config server --port 9000` persists the setting. Likewise `git explain --profile cloud` is a one-shot profile override, while `git explain profile use cloud` persists the default selection.

For testing or portable setups, `GIT_EXPLAIN_USER_CONFIG` can point to an alternate user configuration file. `git explain config path` and `config show` display the effective path. `GIT_EXPLAIN_PROFILE` remains a one-shot profile-selection override.

Use `git explain profile list`, `show`, `add`, `edit`, `test`, `use`, and `remove` to manage profiles. `edit` changes only supplied fields; `--clear-preset`, `--clear-api-key-env`, and the generation clear flags remove optional fields. `git explain config show` displays the resolved selection.

Run `git explain profile add` in an interactive terminal to create a profile through a plain-text, numbered wizard, or use `git explain profile add <name> ...` for scripted creation. Run `git explain profile edit <name>` for the corresponding editor. Changes are kept in a draft and saved only after review and confirmation; canceling leaves the configuration unchanged. The editor can change a preset model port without requiring a full URL and never asks for API-key contents. With explicit options such as `--model` or `--model-port`, profile creation and editing remain non-interactive and script-friendly.

## Commands

`git explain` explains current changes. Pass a revision to explain a commit. `--direct` bypasses the local daemon; `--debug` inspects detected changed source units without opening the browser. The daemon binds to `127.0.0.1`.

For help, use `git explain -h`, `git explain profile -h`, and `git explain config -h`. Git may intercept `git explain --help` before this application starts.

### Convenience aliases

Canonical command and option names are recommended in documentation. For frequent interactive use, these aliases are also available:

```text
Commands:  profile -> prof    config -> cfg
Options:   -m -> --model      -u -> --base-url
           -s -> --preset     -r -> --repo
           -f -> --force      -d -> --debug
```

The endpoint and runtime options `--model-port`, `--api-key-env`, `--provider`, `--profile`, `--port`, and `--direct` remain long-only.

Run `task check` before contributing.
