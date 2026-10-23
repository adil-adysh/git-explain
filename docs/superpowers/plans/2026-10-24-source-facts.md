# Tree-sitter Source Facts Implementation Plan

> **For agentic workers:** Implement the approved evidence-layer design through the explicitly requested agy delegation. Primary owns review, live evaluation and installation.

**Goal:** Add bounded, revision-specific syntactic evidence to existing explanation requests without additional model requests.

**Architecture:** Pure extraction in `src/language/` produces typed source facts and exact spans. File/diff request construction selects evidence from the active revision; the model adapter distinguishes selected-region facts from surrounding context. Facts describe syntax, never infer unseen helper behavior or custom types.

**Tech Stack:** Existing Rust and tree-sitter grammars; no new dependencies or compiler/LSP services.

**Spec:** The approved design in this chat: deterministic source → typed facts/unknowns → existing request → existing quote validation/shared reader.

## Global Constraints

- Preserve all preexisting dirty work, explanation UX contract, loopback transport and control tokens.
- One explicit generation per action; no navigation inference, automatic retries or prerequisite requests.
- Keep source facts independent of diff classification and mutable/model concerns.
- Deleted uses before source; modified/added uses active after source; file mode uses its selected revision.
- Bound evidence by count and text budget; omit uncertain/error-affected facts; unknown is not a factual assertion.
- No new response fields, UI panels or external tools. Cache identity includes evidence content/version.
- User clarification: support all nine registered languages: Rust, Python, Go, Java, C#, JavaScript, TypeScript, C++, C. Each requires meaningful extraction tests for applicable comparisons, calls, returns and explicit type/assertion syntax; empty fallback alone does not count as support. Include TSX/JSX grammar handling where the existing project accepts those extensions. Unsupported syntax shapes remain conservative omissions, explicitly documented.
- No commits, model/config changes or installs by agy. Primary performs final checks and installation.

## Review Focus

- Exact >= versus >, multiline operands and UTF-8 spans.
- Macro assertions: preserve invocation syntax without pretending macro expansion was analyzed.
- Free calls versus member-call syntax; unresolved names must not imply side effects, return types or symbol resolution.
- Malformed and partial fragments; unsupported-language fallback must preserve generation.
- Deleted/partial diff targets: surrounding facts must never become selectable annotation regions.

## Tasks

1. **Extraction** — Add `src/language/facts.rs`, export it from language/mod.rs. Define serializable typed facts, source spans and an evidence version; deterministic extractor accepts language/source. Cover comparisons, calls, assertion syntax, explicit type syntax and returns/tail expressions conservatively. First add and observe a failing regression, then implement. Test bounds, UTF-8, malformed input, unresolved helper names and deterministic ordering.
2. **Integration** — Carry evidence in ExplanationRequest (mechanically initialize test constructors), populate file/diff request builders from exact active source. Put pure extraction behind a reusable interface; do not make RepositoryAnalyzer depend on model/runtime. Serialize bounded selected/context evidence into every view including Deep; version/cache evidence. Observe a failing request/cache regression before implementation; verify deleted and file-mode parity.
3. **Review** — agy reports exact edits, RED/GREEN commands, real exit codes and limitations. Primary compares against task-start snapshots, checks language semantics and fallback, and runs full task check and browser regressions.
4. **Live validation and ship** — Freeze old installed binary and candidate. Reuse five-case evaluator serially, independently review actual prose/target accuracy and request counts. Update implementation task file, run task install and verify installed daemon source/model behavior. Report remaining quality limits honestly.

## Acceptance

- [x] agy extraction/integration reviewed against initial snapshot
- [x] Meaningful observed RED and focused GREEN evidence
- [x] Exact facts/spans, budgets, unknowns, fallback and revision selection verified
- [x] Meaningful grammar-specific extraction tests pass for all nine supported languages
- [x] Existing request/response/UI contracts preserved; cache identity updated
- [x] Full regression and browser checks pass
- [x] Fresh live comparison reviewed, one generation per action
- [x] Task file updated, frontend/release rebuilt and installed behavior verified

Primary review repaired draft defects with observed failing regressions: multiline type provenance, assertion behavior guessed from function names, TypeScript return annotations confused with binding types, stale language/policy evidence and prompt/cache disagreement, missing literal tails, reconstructed pointer calls/macros, pointer/reference declaration reconstruction, generator/tagged-template argument counting, and TypeScript-versus-TSX grammar selection. Independent read-only review confirmed the declaration/counting repairs. `check-final.log` under the ignored task artifact directory records the passing full check and 14 independent contract tests; `browser.log` records 85 passing tests. Live results, remaining teaching limits and installation proof are recorded in `report.md`, `mechanical-summary.json`, `installed-verification.json` and `installed-concepts.json` there. All nine languages are covered by grammar tests; the representative live teaching comparison is Rust-only.

Extraction remains a pure reusable language helper invoked by request builders, rather than adding model-related state to SemanticDocument or RepositoryAnalyzer. Source spans are unit-relative and hash-bound. The source-facts policy is `source-evidence-v1`, prompt `explanation-prompt-v20`, response shape `structured-response-v9`. Limits: 32 facts, 200 bytes per rendered fact summary, 2048 bytes of summaries total; region labels/header add bounded overhead. Conservative omissions remain intentional, including unsupported node shapes and facts excluded by these budgets. Calls, macros, declarations and returns preserve complete syntax when included.
