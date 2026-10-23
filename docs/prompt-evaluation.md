# Personalized explanation prompt evaluation

The objective is accurate assistance matched to an explicitly resolved learning
policy. Syntax familiarity, reasoning needs, architectural interests, and desired
detail are independent. More text is not evidence of better teaching.

## Prompt structure

`src/model/prompts.rs` owns reusable application rules, reader guidance and
per-view tasks. `openai.rs` assembles requests, budgets complete payloads and
validates responses. Keep application rules separate from reference data:

1. System rules establish accuracy, source boundaries, JSON output, and the
   application-controlled language-help instruction derived from a typed enum.
   Never interpolate editable concept names or profile text into system rules.
2. Each explanation kind has its own task. Overview summarizes; Changes explains
   Git; Concepts selects useful knowledge gaps and permits zero lessons; Deep
   explains purpose, causal steps, supported boundaries, and an optional example.
3. The resolved policy is reference data. Known lists constrain repetition;
   learning lists identify goals. Neither establishes repository dependencies.
4. The supplied code section, deterministic regions, Git context and actual prior
   result are evidence. The default evaluates the target alone. The explicitly
   authorized [related-source experiment](related-source-context.md) adds a
   bounded, identified selection of actual helpers for a paired comparison;
   it does not change target annotation regions or production context collection.
5. JSON schema and application validation enforce structure, categories, region
   identity and bounds. They cannot certify factual correctness or teaching value.

Retain task, personalization and evidence boundaries on retry. Simplify optional
wording rather than dropping constraints. Model output is always untrusted text.

## Evaluation protocol

Use the ignored `tests/learning_quality.rs` harness with actual repository source
read at runtime and the verified loopback `qwen35b` profile. Serialize requests.
Record unit content SHA256, revision, selection, resolved policy/ID, prompt version,
generation settings, latency, outputs and validation failures under the ignored
`target/learning-evaluation/` directory.

Deep evaluations generate Overview first by default, under the same immutable
policy, and use that actual response as the prerequisite. Reports record its
generation identity, output and latency before starting Deep; a policy mismatch
is rejected. Set `GIT_EXPLAIN_LEARNING_WITH_PREREQUISITE=false` only for an explicitly
labeled isolated-prompt experiment. Protocol `source-learning-v2` records this
mode and the evaluation executable hash to distinguish frozen candidates.

Change one factor at a time when diagnosing a failure: task wording, policy
placement, reasoning mode, or output budget. Compare the same source and policy.
State which factors changed; do not attribute a configuration improvement to
prompt wording alone. Repeat promising cases, then rerun successful cases and
the scenario matrix to detect regressions.

The matrix includes Python-to-Rust syntax assistance, familiar Rust execution,
Rust reasoning on the same region planner, architecture with missing evidence,
Git comparisons, JavaScript rendering, true application defaults, and familiar
normalization code. Inspect these criteria manually:

- Correct method receivers, branches, coordinates and source behavior.
- Useful syntax teaching without assuming reasoning difficulty.
- Causal reasoning without definitions of familiar basics.
- Restrained architecture claims with missing evidence acknowledged.
- Empty Concepts for routine, familiar code when no useful gap remains.
- Concrete Deep walkthroughs and examples that follow actual source conditions.

Do not use response length, exact wording or structural pass rates as quality
scores. Do not add function-specific factual hints merely to pass a scenario.

## Configuration experiments

The harness defaults to temperature 0, reasoning disabled, 900 Concepts / 1600 Deep
output tokens and a 16384-token complete-payload context budget. Explicit experiment
variables permit reasoning, bounded output budgets and 1–5 repeats. Effective
settings are recorded in each report. Budgets remain authoritative: requested
detail cannot bypass them. Saved profiles and llama.cpp runtime allocation are
unchanged by evaluation settings.

The harness also accepts an explicit bounded temperature for controlled sampling
comparisons. Qwen's [published model guidance](https://huggingface.co/Qwen/Qwen3.6-35B-A3B)
recommends temperature 0.6 for precise coding tasks in thinking mode, with
top-p 0.95, top-k 20, min-p 0 and presence penalty 0. The verified local preset
already supplies those other values. Compare the temperature change separately
from prompt changes and repeat stochastic cases before choosing a saved profile.

Observed results so far: adding fact-check wording alone did not resolve receiver
errors. Reasoning with 3000 output tokens exhausted the cap twice; reasoning
with 6000 corrected the Boolean receiver in three repeated default-policy runs,
but still needs review of checked-slice claims. Familiar-code basics persisted
with reasoning enabled. The v9 gap-selection task subsequently produced zero
annotations on familiar normalization code in three temperature-0.6 repetitions;
this is evidence for that scenario, not acceptance of the complete matrix.

The v11 evidence-calibration examples improved one architecture sample over v10:
Deep described serialization of generation requests without inventing source-file
modifications, stated both cache-hit conditions and avoided blanket invalidation
guarantees. Its example still incorrectly implied that a valid identifier ensures
an opaque request-building helper succeeds, and it did not acknowledge missing
helper implementation context. Repeated and cross-scenario acceptance remains
incomplete. These runs used actual Overview prerequisites, reasoning enabled,
6000/8000 normal/Deep output limits, temperature 0.6 and serialized inference.

Antigravity successfully ran the bounded evaluation and wrote a preliminary
assessment. Primary review found its example grade too generous and corrected a
report filename against the executable hash. Use the delegate for execution and
initial review; independently verify factual claims and report metadata. See the
design document and ignored batch2 model reviews for the underlying evidence.

The v11 Python-to-Rust run improved Boolean receiver accuracy and explained
concrete syntax with a consistent example. Deep still incorrectly claimed that
capping a slice's upper bound guarantees success, without proving that its lower
bound remains valid. Both primary and delegated review found the defect. The v12
candidate adds a general rule requiring endpoint ordering and bounds to be proved
together, with metadata consistency assumptions stated. Its first live syntax
comparison removed that upper-bound claim, but Concepts merely copied the summary
and Deep incorrectly attributed underflow prevention to subtracting one from one.
It also blurred the fallback branch with a `join` call that branch does not execute.
The candidate remains unaccepted. Unit checks prove delivery of instructions, not
model compliance; the broader matrix and consistency checks remain incomplete.

Promote a prompt or model-profile change only after reviewing its outputs across
the relevant scenarios, including repeats and regression cases. Bump prompt/cache
identity for changed prompt semantics. Document remaining model limitations
explicitly; never weaken correctness requirements to obtain a green evaluation.

## Current qualification matrix

This tracks manual teaching-quality acceptance, separately from passing automated
checks. Earlier candidates do not establish acceptance of the current candidate.
v13 removes duplicated source text for a region covering the complete target:
its existing ID/bounds reference the source supplied above. Partial regions keep
their exact evidence. This saves input capacity without enlarging the source
scope; live qualification of the revised formatting is still required. The paired
related-source experiment remains frozen on v12 to isolate its context comparison.
The subsequent v14 candidate retains that deduplication and strengthens only
Architecture-focus guidance: organize around responsibilities, state ownership
and data flow, connect helper evidence to target calls, and acknowledge missing
implementations. Language-help controls remain independent. Its paired study is
separate from v12; changes between those versions include both formatting and
architecture guidance, so effects cannot be attributed to either factor alone.

| Scenario | Latest reviewed evidence | Remaining work |
| --- | --- | --- |
| Application defaults | Earlier reasoning experiments fixed the Boolean receiver but retained checked-slice errors | Fresh current-candidate Concepts/Deep and repeats |
| Familiar normalization | v9 returned zero Concepts annotations in three temperature-0.6 repetitions | Requalify the current candidate and inspect Deep |
| Python learning Rust syntax | v12 improved one boundary but Concepts copied its summary and Deep invented underflow prevention at one | Correct, rerun and repeat |
| Rust execution | Earlier samples overclaimed behavior of unseen helpers | Fresh current-candidate review |
| Rust reasoning | v12 used inconsistent example metadata and omitted the final selected line in its slice example | Correct, rerun and repeat |
| Architecture boundaries | v11 reduced unsupported helper claims but retained an unsupported example assumption | Current target-only versus related-source study, then repeated review |
| Concise balanced help | This is an explicit override, not the application default | Fresh current-candidate review |
| Git comparison domain help | Actual `range_analysis` case is available in the harness | Fresh current-candidate review |
| JavaScript rendering | Earlier Deep falsely described the empty-annotation path | Fresh current-candidate review and regression repeat |

Related-source protocol `source-learning-v3` records the explicit mode and exact
supporting-source manifest/identity with every result and prerequisite. It is
currently an opt-in architecture experiment. The default remains target-only.
