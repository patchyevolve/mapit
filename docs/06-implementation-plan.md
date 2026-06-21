# Implementation Plan
## Project: `mapit`

**Document version:** 1.0
**Depends on:** all preceding documents (01–05)
**Audience:** AI coding agent (implementer)

This plan exists so the build proceeds in a strict, testable order — each phase produces something runnable and verifiable before the next phase begins. **Do not skip ahead to later phases before earlier phases are functionally complete and manually verified.** If context is lost mid-build, re-read 07-MASTER-AI-PROMPT.md, then this file, to determine exactly which phase/step to resume at.

---

## 0. Repository Layout (set this up first, exactly, before writing any logic)

```
mapit/
  Cargo.toml                      # workspace root
  crates/
    mapit-cli/                     # binary crate — the `mapit` executable
      src/main.rs
      src/commands/                # one file per CLI subcommand (App Flow §1)
    mapit-core/                     # library crate — engine (TRD §3)
      src/walker.rs                 # directory walking (ignore crate)
      src/languages/                 # one module per language adapter (TRD §4)
        mod.rs                       # the LanguageAdapter trait (TRD §4.1)
        c.rs
        cpp.rs
        rust.rs
        asm.rs
        python.rs
        javascript.rs
      src/graph/
        model.rs                      # Rust structs mirroring data model doc §1–3 exactly
        builder.rs                     # turns parsed adapter output into graph nodes/edges
        store.rs                       # SQLite persistence (data model §6)
        incremental.rs                 # manifest diffing (TRD §3.3)
      src/control_flow.rs              # ControlFlowGraph extraction (data model §5)
      src/config.rs                    # reads/writes config files (Backend Schema doc §1–4)
    mapit-ai/                          # library crate (TRD §5)
      src/provider.rs                   # AiProvider trait
      src/ollama.rs
      src/openai_compatible.rs
      src/prompts/                       # versioned prompt template files
        summarize.txt
        classify.txt
        flag_flaws.txt
        answer.txt
      src/tasks.rs                       # the 4 task implementations (TRD §5.2) calling the trait
    mapit-server/                        # library crate (TRD §6.1)
      src/api/                            # one file per REST resource (Backend Schema doc §6)
      src/ws.rs                            # WebSocket event stream
      src/state.rs                         # shared app state (graph store handle, progress tracker)
  web/
    mapit-web/                           # the React+TS app (TRD §6, App Flow §4)
      src/components/
      src/screens/                        # one component tree per App Flow §4 screen
      src/api-client.ts                    # typed client matching Backend Schema doc §6 exactly
      src/types.ts                         # TypeScript interfaces matching data model doc §1–3 exactly
      vite.config.ts
  docs/                                    # this document set, kept in the repo for future reference
```

Build the binary so that `cargo build --release` first builds `web/mapit-web` (via a build script or documented manual step — either is acceptable, but it must be automated rather than relying on a human remembering to run `npm run build` first) and embeds `web/mapit-web/dist` via `rust-embed` into `mapit-server`.

---

## Phase 1 — Core walking + one language adapter end-to-end (Rust adapter chosen as the first, since the user's own primary project is Rust/C-based and this validates the hardest cross-language case early)

**Goal:** prove the entire pipeline shape works for one language before fanning out to five more.

1. `mapit-core::walker` — walk a directory respecting `.gitignore` + default excludes (Backend Schema doc §1's `default_ignore_patterns`), return a list of candidate files with detected language by extension.
2. Define the `LanguageAdapter` trait (TRD §4.1) in `languages/mod.rs`.
3. Implement `languages/rust.rs` using `tree-sitter-rust`: extract `function_item`, `struct_item`, `impl_item` methods, `use` statements, and call-expression references.
4. Implement `graph/model.rs` as the exact Rust-struct mirror of data model doc §1–3 (use `serde` derives now — this is the contract that everything else depends on; get field names exactly matching the spec doc, snake_case, on the first pass).
5. Implement `graph/builder.rs`: take adapter output for a set of files, produce `Node`/`Edge` values, resolve same-file and same-crate calls per TRD §3.2's confidence-tiered resolution (exact/probable/dynamic_unresolved).
6. Implement `graph/store.rs`: create the SQLite schema (data model doc §6) and write/read nodes & edges.
7. **Phase 1 done when:** running a small test Rust project through this pipeline (no CLI yet — a `#[test]` or a throwaway `main.rs` is fine) produces a correct, inspectable SQLite file with sensible nodes and `calls` edges matching manual expectation on a hand-written sample with known structure (write this sample test project as part of this phase — at least 3 files, with at least one cross-file call, one unresolved/external call, and one genuinely unreachable function to verify dead-code-relevant data (`has_incoming_calls`) is correct).

## Phase 2 — Remaining language adapters

8. Implement `c.rs` and `cpp.rs` (these can mostly share helper logic given C++ is a superset-ish grammar relationship — share code via a common internal helper module if natural, but keep two distinct adapter implementations per the trait).
9. Implement `asm.rs` per TRD §4.2's reduced scope (labels + call/jmp targets only, not full instruction semantics).
10. Implement the C/asm `extern "C"` and naming-convention cross-linking described in TRD §4.3 — write a test fixture with a `.c` file calling into a `.S` file's labeled routine and verify the edge resolves.
11. Implement `python.rs` and `javascript.rs` (TypeScript can reuse the JS grammar/adapter with minor extension — confirm via `tree-sitter-typescript`'s actual grammar split between TSX/TS/JS at implementation time).
12. **Phase 2 done when:** a small multi-language test fixture (e.g., a toy project with a C file, an asm file, and a Python build script) maps correctly end-to-end with correct cross-language edges where expected and correctly-flagged `dynamic_unresolved` edges where resolution genuinely isn't possible.

## Phase 3 — Incremental remapping + control flow extraction

13. Implement `graph/incremental.rs` per TRD §3.3 and Backend Schema doc §5's manifest file, including the "manifest vs SQLite mismatch → rebuild manifest from SQLite" recovery rule.
14. Implement `control_flow.rs` (data model §5) for at least the Rust and C adapters (the languages most relevant to the primary use case); branch/loop/sequential block extraction sufficient to answer "what's the order, and what forks where."
15. **Phase 3 done when:** re-running the pipeline on a test project after modifying exactly one file only re-processes that file (verify via a log/counter, not just "seems fast"), and a `/trace`-shaped query (even via a temporary test harness, before the real API exists) on a function with an `if` branch produces two correctly labeled paths.

## Phase 4 — CLI binary (`mapit-cli`)

16. Implement `clap`-based argument parsing for the full command table in App Flow doc §1.
17. Wire `mapit map`, `mapit map --force`, `mapit status`, `mapit find`, `mapit explain`, `mapit trace`, `mapit flaws` against `mapit-core` directly (no server needed for these — they query the SQLite store directly per TRD §3.1's "no duplicate logic" principle, by calling the same `mapit-core` functions the server will later call).
18. Implement the live progress bar (TTY-aware, falling back to plain logging per TRD §8) during `mapit map`.
19. **Phase 4 done when:** every headless command in the App Flow §1 table works correctly against a real test project from an actual terminal session, with output matching the style shown in App Flow §3.

## Phase 5 — AI provider layer (`mapit-ai`)

20. Implement the `AiProvider` trait, `OllamaProvider`, and `OpenAiCompatibleProvider` (TRD §5.1).
21. Write the four prompt templates (TRD §5.2) as separate template files, not inline strings, with clear `{{placeholder}}`-style substitution points for injected context.
22. Implement `tasks.rs`: the summarize/classify/flag_flaws/answer task functions, each building its prompt from graph context (pulled from `mapit-core`'s store) and parsing/validating the structured JSON response per TRD §5.3, including the retry-once-then-mark-unavailable behavior.
23. Implement the `mapit annotate` CLI command wired to this layer, with the same TTY-aware progress reporting pattern as mapping.
24. Implement the dead-code gating rule from data model doc §3 explicitly in code (a single, named, testable function — e.g. `is_dead_code_candidate(node) -> bool` checked before ever invoking the AI flaw task for that node) so this rule cannot be accidentally bypassed by a future change elsewhere.
25. **Phase 5 done when:** running `mapit init` against a real local Ollama install, then `mapit annotate` against the Phase 2 test fixture, produces sensible `ai_summary` text and at least one correctly-gated flaw flag, end-to-end, with a real model.

## Phase 6 — Server (`mapit-server`) + REST/WebSocket API

26. Implement `state.rs` (shared handle to the store + an in-memory progress tracker struct that both CLI progress rendering and WS events read from — per Backend Schema doc §7's single-process design).
27. Implement every endpoint in Backend Schema doc §6 exactly as specified (request/response shapes are not negotiable at this stage — match the doc).
28. Implement the WebSocket event stream with the exact message shapes in Backend Schema doc §6.
29. **Phase 6 done when:** every endpoint can be exercised with `curl`/a REST client against the Phase 2 test fixture and returns exactly the documented shape, and a simple WS client can observe `map_progress` events during a `POST /api/remap` call.

## Phase 7 — Web app (`mapit-web`)

30. Scaffold Vite + React + TypeScript + Tailwind; write `types.ts` to exactly match data model doc §1–3; write `api-client.ts` to exactly match Backend Schema doc §6.
31. Build screens in this order (each is independently testable against the running Phase 6 server before moving to the next): System Overview (App Flow §4.2) → Function Detail Panel (§4.3) → Trace View (§4.4) → Flaws Report (§4.6) → Settings (§4.7) → Ask the Codebase (§4.8) → radial callers/callees view (§4.5).
32. Implement the `GraphView` abstraction (TRD §6.3) switching between `react-force-graph` and `reactflow` based on the active view's node count.
33. Implement the WebSocket client and the live `node_updated` dispatch into a normalized client-side node store, per Backend Schema doc §6's WS section.
34. Implement the full frontend state machine exactly as laid out in App Flow doc §5.
35. Implement all empty/edge-case states from App Flow doc §6 — these are not optional polish, they are required behaviors.
36. **Phase 7 done when:** a full manual walkthrough of every screen and mode in App Flow §4, against a real mapped project, works without console errors, without dead ends, and without any screen showing a blank/broken state for pending or missing data.

## Phase 8 — Integration, binary embedding, packaging

37. Wire the build so `cargo build --release` produces one binary with the web app embedded (TRD §1.1's distribution requirement) — document the exact build command sequence in a `README.md` at the repo root.
38. Run the full first-run setup flow (App Flow §2) end-to-end for both an Ollama path and an OpenAI-compatible path (can use a free-tier OpenRouter key or a local mock server presenting the OpenAI chat-completions shape for testing if a real remote key isn't available during build).
39. Run a large-scale smoke test: point `mapit` at the biggest available real codebase (the user's own OPERtur/TRY1 kernel project is an excellent real-world candidate given it's multi-language C/asm and already exists) and verify it completes without crashing, produces a sensible feature classification, and the web UI remains responsive at the System Overview level.
40. **Phase 8 / project done when:** PRD §6's full success-criteria list can be checked off against this real test run.

---

## Testing Philosophy Throughout

- Every phase that touches parsing/graph-building must include at least one small, hand-constructed test fixture with a **known correct answer**, checked in alongside the code (e.g., under `crates/mapit-core/tests/fixtures/`) — never rely solely on "ran it on a real project and it looked plausible" as the only validation for structural correctness, per PRD §6 and TRD §3's "structure before semantics, must be real, not hallucinated" principle.
- AI-layer tests (Phase 5) should mock the `AiProvider` trait for fast, deterministic unit tests of `tasks.rs`'s prompt-building and response-parsing logic, separately from a small number of real-provider integration tests that are allowed to be slower/manual.
- Frontend component tests are encouraged but, given this is a single-developer learning-context tool, manual screen-by-screen walkthroughs per phase's "done when" criteria are the minimum acceptable bar — do not skip the manual walkthroughs even if automated tests exist.

## What To Do If Context Is Lost Mid-Build

If you are an AI coding agent resuming this project without full memory of prior progress:
1. Read `docs/07-MASTER-AI-PROMPT.md` first.
2. Read this file in full.
3. Inspect the actual repository state (`crates/*/src` contents, presence/absence of test fixtures, git log if available) to determine which phase's steps are actually complete versus merely planned.
4. Resume at the first incomplete step of the first incomplete phase — do not assume a later phase is safe to start just because some of its files exist; verify the "done when" criteria of every earlier phase first.
