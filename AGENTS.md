# AGENTS.md — Rules for any AI agent working in this repository

This file is the contract. If anything you are about to do conflicts with this file, **stop and follow this file**, not your own judgment, not a prior session's apparent intent, not a convenient shortcut. This file is intentionally short and imperative. The reasoning behind every rule here is written out in full in `docs/` — read this file first, the relevant doc second if you need the "why."

---

## 0. Read order (do this before writing any code, every fresh session)

1. This file, in full.
2. `docs/07-MASTER-AI-PROMPT.md`
3. `docs/06-implementation-plan.md` — then inspect the actual repo state (see §6 below) to find your real resume point.
4. Only the specific doc(s) relevant to the phase/file you're about to touch, from the table in §1.

Do not start writing code before step 3 is done. Do not trust a chat summary or your own prior turn's claim about "what's done" — verify against files on disk.

---

## 1. Document index (reference, not required reading every time)

| Doc | Use it when you need... |
|---|---|
| `docs/01-PRD.md` | Product intent, success criteria, what's explicitly out of scope |
| `docs/02-TRD.md` | Architecture, tech stack, why a component exists |
| `docs/03-graph-data-model.md` | **The exact shape of a node/edge/flaw.** Non-negotiable. |
| `docs/04-app-flow.md` | Exact CLI command behavior, exact web screen/state behavior |
| `docs/05-backend-schema.md` | Exact config file JSON shapes, exact REST/WebSocket contract |
| `docs/06-implementation-plan.md` | Repo layout, phase order, "done when" criteria |
| `docs/07-MASTER-AI-PROMPT.md` | Top-level orientation and operating philosophy |

---

## 2. Hard rules — never violate these, no exceptions

- **NEVER invent a data shape.** If `03-graph-data-model.md` or `05-backend-schema.md` doesn't define a field/endpoint/file format you need, **edit that doc first**, in writing, then write code to match it. Code that disagrees with the docs is the bug.
- **NEVER let AI output become structural graph data.** Calls, includes, defines, references — these come only from tree-sitter parsing in `mapit-core`. The AI layer (`mapit-ai`) only summarizes, classifies, and flags on top of structure that already deterministically exists. If you're about to prompt a model to determine "what calls what," stop — that's a parsing bug, not a prompting task.
- **NEVER flag `dead_code` from AI judgment alone.** It must be gated on `has_incoming_calls == false` AND `is_entry_point_candidate == false` first (structural facts), with AI only assessing plausibility on top. This gate must exist as one named, testable function. See `03-graph-data-model.md` §3.
- **NEVER write into the analyzed project's source tree** except the single `.mapit/` directory, which must be auto-`.gitignore`d. `mapit` is read-only with respect to user code.
- **NEVER bind the local server to `0.0.0.0` by default.** `127.0.0.1` only, unless the user explicitly passes a flag.
- **NEVER add telemetry, analytics, or any "phone home" behavior.**
- **NEVER write a new per-vendor AI adapter.** New remote providers go through `OpenAiCompatibleProvider` plus one new entry in the bundled preset JSON (`05-backend-schema.md` §3). Only two provider implementations exist, ever, in v1: Ollama and OpenAI-compatible.
- **NEVER add a new supported language by touching core engine logic.** New languages implement the `LanguageAdapter` trait only (`02-TRD.md` §4.1).
- **NEVER skip ahead in the phase order** in `06-implementation-plan.md`. Do not start Phase 5 before Phase 1–4's "done when" criteria are actually, verifiably true. Do not start Phase 7 before Phase 6 actually serves correct data.
- **NEVER let a single failure take down a whole run.** A bad file parse, an unreachable provider, a malformed AI JSON response — each has a specified degraded-but-alive behavior. Implement it. Silent crashes and unhandled panics are bugs, full stop.
- **NEVER log or display a raw API key.** Redact to last 4 characters in any error/log/UI surface.

---

## 3. Build order (do not reorder — full detail in `06-implementation-plan.md`)

```
Phase 1  Core walker + Rust language adapter, SQLite store        → prove the pipeline shape
Phase 2  Remaining language adapters (C, C++, asm, Python, JS/TS) → cross-language edges work
Phase 3  Incremental remap + control-flow extraction               → re-runs are fast & correct
Phase 4  CLI binary, all headless commands                         → usable without AI, without browser
Phase 5  AI provider layer (Ollama + OpenAI-compatible)            → summaries/classify/flaws work
Phase 6  Server: REST + WebSocket API                               → matches 05-backend-schema.md exactly
Phase 7  Web app: all screens per 04-app-flow.md                    → full interactive graph works
Phase 8  Embed web build into binary, large-scale smoke test        → PRD §6 success criteria all pass
```

Each phase has explicit "done when" criteria in `06-implementation-plan.md`. Treat them as gates, not suggestions.

---

## 4. Repository layout (set up exactly once, in Phase 1 — see `06-implementation-plan.md` §0 for the full tree)

```
mapit/
  Cargo.toml
  crates/
    mapit-cli/      # the `mapit` binary, one file per subcommand
    mapit-core/      # walker, language adapters, graph builder/store, control flow
    mapit-ai/         # provider trait, ollama.rs, openai_compatible.rs, prompts/, tasks.rs
    mapit-server/      # REST + WS, embeds web/mapit-web/dist via rust-embed
  web/
    mapit-web/          # React + TS + Tailwind, types.ts mirrors data-model doc exactly
  docs/                  # this doc set — never delete, keep in sync with code
```

---

## 5. Testing requirements (not optional)

- Every parser/graph-building change ships with a hand-built fixture with a **known correct answer**, checked into `crates/mapit-core/tests/fixtures/`. "Ran it on a real project, looked plausible" is never sufficient evidence of structural correctness on its own.
- AI-layer logic (`tasks.rs` prompt-building, response parsing) is unit-tested against a mocked `AiProvider`, separate from any slower real-provider integration test.
- Before marking any phase complete, manually walk through that phase's "done when" criteria — don't infer completion from code existing.

---

## 6. Resuming after context loss / compaction

When you don't have reliable memory of prior progress in this session:

1. Re-read this file top to bottom.
2. Re-read `docs/06-implementation-plan.md`.
3. Run a repo inventory before writing anything:
   ```
   find crates web -type f -name '*.rs' -o -name '*.ts' -o -name '*.tsx' | sort
   ls crates/mapit-core/tests/fixtures/ 2>/dev/null
   git log --oneline -20 2>/dev/null
   ```
4. For each phase in §3 above, in order, check its "done when" criteria against what you actually find on disk — not against what a prior message claims. The first phase whose criteria are not fully met is your resume point.
5. Do not assume a later phase's files being present means an earlier phase is solid. Verify earlier phases first regardless.
6. If you find code that contradicts `03-graph-data-model.md` or `05-backend-schema.md`, the docs win — fix the code, and only edit the docs if you have a genuine, deliberate reason to change the contract itself (and if so, update the doc explicitly, in its own commit/step, before changing dependent code).

---

## 7. Scope discipline

This is a real, sizeable system, described in detail on purpose — but detail is not license to build everything in parallel. At every point in the build there must be a smaller, working, verifiable thing. Do not jump to the graph UI or AI flaw detection before the boring foundation (walking, parsing, a correct SQLite-backed graph) is solid and tested. The product's entire value proposition — "this graph is real, not hallucinated" — depends on that foundation being right before anything else is layered on top.
