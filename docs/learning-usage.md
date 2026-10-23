# Personalized Learning and Explanation Preferences

`git-explain` tailors source-code explanations to your familiarity and current focus without altering Git analysis or your model endpoint. You can request syntax explanations for a language you are learning, trace concurrency and execution flow, ask for high-level architectural relationships, or keep changes concise.

Personalized learning is strictly decoupled from your **model profile**:
- **Model profile** (`git explain profile ...`, `--profile <name>`): Selects the model endpoint, provider, base URL, credentials, and inference token/temperature settings.
- **Learning profile** (`git explain learning ...`, `--learning-profile <name>`): Configures what you want explained—language familiarity, explanation focus, detail level, and known/learning concepts.

Selecting or changing a learning profile never redirects network traffic or alters model authentication.

---

## Quick Start

Inspect existing learning settings or create a named learning profile:

```text
# List configured learning profiles
git explain learning list

# Inspect user learning defaults or a named profile
git explain learning show
git explain learning show rust-learner

# Interactively create or edit a learning profile
git explain learning add rust-learner
git explain learning edit rust-learner

# Set a profile as your default
git explain learning use rust-learner

# Remove a learning profile (must select another default first if it was the default)
git explain learning remove rust-learner
```

When launching `git explain diff` or `git explain file`, apply one-off preferences or select a profile:

```text
# Use a specific learning profile for this run
git explain --learning-profile rust-learner diff

# Temporarily ignore saved profiles and use base user defaults
git explain --no-learning-profile diff

# Override specific preferences for this session
git explain --language-help explain --focus execution --detail detailed diff

# Opt in to repository-specific learning defaults
git explain --use-repository-learning diff
```

> [!NOTE]
> `--learning-profile <name>` and `--no-learning-profile` are mutually exclusive. Passing `--no-learning-profile` explicitly clears profile selection for the run, bypassing any configured default profile and resolving settings from user defaults (and repository defaults if `--use-repository-learning` is also set).

---

## Configuration Reference

Learning preferences are configured in your user configuration file (`~/.config/git-explain/config.toml` or platform equivalent; run `git explain config path` to check).

### Valid Enum Settings

| Setting | Enum Values | Description |
| --- | --- | --- |
| `language_help` | `automatic`, `explain`, `brief`, `skip` | How much language syntax and basic mechanics to explain. `automatic` is the preference configuration value; it checks whether the file's language is in `learning_languages` (resolving to `explain`), `known_languages` (resolving to `skip`), or neither (`brief` / Brief reminders). If a language is listed in both `learning_languages` and `known_languages`, `learning_languages` takes precedence (`explain`). In the resolved policy (`ResolvedExplanationPolicy`), `language_help` is always an explicit value (`explain`, `brief`, or `skip`), never `automatic`. |
| `focus` | `balanced`, `syntax`, `execution`, `reasoning`, `architecture` | The primary lens for explanations. `syntax` requests focus on language constructs; `execution` traces operational control and state changes; `reasoning` explores structural motivations and tradeoffs; `architecture` explains boundaries and component roles. |
| `detail` | `concise`, `balanced`, `detailed` | Requested depth of teaching and walkthrough steps, within the configured model token and context limits. |

### TOML Configuration Structure

```toml
[learning]
# Default learning profile to activate if not overridden by CLI flags
profile = "systems-dev"

# Global user defaults across all profiles and languages
[learning.defaults]
language_help = "automatic"
focus = "balanced"
detail = "balanced"
known_languages = ["python", "javascript"]
learning_languages = ["rust", "go"]
known_frameworks = ["django", "react"]
learning_frameworks = ["tokio"]
known_domains = ["web", "http"]
learning_domains = ["distributed-systems", "concurrency"]
known_concepts = ["async/await", "garbage-collection"]
learning_concepts = ["ownership", "borrowing", "memory-barriers"]

# Named learning profile: base preferences
[learning.profiles.systems-dev.preferences]
language_help = "explain"
focus = "execution"
detail = "detailed"
learning_languages = ["rust"]

# Per-language overrides within a profile:
# Keys are normalized (e.g. "rust", "rs", "python", "py", "typescript", "ts", "c_sharp")
[learning.profiles.systems-dev.languages.rust]
language_help = "explain"
focus = "syntax"
learning_concepts = ["lifetimes", "pinning", "drop-guards"]

[learning.profiles.systems-dev.languages.python]
language_help = "skip"
focus = "architecture"
```

### Repository Defaults (`.git/git-explain.toml`)

Repositories can suggest learning defaults for contributors (for example, pointing out that a repository is written in Rust and uses Tokio), but **cannot** define learning profiles or force settings on readers:

```toml
# .git/git-explain.toml
[learning.defaults]
focus = "execution"
learning_languages = ["rust"]
learning_frameworks = ["tokio"]
```

> [!IMPORTANT]
> **Explicit repository opt-in required**: Repository defaults are ignored by default to protect reader preferences. They are only merged if you explicitly opt in using the `--use-repository-learning` CLI flag or by checking the **Use repository learning defaults** box in the browser UI.

### Legacy `[reader]` Configuration

The legacy `[reader]` configuration section is **intentionally unsupported**. No automated migration is performed. If an older `[reader]` section is detected in your configuration, `git-explain` will reject it with instructions to use `[learning.defaults]` or `git explain learning`. General experience levels (such as `experience = "beginner"` or `experience = "expert"`) from older reader configs are unsupported and rejected; learning preferences use explicit `language_help`, `focus`, `detail`, and familiarity lists instead.

---

## Resolution and Inheritance Hierarchy

When an explanation is prepared for a code section, settings are resolved from broadest to most specific:

1. **Application defaults**: Automatic language help (resolves to `brief` / Brief reminders when no language familiarity is specified), balanced focus and detail, and empty familiarity lists.
2. **User defaults**: Values from `[learning.defaults]` in your user configuration.
3. **Repository defaults** (*if opted in*): Values from `[learning.defaults]` in `.git/git-explain.toml`.
4. **Active profile base**: `[learning.profiles.<name>.preferences]` for the selected profile.
5. **Profile language override**: `[learning.profiles.<name>.languages.<lang>]` matching the file's normalized language.
6. **Session preferences**: Runtime overrides passed via CLI flags (`--language-help`, `--focus`, `--detail`) or set in the browser's "For this session" controls.
7. **View-level temporary overrides**: In-browser adjustments for the specific code section and active tab view.

Metadata provenance tracking records which layer supplied each effective preference, using internal source labels: `"application defaults"`, `"user defaults"`, `"repository defaults"`, `"learning profile"`, `"language preferences"`, `"session preferences"`, and `"current view"`. This provenance is presentation-only and excluded from policy hashes and cache keys.

### Omitted Fields vs. Explicitly Empty Lists

- **Omitted fields and "Inherit"**: Any preference not explicitly set in a layer (or set to "Inherit" in the browser UI dropdowns) inherits from the broader configuration layer beneath it.
- **Explicitly empty lists**: Setting a list to empty (`known_languages = []` in TOML, `-` in interactive CLI prompts, or clearing the field in the browser UI saved editor) deliberately **clears** any values inherited from broader layers, resulting in an empty list. In interactive CLI prompts, leaving a familiarity prompt blank preserves the existing list value, whereas entering `-` clears it.

### Deterministic Policy Identity and Caching

- Once resolved, preferences are normalized (language aliases mapped, strings lowercased, lists sorted and deduplicated) into an immutable `ResolvedExplanationPolicy`.
- A SHA-256 hash of this serialized policy forms its `policy_id`.
- Different learning configurations that resolve to identical effective settings share a `policy_id`. They also share an explanation cache key when the source, request kind, model and generation settings, and other request inputs match. Provenance labels, learning profile names, and transport credentials are excluded from the key.
- Explanation variants are distinguished by source identity, explanation kind, effective policy, prompt/response contract versions, model generation parameters (`reasoning`, `max_tokens`, `temperature`), and the prerequisite result identity for Deep explanations. Switching between profiles or temporary settings restores previously generated results for that policy without calling the model again.

---

## Browser Workflow

The browser interface provides real-time control over explanation personalization:

```
+---------------------------------------------------------------------------------+
|  > Learning settings                                                            |
|    Learning profile: [ systems-dev       v ]  [x] Use repository learning       |
|                                                   defaults                      |
|    For this session:                                                            |
|      Language help: [ Explain syntax    v ]                                     |
|      Explanation focus: [ Execution flow v ]                                    |
|      Detail: [ Detailed                 v ]                                     |
|    [ Reset temporary settings ]                                                 |
|                                                                                 |
|    > Customize saved preferences                                                |
|      Edit: [ Selected profile v ]                                               |
|      Language override (leave empty for all languages): [ rust        ]         |
|      ...                                                                        |
|      Profile name: [                  ]                                         |
|      [ Save preferences ]  [ Save as profile ]                                  |
|      [ Use selected profile by default ]                                        |
+---------------------------------------------------------------------------------+
```

### Explanations are Generated On Demand Only

Changing settings in the dropdowns or switching between explanation tabs (Overview, Changes, Concepts, Deep) **never generates an explanation or contacts the model**.
- Settings changes immediately recompute the resolved policy status shown on the code section (e.g., `Pending settings: explain · execution · detailed. Press Generate to apply.`).
- Generation only begins when you click the visible **Generate selected explanation** (or **Regenerate explanation**) button.

### Scope of Reset Buttons

- **Reset temporary settings** (under top-level Learning settings): Clears both the session preferences (`learning.selection.session`) and all per-section view overrides across all code sections in the document.
- **Reset view settings** (inside a code section's "Settings for this code section and view" panel): Clears temporary overrides only for the currently active tab view on that specific code section.

### Invalidation Rules

- **Overview** (file mode) and **Changes** (diff mode) serve as prerequisites for **Deep** explanations.
- In diff mode, regenerating Changes invalidates Deep results for that code section that were generated under that same policy. In diff mode, regenerating Overview does not invalidate Deep because Changes is the prerequisite.
- In file mode, regenerating Overview invalidates Deep results for that code section that were generated under that same policy.
- Deep explanations for other policies remain available and are not deleted.
- Independent views (such as Concepts) have no prerequisites and retain their generated content across other tab updates.

### Feedback Workflow ("I know this already" / "Explain this term")

Under the **Concepts** view, inline concept annotations provide two feedback buttons on titled concepts:
- **"I know this already"**: Moves the concept title to `known_concepts` in your temporary view override (removing it from `learning_concepts`).
- **"Explain this term"**: Moves the concept title to `learning_concepts` in your temporary view override (removing it from `known_concepts`).

Clicking either button updates the pending policy for that code section immediately and populates the editable **Feedback term** text input (`<input class="feedback-term">`) in the section's view settings panel. You can edit the term label in this field before persisting it.

To persist these adjustments permanently into your active learning profile:
1. Ensure a learning profile is selected (saving feedback is not permitted when "User defaults" is active).
2. Click the explicit **Save feedback to selected profile** button.

This persists the trimmed feedback term into `[learning.profiles.<name>.preferences]` (`known_concepts` or `learning_concepts`) in your configuration file using safe file locking.

### Structured Deep Explanations

Deep explanations request structured pedagogical walkthroughs from the model rather than unstructured text. The model prompt specifies structured JSON fields conforming to the `DeepTeaching` schema:
- **Purpose**: High-level explanation of what the code achieves, its inputs, and expected outcomes.
- **Walkthrough**: An ordered, causal step-by-step trace of visible operations and state changes (rendered as an ordered list `<ol>`).
- **Boundaries**: Explanations of boundary, error, or fallback branches visible in the source (rendered as an unordered list `<ul>`, or empty if no boundary conditions are present in the source).
- **Example**: An illustrative example tracing expressions through the code (optional; omitted if null or when an illustrative example cannot be established).

When returned by the model, these sections are rendered under distinct `<h5>` HTML headings (`Purpose`, `Walkthrough`, `Boundaries`, and optionally `Example` when present) inside the Detailed explanation section. Prompting instructs the model to adhere to source evidence and structured schema constraints, but model output accuracy and quality depend on the configured model server and inference capabilities.

---

## Safety and Concurrency

- **Atomic file updates**: Modifying saved preferences through CLI or browser uses atomic file replacement (via `config.learning-<pid>.tmp`) and advisory file locking (`config.learning.lock`).
- **Conflict detection**: Saves verify the configuration revision hash against concurrent modifications and reject stale writes with HTTP 409 Conflict ("learning preferences changed; reload before saving", shown in the browser as "Settings changed elsewhere. Reload before saving again.").
- **CSRF and Origin protection**: Saving through the browser learning API requires:
  - Header `x-git-explain-learning-token` matching the per-session token.
  - Header `Content-Type: application/json`.
  - `Host` header must name an accepted loopback host (`127.0.0.1`, `localhost`, or `[::1]`).
  - `Origin` header must strictly match `http://{host}`.
  - Unauthorized requests return HTTP 403 Forbidden with exact message `"Learning settings require this page's settings token and origin."`.
