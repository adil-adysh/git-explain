# Infrastructure and Kotlin Language Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Implementation is delegated to agy; primary reviews and integrates each task.

**Goal:** Support Terraform/HCL, YAML, JSON/JSONC, Dockerfile, Bash, PowerShell and Kotlin throughout file/diff discovery, source sections, syntactic evidence and explanation annotations.

**Architecture:** Reuse the existing deterministic analysis and explanation pipeline. Centralize grammar selection so section extraction, annotation partitioning and evidence cannot disagree. Add format-aware section extraction and bounded syntax facts without executing input or inferring cloud/schema/runtime behavior.

**Tech Stack:** Rust stable, Tree-sitter core 0.25 (ABI 14 and 15), existing Svelte workspace and test harnesses. Candidate grammar versions verified from published sources: hcl 1.1.0, yaml 0.7.2, json 0.24.8, containerfile 0.9.2, bash 0.25.1, powershell 0.26.4, kotlin-ng 1.1.0.

**Spec:** User-selected infrastructure-first scope plus Kotlin; repository AGENTS.md and docs/explanation-ux.md.

## Global Constraints

- Preserve existing work committed as ebea755, 5427355 and 0199705. No push.
- Keep inference serialized for the local GPU; Generate/Regenerate are the only generation triggers.
- Preserve independent Overview, Changes, Concepts and Deep states/cache identities; file mode omits Changes.
- Render one shared source view per code section. The model may cite only application-supplied regions.
- Diff Changes/Concepts use changed regions with the full section as context; file Overview/Concepts use the full section.
- Retain loopback-only transport and control tokens. Never execute configuration or shell input.
- Facts prove visible syntax only: no resource validity, deployment behavior, command effects, inferred types or unseen-helper claims.
- Keep evidence bounded at 32 facts, 200 bytes per summary and 2048 bytes total; preserve exact original byte/line provenance.
- Primary reviews agy's actual diff and command output before authorizing focused commits. No concurrent edits or overlapping builds/model runs.

## Review Focus

- Extensionless Dockerfile and compound suffixes must appear consistently in file and diff modes.
- Multiline literals, YAML block scalars and shell/Docker heredocs must remain intact when selecting sections and partitioning annotations.
- JSONC trailing commas must not shift source offsets or silently permit JSON5 constructs.
- Nested declarations/configuration and deleted-only changes must select useful sections without duplicating or miscounting unchanged sections.
- All nine existing languages must still parse and produce facts after the Tree-sitter upgrade; Kotlin extension/suspend/expression-body functions need explicit coverage.

## File Structure and Detection

- Cargo.toml/Cargo.lock: compatible runtime and pinned reviewed grammars.
- src/language/mod.rs and new grammar.rs: centralized path/language/grammar mapping; analyzers consume the same identity.
- New src/language/infra.rs: format-aware configuration and script section extraction; split into focused modules only if needed for clarity.
- New src/language/kotlin.rs: Kotlin declarations and script top-level source.
- src/language/facts.rs and a focused infrastructure facts module if needed: exact structural syntax evidence.
- src/runtime.rs: consume shared grammar selection and format-aware deterministic region boundaries.
- src/file_analysis.rs, src/semantic.rs, src/workspace.rs, src/web.rs: adjust only if registry integration exposes actual gaps.
- frontend/src and src/learning.rs: update hardcoded supported-language displays only where present; retain free-form known-language preferences.
- tests/infra_languages.rs, tests/source_facts_contract.rs and relevant HTTP/browser fixtures: end-to-end and regression contracts.
- README.md, docs/explanation-ux.md and this plan: supported formats, factual limits, validation evidence.

Path rules: .tf/.tfvars/.hcl -> HCL; .yaml/.yml -> YAML; .json -> JSON (including .tf.json); .jsonc -> JSONC; Dockerfile/Containerfile and their dotted variants -> Dockerfile; .sh/.bash -> Bash; .ps1/.psm1/.psd1 -> PowerShell; .kt/.kts -> Kotlin. Do not infer arbitrary extensionless files from a shebang. Filename and extension handling must be tested on Windows paths.

## Task 1: Grammar, discovery and section extraction

**Interfaces:** Preserve LanguageRegistry::language_for_path, analyzer_for_path and find_changed_units. Add a shared grammar_for_language(language: &str) -> Option<tree_sitter::Language> in src/language/grammar.rs. Preserve TSX fallback. Parsers consume original text except the documented JSONC normalization below.

- [ ] Add failing tests in tests/infra_languages.rs for every path rule and meaningful source sections in file/diff extraction. Observe failures before production changes.
- [ ] Upgrade Tree-sitter core minimally to 0.25, add reviewed grammar versions and verify set_language succeeds for every existing/new grammar without unsafe compatibility bridges.
- [ ] Implement analyzers using actual grammar node fields. HCL: top-level blocks/attributes with labels. YAML: documents and coherent top-level mapping entries, keeping each list/block scalar intact. JSON/JSONC: top-level members or complete root arrays/scalars. Dockerfile: complete instructions, preserving continuation/heredoc. Bash/PowerShell: functions plus coherent top-level statements. Kotlin: declarations, extension/suspend functions, properties and script statements.
- [ ] Prevent generic brace/declaration fallback from inventing sections for these formats; malformed content may remain a coarse original-source section, with no fabricated facts.
- [ ] JSONC: permit comments and trailing commas using a string/comment-aware, length-preserving replacement of only trailing separator commas with ASCII space before parsing. All source slices and hashes use original bytes. Strict JSON must not inherit JSONC-only acceptance. Reject JSON5-only constructs for fine analysis. Test escaped strings, URL/comment text, Unicode, comments between comma and closing delimiter, nested arrays and malformed input.
- [ ] Assert sections have nonempty exact source, stable names/ranges, no duplicates and correct changed/deleted intersections. Include all five Review Focus conditions assigned to extraction.
- [ ] Run cargo test --test infra_languages and existing parser/document/diff tests; inspect outputs and diff. Primary authorizes commit feat: add infrastructure and Kotlin parsing after review.

## Task 2: Evidence and deterministic annotation regions

**Interfaces:** Preserve SourceEvidence::extract(language, source), request.source_evidence() and annotation_regions(unit, changed_ranges). Introduce a structural syntax fact containing span, actual node kind and original syntax (or narrow typed variants) rather than treating data members as runtime calls. Reuse SourceSpan and evidence bounds.

- [ ] Add failing contract tests for useful nonempty evidence in all seven families, exact original byte/line spans and distinct deterministic region references.
- [ ] Extract HCL block/attribute, YAML mapping entry, JSON member, Docker instruction, shell command/function/pipeline syntax, PowerShell command/function/pipeline syntax and Kotlin explicit declaration/comparison/call/return syntax where unambiguous. Do not apply generic function arity to shell commands, splats or pipelines.
- [ ] Partition at complete format-aware statement/member/instruction boundaries; never split strings, block scalars, interpolations or heredocs. On parse errors retain safe coarse regions. Preserve supplied changed-region boundaries and full section context.
- [ ] Bump evidence/cache prompt identities when emitted evidence semantics change. Confirm prerequisite and request-kind independence is preserved.
- [ ] Test YAML anchors/tags/multidoc, HCL nested interpolation, Docker heredocs, Bash heredocs, PowerShell here-strings/pipelines and Kotlin nullable/extension/expression-body examples. Test malformed input and absent facts rather than fabricated semantics.
- [ ] Run focused contracts, runtime region tests and model HTTP tests. Primary reviews quotes and region contents before commit feat: ground infrastructure and Kotlin explanations.

## Task 3: End-to-end integration, documentation and validation

**Interfaces:** Existing explicit generation HTTP/UI flows and shared source reader; do not add a parallel explanation path.

- [ ] Add fixture repositories and HTTP/browser cases covering discovery, file/diff counts, changed default, selection and generation requests for all new language families. Assert navigation makes no model request, one reader per section, exact supported regions, independent request kind/cache state and deleted-source context.
- [ ] Update actual hardcoded support lists, README format table and factual limitations. Explain that Kubernetes/CloudFormation YAML is parsed as syntax, Helm/Jinja rendering and cloud/schema validation are outside scope.
- [ ] Run task check and relevant browser suite serially. Review grammar dependency licenses and lockfile scope. Update this checklist with actual commands/results.
- [ ] Freeze a freshly built evaluation binary in ignored artifacts. Run one fresh serialized Concepts evaluation per new language family with raw request/response/settings/source identities and usage. Include one configuration boundary counterexample and one Kotlin helper/nullable counterexample; primary grades actual prose independently of JSON validity. No duplicate requests to repair prose silently. Report unresolved limitations.
- [ ] Update task/install documentation only if necessary; task install must rebuild frontend and release executable. Run task install, verify built/installed hashes and loopback daemon health; perform an explicit-generation installed smoke test.
- [ ] Primary reviews final diff and evidence. Have agy create focused test/docs/integration commits (no binaries or target artifacts); verify clean working tree and report commit hashes, supported scope and real live validation limits.

## Execution Record

- 2026-10-24: User approved the plan, including Kotlin, and retained agy implementation with primary review.
- Existing changes committed by agy before expansion: ebea755, 5427355, 0199705; guarded runner verified unchanged working-file hashes and clean tracked worktree.
- Primary inspected published grammar source/manifests and confirmed ABI requirements and JSON comment/trailing-comma behavior. Licenses: HCL Apache-2.0; other selected grammar crates MIT.
- Task 1 delegated to agy conversation 23fe560e-69ab-476f-9f2b-ac578fd878f7 with restricted edit paths, exact verification commands, no commits/configuration changes/model requests and mandatory observed RED/GREEN results.
- Optional read-only grammar audit was stopped after sufficient inspection because its discovery cost exceeded its value; no audit completion claim. Primary retains grammar research artifacts for review.
- Local llama.cpp health endpoint returned HTTP 200 before implementation validation; no generation requested at this point.

- Parser review: agy observed RED (11 failures), then 13 parser tests and 186 library tests passed. Its CLI timed out before a final report; SUCCESS status was not treated as completion. Primary independently reproduced five failures plus YAML sequence identity, fixed them, and verified 20 parser/review tests and 14 legacy facts contracts pass. New facts/annotation work remains pending.
