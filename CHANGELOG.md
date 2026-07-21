# Changelog

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
