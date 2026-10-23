# Personalized explanation and learning design

Status: Working-tree implementation with open gaps, updated 2026-10-22.
This document describes the implemented personalized learning architecture in
the current working tree and identifies concrete remaining gaps. For hands-on
CLI, configuration, and browser usage, see [learning-usage.md](learning-usage.md).
The [explanation UX contract](explanation-ux.md) remains authoritative for
generation, source boundaries, explanation views, and local-server behavior.

## Problem and intended outcome

A single beginner/intermediate/expert setting mixes several different needs.
An experienced Python developer may understand an algorithm immediately but
need Rust syntax explained. Someone who can write Rust may instead struggle
to follow concurrent execution or understand an architectural boundary.

Personalization should separate what a reader knows from the help they want
for the current task. Defaults should work without onboarding, and readers
should be able to customize their experience without maintaining a long form
or adopting a fixed persona. Preferences express requested assistance, not an
assessment of the reader's intelligence or ability.

## Independent dimensions

| Dimension | Meaning | Examples |
| --- | --- | --- |
| Language familiarity | Syntax and semantics the reader knows in this particular language | Explain unfamiliar Rust syntax; give brief reminders; skip Python basics |
| Explanation focus | The question the reader wants answered now | Trace execution; explain reasoning and tradeoffs; explain architectural relationships |
| Library and domain familiarity | Knowledge beyond the programming language | Learning Tokio; familiar with HTTP; unfamiliar with Git revisions |
| Detail level | How much explanation is useful for this task | Concise; balanced; detailed |

Keep these dimensions independent. Knowing syntax does not imply understanding
an algorithm, and requesting reasoning help does not imply needing syntax
lessons. A reader can be experienced in programming while learning a new
language. Detail is independent of familiarity: syntax help can be one sentence,
and an experienced maintainer can request an extensive architectural explanation.

Treat known languages as possible bridges for explanation, not proof that
another language has equivalent semantics. Learning goals should influence
which source-grounded concepts receive attention; they are not evidence that
a framework or dependency exists in the code.

## Scenarios using the same source

Consider this pattern from `src/server.rs`:

```rust
let _generation = state.generation_lock.lock().await;
```

| Scenario | Requested help | Expected explanation |
| --- | --- | --- |
| New programmer learning Rust | Syntax and execution basics | Introduce `let`, the method call, `.await`, and a lock using a small concrete example. Explain the sequence before introducing terminology. |
| Experienced Python developer learning Rust | Rust syntax and semantics | Explain the lock guard, the binding's lifetime, and that `_generation` retains its value despite its underscore prefix. A brief Python context-manager comparison may help, with differences stated explicitly. |
| Rust developer unfamiliar with concurrency | Execution reasoning | Trace two requests: the first acquires the lock, the second waits, and dropping the guard permits the next request to proceed. |
| Rust developer evaluating the design | Architectural relationships and tradeoffs | Explain where this lock is shared, which operations its scope covers, and how serialization relates to the local GPU constraint, using available surrounding code and documented requirements. |
| Maintainer checking a change quickly | Concise change explanation | Describe the behavior introduced by the change and its scope; omit syntax lessons. |
| Rust developer encountering unfamiliar Git logic | Domain concepts | Explain the relevant revision or comparison behavior without reteaching Rust. This requires a section that actually contains that Git logic. |

These scenarios are evaluation cases, not mandatory personas or hard-coded
explanation templates. The same reader may need different assistance tomorrow
or in another file.

Architectural explanations need architectural evidence. A single lock statement
does not establish sharing across processes or all application entry points.
Explain the relationships visible in supplied context, identify what cannot be
established, and never compensate for missing context by inventing a design.

## Defaults and preference layers

The default application controls are `language_help = automatic`, `focus = balanced`,
and `detail = balanced`. `language_help = automatic` resolves by explicit language
membership: it explains languages listed in `learning_languages`, skips syntax
basics for languages in `known_languages`, and falls back to brief reminders when a
language appears in neither list. If a language is listed in both, `learning_languages`
takes precedence (`explain`). General programming experience does not infer
language fluency, and no onboarding wizard or learning profile is required before
the first explanation.

Resolve preferences from broad defaults to specific overrides:

1. Application defaults (`automatic` / `balanced` / `balanced`).
2. Saved user defaults (`[learning.defaults]`).
3. Opted-in repository defaults (`[learning.defaults]` in `.git/git-explain.toml`,
   active only when explicitly enabled via `--use-repository-learning` or the
   browser UI checkbox).
4. An explicitly selected learning profile (`[learning.profiles.<name>.preferences]`).
5. Language-specific preferences within that profile (`[learning.profiles.<name>.languages.<lang>]`).
6. Temporary session preferences (CLI launch flags such as `--language-help`,
   `--focus`, or `--detail`). Profile selection itself participates at layer 4.
7. Temporary section and view overrides (in-memory browser UI controls).

A learning profile is a collection of reader preferences (such as "Learning
Rust from Python") stored under the dedicated `[learning]` namespace. It is
strictly separate from a model endpoint profile under `[profiles.<name>]`.
The `[model]` section selects that endpoint profile, which defines the provider,
base URL, model, credential environment variable, and token settings.
Selecting or editing a learning profile never redirects network traffic or alters
model authentication.

The legacy `[reader]` configuration section is intentionally unsupported and
rejected with an error if encountered; legacy `experience` labels are not
migrated. All reader preferences belong to `[learning]` and its typed models.
Repository teaching configuration may suggest `[learning.defaults]` for
contributors, but cannot define personal learning profiles, grant permissions,
or expand existing endpoint and credential authority.

Unset values inherit; explicit values override. Browser controls provide reset
actions and show effective settings and their provenance. CLI launch flags
provide temporary overrides.
Temporary overrides disappear on process restart and must not silently rewrite
saved preferences.

## Reader controls

The core controls provide three independent dimensions:

- Language help: `automatic`, `explain`, `brief`, or `skip`. In the resolved
  policy (`ResolvedExplanationPolicy`), this always resolves to an explicit
  `explain`, `brief`, or `skip`.
- Explanation focus: `balanced`, `syntax`, `execution`, `reasoning`, or
  `architecture`.
- Detail: `concise`, `balanced`, or `detailed`.

Browser controls expose the active learning profile, allow inspecting effective
provenance, and provide an interactive editor for user defaults and named
profiles with explicit Save and Save as actions. Selecting tabs and editing or
saving settings never trigger model generation; tab navigation performs zero
network requests. Only the visible Generate or Regenerate button starts inference.

Readers can provide explicit feedback on concept annotations using "I know this already"
or "Explain this term" actions. Feedback adjusts section and view concept
overrides temporarily. The explicit Save feedback action records the chosen
category, updates the saved profile preferences while pruning the concept from
the opposite list, and cleans up view overrides. No implicit tracking, reading-time
heuristics, or automatic profile mutation occurs.

## Preserve the four explanation views

| View | Fixed responsibility | How preferences affect it |
| --- | --- | --- |
| Overview | What the complete code section does | Vocabulary and brevity; do not turn it into a tutorial |
| Changes | What changed in Git | Clarify unfamiliar terms while staying focused on the actual change |
| Concepts | Relevant knowledge needed to understand the supplied code | Prioritize knowledge gaps; connect each lesson to the code; allow zero lessons for familiar or trivial code |
| Deep | A more detailed teaching explanation | Adapt the walkthrough to syntax, execution, reasoning, or supported architectural questions |

File mode still omits Changes. A preference cannot turn one view into another,
expand deterministic annotation regions, or cause the model to review or
rewrite code. Context expansion follows a separate design from learning
preferences. The user has authorized testing additional source to improve
quality; the bounded [related-source experiment](related-source-context.md)
compares selected helper definitions with target-only context. Production
context collection remains unchanged until the experiment supports promotion.

## Implementation approach

Resolve configuration into one normalized effective explanation policy before
building a request. Keep preference resolution deterministic and outside
`RepositoryAnalyzer`; the analyzer remains responsible for Git and source
analysis, not mutable reader state.

Use that same policy consistently across:

- Prompt construction: explicit teaching instructions, not just a list of
  preferences. Preserve the view's purpose and reader adaptation on retries.
- Response validation: enforce schema, permitted annotation kinds and regions,
  enabled concept categories, and valid empty Concepts results. Validation can
  enforce structure; it cannot prove that prose teaches well or is correct.
- Request identity: include the normalized preferences that affect output,
  prompt/response versions, explanation kind, and existing source identity.
- Browser state: associate each result with the policy that generated it;
  never display a result for one policy as if it belonged to another.

Changing a preference should not trigger generation or erase unrelated view
results. Previously generated explanations remain readable with their
generation settings identified. The next explicit Generate action requests the
chosen policy. Regenerate replaces the exact variant. Deep explanations track
the exact prerequisite result identity including its policy (using the actual
Overview result in file mode and Changes in diff mode); if a prerequisite
explanation changes, invalidate dependent Deep state according to the existing
contract while leaving unrelated views intact. Structured Deep requires a
nonempty purpose and step-by-step walkthrough, source-supported optional
boundaries, and an optional clearly labeled illustrative example, rendering
with escaped accessible headings under cache contract `structured-response-v4`.

Keep factual grounding, deterministic source boundaries, local transport,
control-token requirements, and serialized inference fixed. These are not
personalization options. Continue to budget the complete serialized request
for the local GPU target; adding reader guidance must not bypass context checks.

## Implemented behavior and architecture

The working-tree implementation includes:

- **Typed policy and profiles**: `LearningPreferences`, `LearningProfile`,
  and `ResolvedExplanationPolicy` are fully implemented in `src/learning.rs`
  and strictly separated from model endpoint profiles.
- **Defaults and precedence resolution**: The default application controls
  are `automatic` / `balanced` / `balanced`. Resolution strictly follows the
  7-tier precedence chain (application defaults -> saved user defaults ->
  opted-in repository defaults -> selected learning profile -> profile
  language-specific overrides -> temporary session flags -> section/view
  overrides). Repository defaults require explicit opt-in (`--use-repository-learning`
  or browser checkbox) and cannot define profiles or credentials.
- **CLI controls**: Full management suite via `git explain learning` (`list`,
  `show`, `add`, `edit`, `use`, `remove`), an interactive numbered terminal
  wizard, and CLI flags (`--learning-profile`, `--no-learning-profile`,
  `--language-help`, `--focus`, `--detail`, `--use-repository-learning`).
- **Browser controls and provenance**: Profile selection dropdown, interactive
  in-browser profile and defaults editor, effective settings provenance display,
  reset to defaults, and explicit Save / Save as actions. Settings edits and
  saves never trigger generation.
- **User-only persistence and revision protection**: Dedicated `[learning]`
  TOML namespace in user configuration; writes preserve unrelated TOML
  sections; atomic file replacement with revision checks; concurrent stale
  saves return HTTP 409 Conflict; browser preserves unsaved editor drafts.
  The server binds to loopback; settings writes require JSON, same-origin
  validation, and a distinct per-session capability token.
- **Per-request and cache policy identity**: Every generation resolves one
  immutable policy. The policy is embedded in request identity, prompts,
  response validation, and cache keys. `RepositoryAnalyzer` remains pure Git
  and source analysis without learning state. Browser state keys results by
  section, kind, policy, and source; existing results remain readable after
  settings changes.
- **Structured Deep and exact prerequisite tracking**: Deep explanations
  track the exact prerequisite result identity including its resolved policy
  (Overview in file mode, Changes in diff mode). Prerequisite replacement
  invalidates only dependent Deep results. Deep prompts enforce structured
  output (nonempty purpose and walkthrough, source-supported optional boundaries,
  optional illustrative example) rendered with escaped headings under cache
  response contract `structured-response-v5`. Concepts rejects a title repeated
  as the entire lesson; schema validity alone does not establish teaching quality.
- **Explicit feedback movement and edited-label saving**:
  - *Opposite-list classification fix (Task 4)*: Clicking feedback explicitly
    records the chosen category (`known` vs `learning`) in section/view state,
    preserves existing effective concepts, and on save removes the editable
    label case-insensitively from the opposite list in profile preferences while
    adding it only to the target list and preserving unrelated entries already
    saved in that profile.
  - *Conflict draft preservation fix (Task 5)*: Temporary concept overrides
    and feedback categories are cleared only after a successful save. If save
    fails (e.g. concurrent HTTP 409 conflict), the category, overrides, and
    editable label remain intact in the browser so the user can retry without
    re-entering their feedback.
- **Verification status**:
  - Rust test suite passed (`task check` exit 0: 359 passed, 7 ignored),
    including queued-request policy isolation in file and direct diff modes.
  - Debug binary build passed (`task build` exit 0).
  - Playwright browser suite passed (`task test-browser`: 35 passed, exit 0),
    including `feedback-moves`, `conflict-draft`, and primary-added
    `reset-cancels-feedback` assertions.

## Follow-up correctness and integration work

1. **Inherited lists**: Every saved-list editor has an explicitly labeled
   Inherit checkbox. Saving unrelated controls preserves absent fields;
   unchecking with a blank input intentionally saves an empty list. Typing
   selects an explicit override. Save and Save as share this serialization.
   Chromium covers profile/language inheritance, clearing/restoring inheritance,
   keyboard operation, narrow screens, and absence of model calls.
   Save as clones the selected profile's base and language settings, applying
   the editor draft at its actual layer. It preserves explicitly inherited scalar
   choices and keeps other languages intact instead of flattening a language
   override into the profile base.
2. **Displayed versus pending settings**: When a readable result differs from
   the pending selection, its captured language-help, focus and detail appear
   under Displayed settings. Browser tests cover variant restoration and
   preference changes during generation without relabeling results.
3. **Typed save conflicts**: File-lock contention and stale revisions use
   explicit conflict errors mapped to HTTP 409. Validation errors remain 400,
   even when their text contains "changed". Tests hold an actual filesystem
   lock, verify unchanged configuration, release it and retry, including real
   HTTP requests in file, direct diff and daemon modes.
   The browser displays the server's distinct Busy retry and stale-revision
   reload guidance while retaining unsaved feedback for retry.
4. **Real lifecycle coverage**: Chromium exercises daemon settings and
   generation serialization across policies, process restart with saved profiles
   and two restored Overview/Deep variants, prerequisite replacement while a
   model response is held, and source refresh during an outstanding generation.
   Controlled local mock gates make races observable. Deep retains actual
   prerequisite identities; replacing one policy preserves another policy's
   Deep result. A Rust regression verifies that changing only a prerequisite
   result ID changes both cache namespaces even when its text is unchanged.
5. **Queued request isolation**: File and direct diff requests resolve their
   policy before waiting for serialized generation. Saving preferences during
   that wait cannot relabel the request. After acquiring the lock, Deep refreshes
   its actual prerequisite using that captured policy. Four deterministic Rust
   regressions cover both modes, preference changes while queued, and a new
   prerequisite becoming available during the wait.

Teaching-quality acceptance remains open. Mock-model lifecycle tests establish
transport and state behavior, not factual correctness of local-model output.

## Local-model evaluation: historical baseline and v6 findings

A qualitative review of local-model performance was conducted using existing
evaluation artifacts in `target/learning-evaluation/` (specifically `before.json`,
`after.json`, and `personalized-v6.json`; the primary review is recorded locally
in `target/learning-evaluation/agy-tasks/local-model-review.md`).
The original delegated tasks used those historical artifacts. Fresh follow-up
runs are recorded below; their output remains subject to qualitative review.

### Evaluation environment and constraints

- **Model and runtime**: `ocr-qwen36-35b` via saved profile `qwen35b` and local
  `llama.cpp` server.
- **Inference parameters**: Serialized requests, temperature 0, reasoning
  disabled.
- **Context budgeting and token caps**: Complete payload context budget capped
  at 16,384 tokens; normal output cap of 900 tokens; Deep output cap of 1,600
  tokens. Actual server runtime context remained unchanged.

### Baseline versus v6 progression

- **Historical baseline (`before.json`)**: Two legacy reader profiles evaluated
  over region-planning code in `src/runtime.rs`. Concept selections showed heavy
  overlap between beginner and experienced profiles, and Deep explanations were
  unstructured short summaries. Legacy reader configuration is now unsupported;
  this artifact serves purely as historical comparison evidence.
- **Intermediate iteration (`after.json`)**: Prompt updates increased syntax
  vocabulary but failed to establish independent policy-driven behavior.
- **Latest v6 matrix (`personalized-v6.json`)**: Evaluated 14 responses across
  7 scenarios (Concepts and Deep each). Structured Deep produced distinct
  purpose, walkthrough, boundary, and example sections, and concept annotations
  included titles.

### Structural validation versus factual correctness

All 14 responses in the v6 evaluation passed automated response validation
under `structured-response-v4`. However, automated response validation verifies
JSON structure, schema bounds, and permitted regions; it cannot evaluate factual
grounding or semantic accuracy.

### Qualitative failures observed in model outputs

Qualitative review of the 14 returned responses revealed several persistent
model limitations:

- **Wrong function attribution**: In `rust-reasoning` Concepts, the model
  misnames `bool::then_some` as `Option::then_some` and invents an unreachable
  zero-relative-start condition to justify subtraction underflow protection.
- **Familiar-basics leakage**: In `rust-execution` Concepts, the model explains
  `Arc` and `.await` despite `language_help = skip`. Restraint around familiar
  constructs remains inconsistent.
- **Incorrect response-flag interpretation**: In `architecture-boundaries` Deep,
  the model misinterprets a response boolean flag as a regeneration indicator
  when it is actually the cache-hit flag, and fails to acknowledge missing
  surrounding architectural evidence.
- **Incorrect JavaScript fallback claims**: In `javascript-rendering` Deep,
  the model falsely asserts that empty annotations leave HTML empty, ignoring
  that the trailing-source branch renders the source.
- **Weak syntax versus reasoning differentiation**: Syntax Deep frequently
  paraphrases procedural execution steps rather than explaining language syntax;
  conversely, reasoning variants do not consistently explain design motivations.

### Matrix coverage and consistency limitations

- **Misleading scenario naming**: The test scenario named `concise-default`
  explicitly requested Brief / Balanced / Concise controls. It did not evaluate
  the actual `automatic` / `balanced` / `balanced` application default.
- **Fresh default coverage**: `batch2-default-baseline.json` evaluates an
  unmodified learning selection on actual region-planning source. Concepts
  still misattributes `then_some` to `Option`; Deep correctly explains
  intersections and the full-source fallback.
- **Fresh familiar-code coverage**: `batch2-familiar-baseline.json` evaluates
  `normalize_language` with Rust and normalization concepts marked familiar.
  Concepts still teaches basic conversions, matching, and method chaining;
  the zero-annotation acceptance case has not passed.
- **Repeated evaluation**: `batch2-default-v7.json` contains three serialized
  repetitions per view with additional receiver and missing-callee fact-check
  guidance. All responses pass structural validation, but Concepts repeats
  the receiver errors and invents a reachable zero-relative-start case.
  This prompt iteration has not established a teaching-quality improvement.
- **Controlled experiments**: The opt-in harness accepts bounded repeat counts
  and explicit evaluation-only reasoning/output budgets. Reports record the
  effective settings; experiments do not change saved profiles or the model
  server's runtime allocation. The full scenario matrix still requires fresh
  acceptance and regression review.

These findings represent qualitative observations from an unaccepted evaluation.
No claim of model acceptance, overall project completion, or release status is
made; evaluation artifacts remain ignored under `target/learning-evaluation/`.

## Subsequent local-model iterations

The v9 familiar-code gap-selection task, with reasoning enabled and 6000 output
tokens, returned zero Concepts annotations in three temperature-0.6 repetitions.
This resolves that observed failure on those samples; it does not establish
acceptance for other source sections or policies.

The v11 architecture run used actual `generate_file_with_learning` source,
serialized Concepts, actual Overview, then Deep with the matching prerequisite.
Compared with v10, Deep correctly described generation-request serialization,
retained both cache-hit conditions, and avoided unsupported guarantees about
cache invalidation or source-file modifications. Its example still inferred
request-building success from a valid identifier although the helper body was
absent. Explicit acknowledgment of missing helper evidence was also absent.
The candidate remains unaccepted pending corrections, repeated cases and the
remaining scenario matrix. See [prompt evaluation](prompt-evaluation.md) for
settings and review criteria.

Antigravity successfully executed the noninteractive local-model evaluation and
produced artifacts and a preliminary report. Primary review corrected its overly
generous example assessment and a harness filename transcription error using
the recorded executable hash. Delegated execution does not replace independent
quality review. These results describe the recorded source identity before the
subsequent queued-policy fix; future server-source evaluations must capture the
new source identity.
