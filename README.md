# git-explain

`git-explain` explains changed source code in a local browser. It analyzes Git changes locally, then asks the model endpoint selected by the active profile for an explanation.

## Quick start

Create the starter configuration:

```text
git explain config init
```

Configure and select a local llama.cpp endpoint:

```text
git explain profile add local --preset llama-cpp --model your-model
git explain profile test local
git explain profile use local
git explain
```

For a hosted OpenAI-compatible endpoint, keep the secret outside configuration:

```text
git explain profile add cloud --base-url https://api.example.com/v1 --model example-model --api-key-env CLOUD_API_KEY
git explain profile test cloud
git explain --profile cloud
```

`profile test` never sends repository source. It verifies model listing when available and otherwise uses a fixed, source-free compatibility request. `--profile` affects one invocation. `GIT_EXPLAIN_PROFILE` can select a profile when an explicit command-line selection is not provided.

Known presets provide local model-server defaults: `llama-cpp` uses `http://127.0.0.1:8083/v1`, and `ollama` uses `http://127.0.0.1:11434/v1`. Use `--model-port` to change only a preset endpoint's port, for example `git explain profile add local --preset llama-cpp --model-port 9000 --model your-model`. Use `--base-url` for a complete custom model endpoint; it cannot be combined with `--model-port`. Generic profiles require `--base-url`.

The global `git explain --port <PORT>` flag changes the git-explain local web-server port. It does not change the model endpoint. Profile `--model-port <PORT>` changes the model-server port.

## Profiles

A profile is a trusted user-level description of one model endpoint. It contains the protocol, optional preset, endpoint, model, optional environment-variable name for a credential, and optional normal/deep generation settings. All current profiles use `provider = "openai_compatible"`. `llama_cpp` and `ollama` are presets, not providers.

The generated configuration deliberately contains no placeholder profile. Add a real endpoint before selecting it. A profile looks like:

```toml
[model]
profile = "local"

[profiles.local]
provider = "openai_compatible"
preset = "llama_cpp"
base_url = "http://127.0.0.1:8083/v1"
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

Repository configuration at `.git/git-explain.toml` may select a logical profile name and repository-safe explanation and Git preferences:

```toml
[model]
profile = "work"
```

It cannot define profiles, endpoints, credential environment variables, or authentication. The `work` profile must be defined in the user configuration. This prevents a repository from silently redirecting source to an unexpected endpoint.

Create the restricted repository file with `git explain config init --repo`, then select a trusted user profile with `git explain profile use work --repo`. Endpoints, models, presets, generation settings, and credentials remain user-owned. `git explain config path` prints both configuration locations, and `git explain config show` identifies where the selection came from.

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
