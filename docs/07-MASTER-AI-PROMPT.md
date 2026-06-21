# Master AI Build Prompt — `mapit`

**Purpose of this file:** this is the single prompt to give an AI coding agent (e.g., an opencode-driven model, Claude Code, or similar) to build this entire project from nothing to a working v1, and to re-orient itself if context is ever lost mid-build across sessions. Paste this whole file as the system/task prompt, with the rest of `docs/` available alongside it in the repository.

---

## What you are building

You are building **`mapit`**: a command-line tool that a developer runs inside any project folder (`mapit`) to produce a complete, AI-enriched, interactively explorable map of that codebase — every function, file, and module; every call/include/reference relationship between them; the execution order through any chosen entry point; AI-classified feature groupings; and AI-flagged dead code, structural flaws, and suspected bugs. The structural graph comes from real parsing (tree-sitter), never from AI guesswork. The AI (Ollama by default/locally, or any OpenAI-compatible remote/free provider the user configures) is layered on top for semantic explanation, classification, and flaw-flagging only. Results are shown two ways: concise status/output in the terminal, and a rich interactive graph in a local web app that `mapit` auto-opens in the browser.

The user's daily workflow is one command: `mapit`, run from inside the folder they want to understand.

## Document map — read in this order, every time you start or resume work

1. **`docs/01-PRD.md`** — what this product is, who it's for, what success looks like, what is explicitly out of scope. Read this first, always, to re-anchor on intent if anything else is ambiguous.
2. **`docs/02-TRD.md`** — the architecture: why Rust core + TS/React web UI, the four major components, the tech stack table, all subsystem requirements (parsing, AI abstraction, server, storage, security).
3. **`docs/03-graph-data-model.md`** — the exact node/edge/flaw data shapes. This is the contract every other piece of code must conform to. If you're ever unsure what fields a node should have, this document is the answer, not your own judgment in the moment.
4. **`docs/04-app-flow.md`** — every CLI command and every web app screen/state, written out concretely, including exact example terminal output and exact screen-by-screen behavior. Build exactly these flows.
5. **`docs/05-backend-schema.md`** — exact config file JSON shapes and the exact REST/WebSocket API contract (every endpoint, every request/response shape).
6. **`docs/06-implementation-plan.md`** — the phased build order, the exact repository/module layout to set up first, and a "done when" checklist for every phase. **This is your execution checklist. Follow its phase order strictly.**
7. **This file** — orientation and the operating rules below.

## Operating rules while building

- **Never invent a data shape that isn't in `03-graph-data-model.md` or `05-backend-schema.md`.** If a feature genuinely needs a new field, update that document first (in writing, in the doc file itself), then write the code to match. The documents are the source of truth; code that disagrees with them is the bug, not the other way around.
- **Structure before semantics, always.** The call graph, dependency graph, and control-flow data must be built from real syntax-tree parsing. The AI model never invents structural edges — it only explains, classifies, and flags issues on top of structure that already deterministically exists. If you ever find yourself about to have the AI model "figure out what calls what," stop — that is a parsing task, not a prompting task.
- **Follow the phased implementation plan in order.** Do not start Phase 5 (AI layer) before Phase 1–4 (core engine + CLI) actually work end-to-end on a real test fixture. Do not start Phase 7 (web app) before Phase 6 (server) actually serves correct data. Each phase's "done when" criteria in `06-implementation-plan.md` must be genuinely true, not assumed, before moving on.
- **Local-first, privacy-respecting, by default.** Ollama is the default AI provider. No telemetry, ever. The local web server binds to `127.0.0.1` only, never `0.0.0.0`, by default. Remote provider use is always opt-in and disclosed.
- **Never write into the user's source tree** except the single `.mapit/` metadata directory, which must be `.gitignore`d automatically. `mapit` is read-only with respect to the code it analyzes.
- **Degrade gracefully, never crash silently.** A file that fails to parse, a provider that's unreachable, a malformed AI response — all of these have a specified graceful-degradation behavior in `02-TRD.md` §9 and `04-app-flow.md` §6. Implement those paths; do not let any single failure take down the whole run.
- **The provider abstraction must stay generic.** Adding support for a new "free provider" the user discovers later should require, at most, adding one entry to the bundled preset JSON file (`05-backend-schema.md` §3) — never a new Rust adapter, because `OpenAiCompatibleProvider` already covers any OpenAI-chat-completions-shaped endpoint. Do not write per-vendor adapter code beyond the two providers specified in `02-TRD.md` §5.1.
- **The language adapter interface must stay generic.** Adding a new language later means implementing the one `LanguageAdapter` trait (`02-TRD.md` §4.1) — never modifying core engine logic. Keep that boundary clean from the start.
- **Test structural correctness against hand-built fixtures with known answers**, per `06-implementation-plan.md`'s testing philosophy — never validate parsing/graph correctness purely by "ran it on a real project, looks about right."
- **If you lose context** (new session, compacted history, etc.): re-read this file, then `06-implementation-plan.md`, then inspect the actual repository state to determine real progress before continuing. Do not assume prior session's claimed progress is accurate — verify against actual files and the phase "done when" criteria.

## Definition of done for v1

v1 is complete when every item in `01-PRD.md` §6 ("Success Criteria") is genuinely true, verified via the large-scale smoke test described in `06-implementation-plan.md` Phase 8, ideally run against a real, substantial, multi-language codebase (a kernel/systems project is an excellent stress test given the project's stated target users in `01-PRD.md` §3).

## A note on scope discipline

This is a large system described across six detailed documents on purpose — it deserves a real architecture, not a quick hack, because it's meant to be genuinely used on large, real, messy codebases. But "large system" does not mean "build everything at once." The phased plan exists specifically so that at every point during the build, there is a smaller, working, verifiable thing. Resist the urge to jump ahead to the impressive parts (the graph UI, the AI flaw detection) before the boring, correct foundation (file walking, parsing, a real SQLite-backed graph) is solid. The whole product's credibility — "this graph is real, not hallucinated" — depends on that foundation being right.
