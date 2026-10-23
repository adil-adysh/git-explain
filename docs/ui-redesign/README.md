# Svelte 5 accessible workspace: specification index

> **Status:** Proposed design, 2026-10-23. **Not implemented.** This is a reviewable frontend migration contract, not a claim about current UI behavior.
>
> **Scope:** `git explain file`, `git explain diff`, and daemon-backed repository sessions. Preserve CLI/debug workflows.

## Objective and north star

Replace the current eager, server-rendered collection of every source section with a **fast, keyboard- and screen-reader-first code-learning workspace**. Users should be able to answer three questions with no guesswork:

1. **Where am I?** Repository/revision → selected file → selected code section.
2. **What am I looking at?** Source/diff and its current explanation view.
3. **What will this action do?** Navigation and settings never initiate model inference; only an explicit Generate/Regenerate action does.

**Primary mental model:** **Choose a file → see all code sections in that file → select one → read code → choose an explanation view.** A user should not have to traverse the full source of other files to do this.

## Navigation map

```text
Git Explain
├── Explore                         (top-level destination/link)
│   ├── Repository/revision status
│   ├── Explorer                     (navigation landmark)
│   │   ├── Files                    (choose one; paths and change labels)
│   │   └── Code sections            (ALL discovered sections for chosen file,
│   │                                  lightweight metadata, ordered by source)
│   │       └── Search/filter
│   └── Reader                       (main landmark; one selected section)
│       ├── Section identity and Previous/Next section
│       ├── Source and, when relevant, Git diff
│       ├── Explanation views        (real ARIA tab widget)
│       │   ├── Overview
│       │   ├── Changes              (diff mode only)
│       │   ├── Concepts
│       │   └── Deep
│       ├── Explicit Generate/Regenerate and explanation status
│       └── Customize this section   (local override disclosure)
│           ├── Whole-section override (NEW backend support)
│           └── Current-view override (existing capability, relocated)
├── Personalization                 (top-level destination/link)
│   ├── Active learning profile / session preferences
│   ├── Saved user defaults and learning profile editor
│   ├── Language overrides and concept lists
│   └── Repository defaults opt-in / feedback management
└── Settings                        (top-level destination/link)
    ├── Appearance, code display, accessibility / shortcuts
    ├── Model endpoint/profile information
    └── Diagnostics and advanced runtime settings
```

**Top-level destinations are links, not ARIA tabs.** They must work with browser history and have an active-location indicator such as `aria-current="page"`. **Overview / Changes / Concepts / Deep are ARIA tabs** within the current Reader. No Previous/Next *explanation* buttons: explanations are non-linear choices.

**Previous/Next section** buttons are secondary to direct Explorer selection and traverse the **visible, filtered list within the current file**. They stop at the file boundary; moving to another file is an explicit Explorer action. Label destination and position so this boundary is predictable. This is a proposed decision; test during usability review before implementation.

## Core requirements and hard constraints

| ID | Requirement |
| --- | --- |
| UX-01 | File selection reveals the complete *metadata list* of its discovered code sections, including unchanged sections when available; source bodies are not bulk fetched |
| UX-02 | Explorer + one Reader is the default desktop organization; narrow screens expose both in a logical sequence |
| UX-03 | Direct section selection, optional section Previous/Next, and browser Back/Forward are reliable; no next/previous explanation controls |
| UX-04 | Explicit Generate/Regenerate is the **only** model-inference trigger, including when switching files, views, profiles, settings, or refreshing index data |
| UX-05 | Personalization and Settings are distinct top-level destinations; local section/view customization stays in the Reader |
| UX-06 | Source and all controls usable without a mouse, with NVDA and Chromium/Edge, at zoom and high contrast |
| UX-07 | Current explanation's generating policy is distinguishable from pending preferences; cached variants cannot be mislabeled |
| UX-08 | Initial document is a small app shell. Selected source, cached results, and policy resolution are lazy and scoped; no `sections × views` startup requests |
| UX-09 | Git snapshot, model, policy, prerequisite and source identities remain authoritative on the Rust side |
| UX-10 | All currently supported learning, feedback, diff, source, explanation, CLI and daemon workflows have a tested migration path |
| UX-11 | UI must support light, dark, forced-colors, reduced motion, text resize, robust focus outlines, and non-color status labels |
| UX-12 | Empty, loading, generating, failed, offline, stale and conflict states are explicit, accessible and recoverable |

## Document set

- [Screens and states](screens.md): each destination, subview, disclosure, empty/error state, commands, and examples.
- [Interaction and accessibility contract](interaction-accessibility.md): focus behavior, keyboard matrix, screen-reader announcements, URL semantics, navigation ordering.
- [Architecture and API contracts](architecture-api.md): Svelte components/stores, Rust changes, metadata/source APIs, cache, security and response races.
- [Phased implementation plan](implementation-plan.md): independently reviewable milestones, dependencies, rollouts, exit gates and risks.
- [Validation and acceptance matrix](test-plan.md): measurable performance/accessibility/feature-parity scenarios.

Use these files together. A screen is not complete unless it satisfies the global interaction contract and is covered by the test matrix.

## Existing code and contract references

This is an **evolution** of the existing project; do not blindly replace established behavior:

- `src/web.rs`, `src/web.js`, `src/web.css`: current monolithic server HTML and browser interactions.
- `src/server.rs`: standalone diff/file web routes. `/api/changes` supplies change metadata, while initial GET `/` currently builds a large HTML page and restores many variants.
- `src/daemon.rs`: repository sessions, snapshot polling and diff JSON.
- `docs/explanation-ux.md`: **authoritative** explanation-view purpose, source/annotation boundaries, on-demand behavior. Resolve any proposed discrepancy before implementation.
- `docs/personalized-learning-design.md` and `docs/learning-usage.md`: learning precedence, temporary overrides, explicit feedback save and cache identities.
- `docs/prompt-evaluation.md`: structural model tests are not proof of factual accuracy.

**Current vs new:**

| Capability | Today | Proposed work |
| --- | --- | --- |
| Overview/Changes/Concepts/Deep + explicit inference | Exists | Preserve exactly; migrate to components |
| Per-section **per-view** temporary learning overrides | Exists in browser | Preserve, move to contextual Reader disclosure |
| Whole-section shared learning override | Not a proven existing backend scope | Implement explicitly and test precedence |
| Complete index of every section for chosen file in diff mode | Do not assume `ChangeSet` contains all unchanged sections | Extend analysis/index projection and test Git before/after semantics |
| File mode lightweight index/source endpoints | Initial HTML render, source embedded | Add metadata and individual source fetch |
| Personalization editor | Embedded in page | Move to separate destination with safe draft editing |
| Model config | Mainly CLI/TOML, model profile chosen on launch | Initially show runtime info + documented CLI actions; editable browser model config is a separate capability/security decision |

## Design decisions and deferred questions

**Decided for the first release:** Svelte 5 runes + TypeScript + Vite; static assets bundled/served by Rust without a runtime Node server; native semantic controls first; no heavyweight code editor, automatic inference, mandatory virtualization or third-party UI component framework; one live status outlet; stable URL for selection; local scope defaults temporary.

**Defer until baseline and user validation:** continuous reading of several source sections, cross-file Next/Previous, user-configurable hotkeys, source prefetch, long-file chunking, resize handles, model switching for an individual explanation. Do not silently add these to the MVP.

**Must resolve before API work:** How to obtain a *complete* section listing in diff mode while keeping change/old-vs-new identities deterministic; handling deleted-only sections (before-source), renames, unsupported languages and zero-section files. Record the decision in [architecture and API contracts](architecture-api.md).

## Design references

- [Svelte runes](https://svelte.dev/docs/svelte/what-are-runes)
- [WAI-ARIA tabs pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/) and [manual-activation example](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/examples/tabs-manual/)
- [WAI-ARIA disclosure pattern](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/)
- [WCAG 2.2](https://www.w3.org/TR/WCAG22/)

## Review policy

A design review must explicitly inspect **keyboard and screen-reader behavior**, **cost/inference surprises**, **source/policy correctness**, **API feasibility**, **feature parity**, and **large-repository performance**. Each phase merges only after its exit gates pass; do not remove the existing renderer at the outset.
