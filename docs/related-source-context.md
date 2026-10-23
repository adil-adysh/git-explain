# Related source context: measured architecture experiment

The user authorized additional source and architectural changes when they improve explanation quality. This supersedes the earlier restriction on extra source for this experiment. The product still generates only on explicit actions, serializes inference, retains target annotation boundaries, and respects complete-payload budgets.

## Question and design

Does directly relevant implementation evidence improve architecture explanations compared with a target function alone? Compare two runs of actual `src/server.rs::generate_file_with_learning` under the same Architecture/Skip/Detailed policy, frozen prompt v12, model preset, context window and generation settings. Both generate their own actual Overview prerequisite and then Deep. The paired runs use one frozen executable and identical target source content. Only supporting context changes.

Related-source mode includes six explicitly selected actual definitions: file_request, file_prerequisite, LearningRuntime.prepare, restore, remember, and LearningResult.response. These cover request construction, prerequisite policy, result restoration, recording, and response metadata. They are evidence, not inferred contracts. Other callees remain unknown. This manual selection is an evaluation probe, not a production call resolver.

The harness extracts definitions at runtime using the existing parser, records exact paths, symbols, line locations and content hashes, and attaches source as reference context. It preserves the target request source and annotation regions. Whole definitions must fit a 12000-byte experiment limit; nothing is cut mid-function. The provider's existing complete-payload token budget remains authoritative and may reject an oversized request before inference.

Evaluate grounded relationships, admission-versus-execution ordering, serialization scope, actual cache-hit conditions, prerequisite identity, response flag meaning, and acknowledgment of still-unknown helpers. Reject invented source modifications or unseen helper guarantees. Correctness and requested help matter; text length is not a score. Record overhead and limitations without treating a single stochastic sample as causal proof.

## Conditional production direction

If the paired study shows useful improvement and repeated cases hold up, introduce an immutable context manifest outside RepositoryAnalyzer: exact target revision and hash, selected related definitions with identities and reasons, omitted/ambiguous dependencies, and a content identity covering the entire manifest. Build it from the same file/Git revision as the target, never mix a revision with current working-tree helpers. Use bounded direct dependencies and type definitions; unresolved names stay unresolved. Avoid indiscriminate repository dumps.

Prompts consume a distinct supporting-evidence field while annotations remain target-only. Cache/result identity must include that context manifest; changing a helper must invalidate its dependent result even if the target is unchanged. Admission captures preferences and source/context together. Prefer whole-definition selection within the existing token budget, preserving target, policy and actual prerequisite over optional helpers. Save/config edits and navigation still make no model request.

A production implementation needs regressions for deterministic selection, ambiguous symbols, revision consistency, content-specific identities, budget omissions, request isolation, target-only annotations and all three server modes. The current experiment changes neither production context collection nor cache identity; promotion remains contingent on measured evidence.

## First paired result

The frozen v12 pair completed against identical target source, policy, model and
generation settings. Each Deep used its own actual Overview and the matching
supporting-source identity. Related context added 1604 actual prompt tokens per
call; Deep's conservative input estimate increased from 4037 to 5952 and still
fit the unchanged context budget. Both runs passed response validation.

Primary review found limited factual gains: related context correctly placed
prerequisite refresh before key construction and identified `remember` as an
in-memory insertion. It did not establish an overall quality improvement: the
related response taught Result/Option/await syntax despite Skip/Architecture
preferences and scarcely explained the supplied policy/prerequisite relationship.
The delegate's report credited cache-tier details that were present in source but
absent from the response; primary review rejected that assessment. Available
evidence must not be counted as evidence actually used in an explanation.

This N=1 study does not justify promoting the six-definition bundle. Next probes
must improve architecture organization and context selection, then repeat the
comparison. v13 independently removes duplicated whole-target region text to
free input capacity; the paired study above stayed on its frozen v12 executable.
