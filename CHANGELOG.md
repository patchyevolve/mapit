# Changelog

## 0.5.0 — Ask AI overhaul: project context, source spans, natural language understanding

- **Rich project context** in Ask AI — every query now includes a full project overview with per-file AI summaries and per-function name+summary lists, enabling natural language questions ("how does the main server work", "what happens on error") without exact symbol names
- **Source code spans** — Ask AI includes exact line ranges of matched functions as source code snippets (up to 30K chars, UTF-8 safe), enabling detailed line-level explanations
- **Smarter grounding** — new `"partial"` status when no symbols match but project overview is available (no more "no relevant context found")
- **Fallback output** — when AI provider is unavailable, returns project stats instead of a useless message
- 11% annotation coverage example: broader questions work via file/function summaries; deeper answers improve as annotation coverage grows

## 0.4.0 — Enhanced simulation UI, resizable panels, condition context

- **Source code panel** in simulation view — live source display for the current step, toggled via header button, stays open during playback
- **Branch conditions** — `edge.condition` extracted from if/while/for/match (all 6 languages) and surfaced in simulation step timeline and force graph (amber edges for conditional calls)
- **AI description header** — calls `/api/simulate` on mount and shows collapsible summary/entry/exit text
- **Mock arguments** — function signatures parsed for parameter names, displayed as mock values `fn(x=42, name="demo")` on call/enter/return steps
- **Resizable panels** — drag handles between timeline ↔ graph and graph ↔ source panel
- **Code deduplication**: context builders moved to `mapit-core`; `create_provider` to `mapit-ai`; `ensure_gitignore` to `mapit-core::config`; all duplicate copies removed from `simulate.rs` and `api/mod.rs`
- **Warning cleanup**: 5 unused imports removed from `api/mod.rs` and `simulate.rs`
- Tests: 22 new condition-extraction tests across Rust/Python/JS/C/C++/asm

## 0.3.0 — Bug fixes, cleanup, professional polish

- 14 bug fixes: `projects remove` implemented, `mapit open` opens browser, `trace --depth N` wired, formatted JSON output in interactive mode, mutex unwrap → HTTP 500, frontend error toasts, `--severity` filter, `--force` remap progress bar, Phase 2 progress bar, config path display, help tip for `--force`
- `docs/` folder deleted (7 spec files — largest AI tell)
- ~1956 lines of AI-generated boilerplate removed (section dividers, handler labels, fixture comments, crate-level docs, JSX comments)
- `CLASSIFY` dead code removed (prompt file, function, struct)
- Stale "Phase 4" comments removed
- README rewritten with walkthrough, CLI reference, architecture diagram
- License (MIT) removed from all Cargo.toml files

## 0.2.0 — Animated simulation, interactive CLI, batch AI

- **Execution simulation**: animated DFS through the static call graph at 5 scopes (function/file/module/subsystem/project) with playback controls
- **Splash screen + interactive CLI**: ASCII logo, use-cases banner, `mapit>` prompt with live commands (annotate, remap, status, flaws, search, open, help, exit) that talk to the running server via REST API
- **Batch AI annotation**: per-file grouping reduces AI calls by 11×; dependency-ordered processing; project overview injected into every batch; cross-file caller context
- **Batch flaw detection**: flaw-flagging also batched by file; structural dead-code gate enforced
- `mapit annotate --no-flaws` flag to skip the flaw pass
- `mapit simulate` command + REST endpoint
- `mapit projects list/remove` commands
- File-level summarization Phase 2 (CLI + resilient server version)
- Server `create_provider` fix: OpenAI-compatible base URL now read from credentials (was incorrectly using Ollama URL)
- Build.rs always rebuilds frontend on web source changes

## 0.1.0 — Initial release

- Structural mapping: walk, parse, build call graph for Rust, C, C++, asm, Python, JS/TS
- CLI: init, map, annotate, open, status, find, explain, trace, flaws, ask, config
- Interactive 3D web graph with React + react-force-graph-2d
- AI enrichment via Ollama or OpenAI-compatible providers
- Incremental re-mapping and control-flow extraction
- REST + WebSocket API for live graph exploration
