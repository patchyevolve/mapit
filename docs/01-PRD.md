# Product Requirements Document (PRD)
## Project: `mapit` — AI-Powered Interactive Codebase Mapper

**Document version:** 1.0
**Status:** Approved for implementation
**Audience:** AI coding agent (implementer), project owner, future contributors

---

## 1. Summary

`mapit` is a command-line tool that, when run inside any folder, analyzes the entire codebase in that folder — no matter how large — and builds a complete, navigable map of how the code is structured and how it actually behaves at runtime: which function calls which, in what order, across which files, modules, and languages. It uses a locally-run or remotely-hosted AI model (configurable, Ollama-first) to enrich this structural map with semantic understanding: what each unit of code is *for*, how units group into features, where things look broken, dead, duplicated, or fragile.

The result is presented two ways:

1. **In the terminal** — fast, essential status output: progress while mapping, a textual summary of the codebase once done, and lightweight query/answer commands.
2. **In a local web app** that `mapit` launches automatically — a fully interactive, explorable graph of the entire codebase, where the user can zoom from "whole system" down to "single function," follow real call chains, see execution order, and see AI-flagged issues overlaid directly on the graph.

The user's core daily command is simply:

```
mapit
```

run from inside the root folder of the project they want to understand.

---

## 2. Problem Statement

Developers — especially when joining a new codebase, returning to old code, doing a security/quality review, or working with a codebase too large to hold in their head — face several recurring problems:

- **No single place shows how code actually connects.** IDEs show "go to definition" and "find references" one symbol at a time. There is no fast way to see the *whole* call graph, the *whole* dependency graph, and the *whole* data flow at once, at any depth the user chooses.
- **Execution order is invisible from reading source alone.** Static reading of code does not make it obvious "function A calls B, which conditionally calls C or D, then control returns to A which calls E" — especially across files, threads, async boundaries, or language boundaries (e.g., C calling into assembly, Rust calling into C, JS calling into native bindings).
- **Dead code, unreachable branches, and structural flaws hide in plain sight** in large codebases. Nobody has time to trace every path manually.
- **New contributors (or the original author returning after months) need a fast onboarding map** — "show me this codebase's whole life cycle, start to end" — rather than reading every file top to bottom.
- **Existing tools are fragmented**: linters find style issues, call-graph generators are usually single-language and produce static images, AI chat tools can explain code but have no persistent structural model and no visual interface, and IDE graphs are local-context-only and not explorable as a whole-system map.

`mapit` exists to be the single tool that takes "I have a folder of code, of any size, in any language(s)" and produces "a complete, interactive, intelligent, visual understanding of that code, from system-level down to function-level, including its flaws."

---

## 3. Target Users

- **Primary:** Individual developers and students (e.g., systems/embedded/kernel developers, students returning to personal projects after a break) who need to deeply understand a codebase — their own or someone else's — quickly and thoroughly.
- **Secondary:** Developers performing code review, onboarding onto a new team's codebase, doing technical due diligence, or auditing a codebase for dead code/bugs before a refactor or handoff.
- **Tertiary:** Teams who want a shared, regenerable visual map of their system architecture without paying for or maintaining a separate enterprise tool.

This tool is explicitly designed to work well on **systems-level, low-level, and embedded codebases** (C, C++, Rust, assembly, kernel code) as well as mainstream application codebases (Python, JS/TS, Go, Java, etc.) — it must not assume a single-language, single-paradigm, application-only codebase.

---

## 4. Goals

### 4.1 Primary Goals (must-have for v1)
1. Run a single command (`mapit`) inside any project folder and produce a complete structural map of the codebase, regardless of size, regardless of programming language(s) used, including mixed-language codebases.
2. Build a true **call graph** and **dependency graph**: which function/method/macro calls which, which file imports/includes which, which module depends on which — across the entire codebase, at every depth.
3. Reconstruct **execution order / control flow** for any given entry point the user selects (e.g., `main`, an HTTP handler, an interrupt handler, an exported API function) — showing the order in which functions are invoked, including branches, loops, and conditional paths, to the depth the user requests.
4. Use a user-selected AI model (local via Ollama by default, or any OpenAI-compatible remote/free provider) to:
   - Classify code into **features / subsystems / logical units** (e.g., "networking," "scheduler," "shell parser") even when the codebase itself has no such organization.
   - Generate human-readable explanations of what each function, file, module, and feature does.
   - Flag likely **dead code** (unreachable or never-called code), **structural flaws** (e.g., circular dependencies, overly coupled modules, missing error handling, resource leaks, inconsistent patterns), and **probable bugs** (e.g., logic that contradicts itself, suspicious patterns the model recognizes).
5. Render the full map as a **high-interactivity visual graph** in a locally hosted web app, automatically opened in the user's browser, supporting:
   - Zoom from whole-system view down to single-function view.
   - Click-to-expand/collapse any node (file, module, feature, function).
   - "Trace this function" mode: highlight every caller and every callee, recursively, to a chosen depth.
   - "Show execution order" mode: animate or numerically label the call sequence from a chosen entry point.
   - Visual flags for AI-detected dead code, flaws, and errors directly on the graph (e.g., color coding, badges).
   - Search/jump to any symbol, file, or feature by name.
6. Provide a useful, readable **CLI experience** for status, progress, configuration, and quick queries — without requiring the browser for basic use (e.g., `mapit status`, `mapit find <symbol>`, `mapit explain <symbol>`).
7. **First-run setup** must let the user configure their AI provider and model (Ollama model name, or API key + base URL + model name for any OpenAI-compatible provider such as OpenRouter or other free/compatible providers), and the user must be able to **change this configuration at any later time** without re-doing first-run setup from scratch.
8. The tool must be able to **re-map incrementally**: after the first full map, subsequent runs should detect changed files (via hashing/mtime) and only re-analyze what changed, rather than re-processing the entire codebase every time, so it remains usable on large, evolving codebases.

### 4.2 Secondary Goals (should-have, can land after v1 core is stable)
9. Export the map (graph data, AI annotations, summaries) to portable formats (JSON, static HTML, Markdown report) for sharing or archiving outside the live tool.
10. Support multiple saved "projects" so a user can switch between previously mapped codebases without re-mapping.
11. Allow the user to ask free-form questions about the mapped codebase ("which functions touch the network driver?") and get an answer grounded in the actual graph data (not just an AI guess), with the relevant nodes highlighted in the web UI.
12. Provide a "diff" view: compare the current map against a previous map (e.g., after a refactor) to show what structurally changed.

### 4.3 Explicit Non-Goals (out of scope for v1)
- `mapit` is **not** a linter or formatter replacement; it does not enforce style rules.
- `mapit` is **not** a full static-analysis security scanner (e.g., it is not a CVE/vulnerability database tool); flaw detection is heuristic and AI-assisted, not a formal verification system.
- `mapit` does **not** execute or run the target codebase to build the map; mapping is done via static analysis of source files only (dynamic tracing is an explicit non-goal for v1, see open questions in TRD).
- `mapit` does **not** modify, refactor, or "fix" the user's code; it is read-only with respect to the analyzed codebase.
- `mapit` is **not** a hosted/cloud SaaS product in v1; everything runs locally on the user's machine (the AI model call may go to a remote provider if the user chooses one, but the tool itself, its UI, and its data stay local).

---

## 5. Key User Stories

1. *"As a developer opening a 50,000-file repo I've never seen before, I want to run one command and get a visual map of the whole system, organized by feature, so I know where to even start reading."*
2. *"As the original author of a personal OS kernel project, returning to it after weeks away, I want to trace exactly what happens from boot to a specific subsystem hang, function by function, in the order it actually executes, so I can find where execution goes wrong."*
3. *"As a developer doing a pre-handoff review, I want the tool to point out dead code and obviously duplicated or contradictory logic across the whole codebase, so I can clean it up before someone else inherits it."*
4. *"As a privacy-conscious developer, I want the AI analysis to run fully locally via Ollama by default, with no code ever leaving my machine, unless I explicitly choose to configure a remote provider."*
5. *"As a developer working across C, assembly, and a build-system layer in one project, I want the map to correctly represent cross-language calls (e.g., C calling into an asm routine) rather than treating each language as an isolated island."*
6. *"As a returning user, I want to change my AI provider/model later (e.g., switch from a small local model to a stronger remote one for one deep analysis pass) without losing my existing map or having to reconfigure everything from scratch."*

---

## 6. Success Criteria

`mapit` v1 is successful if:

- A user can go from "empty terminal in a project folder" to "first full interactive map open in browser" in a single command plus first-run configuration, for codebases ranging from a few files to large multi-language repositories (target: tested up to at least ~50,000 source files / several million lines without crashing or becoming unusably slow — see TRD performance budget).
- The call graph and dependency graph are **structurally correct** for statically resolvable calls in supported languages (i.e., not hallucinated — built from real parsing, not just AI guesswork), with AI used for *semantic* enrichment (naming, classification, flaw-flagging) layered on top of a *deterministic* structural graph.
- A user can pick any function and correctly see, at minimum, its direct callers and callees, and can expand outward to full transitive closure.
- A user can pick an entry point and see a correct, orderable execution trace through static call relationships (acknowledging conditional branches as branches, not as a single linear guess).
- The AI flaw/dead-code detection produces a list a real developer finds *plausible and useful* even if not 100% precise — framed clearly as heuristic suggestions, never as guaranteed facts.
- Re-running `mapit` on a codebase with a small change (e.g., one file edited) completes the incremental re-map meaningfully faster than the first full run.
- The web UI graph remains usable (not a frozen, unreadable hairball) even on very large codebases, via progressive disclosure (collapsed clusters by default, expand on demand) — see App Flow doc and TRD for the specific UX mechanism.

---

## 7. Constraints & Principles

- **Local-first and privacy-respecting by default.** Ollama is the default/primary provider. Any remote provider is opt-in, explicit, and clearly communicated to the user (what gets sent, when).
- **Structure before semantics.** The graph's structural edges (calls, includes, defines, references) must come from real parsing/static analysis, never purely from the AI model. The AI model explains, classifies, and flags — it does not invent the graph.
- **Language-agnostic core, pluggable language support.** The architecture must not hardcode assumptions for one language; new language support should be addable via a defined plugin/adapter interface (see TRD).
- **Read-only on the target codebase.** `mapit` must never write into the user's source tree (all of mapit's own data lives in a dedicated cache/config directory, never inside the analyzed project's source files).
- **Works offline** for the structural mapping and (if Ollama is selected) for the AI enrichment too. Only remote-provider mode requires network access.
- **Scales gracefully.** The tool must degrade gracefully (e.g., longer processing time, clearly communicated) rather than failing outright on very large codebases.

---

## 8. Open Questions for Future Iteration (acknowledged, not blocking v1)

- Whether to eventually support optional dynamic/runtime tracing (e.g., via debugger hooks or instrumented runs) to capture *actual* execution order including data-dependent branches, as a supplement to static analysis. Flagged as a strong v2 candidate, not required for v1.
- Whether to support team/multi-user shared maps (today's scope is single local user, single local machine).
- Formal plugin marketplace for community-contributed language adapters.

These are **not** to be built in v1 but should not be architecturally foreclosed (see TRD extensibility section).
