# Technical Requirements Document (TRD)
## Project: `mapit` — AI-Powered Interactive Codebase Mapper

**Document version:** 1.0
**Depends on:** 01-PRD.md
**Audience:** AI coding agent (implementer)

---

## 1. Architecture Overview

`mapit` is composed of four major components that together form one product, distributed as a single CLI install:

```
┌─────────────────────────────────────────────────────────────────────┐
│                              mapit CLI                              │
│   (Rust binary — entry point, command parsing, orchestration)       │
└───────────────┬───────────────────────────────────┬─────────────────┘
                │                                   │
                ▼                                   ▼
   ┌─────────────────────────┐         ┌─────────────────────────────┐
   │     mapit-core           │         │      mapit-server            │
   │  (Rust library)           │         │  (Rust, embedded HTTP+WS)    │
   │  - file walking           │         │  - serves local web app      │
   │  - language adapters      │         │  - serves graph data API     │
   │  - parsing (tree-sitter)  │         │  - streams progress events   │
   │  - graph builder           │        │  - handles live queries      │
   │  - incremental diffing    │         └───────────────┬─────────────┘
   │  - storage (SQLite)       │                         │
   └───────────────┬───────────┘                         │
                   │                                     ▼
                   ▼                         ┌─────────────────────────┐
   ┌─────────────────────────┐               │     mapit-web             │
   │   mapit-ai                │              │  (TypeScript + React)     │
   │  (Rust module)            │              │  - interactive graph UI   │
   │  - provider abstraction   │              │  - runs in user's browser │
   │  - Ollama adapter         │              │  - talks to mapit-server  │
   │  - OpenAI-compatible      │◄────calls────┤    via REST + WebSocket   │
   │    adapter (OpenRouter,   │              └─────────────────────────┘
   │    opencode-style, etc.)  │
   │  - prompt templates        │
   │  - response parsing/        │
   │    validation                │
   └─────────────────────────┘
```

### 1.1 Why this split

- **Rust for the core engine and CLI**: file-system walking, parsing, and graph construction over potentially millions of lines of code need to be fast and memory-safe; Rust gives both without a runtime/GC pause problem, and compiles to a single static binary that's trivial to install (no Python/Node runtime required for the core tool to function). This also matches the user's own stated skillset/interest (systems programming), making the resulting codebase itself inspectable/extensible by the user if desired.
- **TypeScript + React for the web UI**: the interactive graph (pan/zoom/expand/collapse/search across potentially tens of thousands of nodes) is squarely a rich-front-end problem. React plus a graph-rendering library (see §6) is the pragmatic, well-supported choice; trying to build this UI in Rust-to-WASM is possible but would slow development substantially for no architectural benefit, since the web app's only job is talking to a local HTTP/WebSocket API.
- **Single binary distribution**: the compiled Rust binary embeds the built web app's static assets (HTML/JS/CSS) at compile time, so installing `mapit` does not require the user to separately install Node.js or any web stack to *run* the tool. Node/TypeScript tooling is only needed at *build time* by whoever compiles `mapit` from source (the coding agent now, or a contributor later), not by end users running pre-built releases.

---

## 2. Technology Stack

| Layer | Choice | Justification |
|---|---|---|
| CLI / orchestration | Rust (stable toolchain) | Fast, safe, single static binary, no runtime dependency for end users |
| Argument parsing | `clap` (derive API) | De facto standard, gives `--help`/subcommands for free |
| File walking | `ignore` crate (same engine ripgrep uses) | Respects `.gitignore`, fast parallel directory walk, well-tested at scale |
| Parsing | `tree-sitter` + per-language grammars | Battle-tested incremental parser used by GitHub, Neovim, Helix; supports huge range of languages including C, C++, Rust, asm-adjacent, Python, JS/TS, Go, Java, etc.; produces real syntax trees (not regex hacks), which is essential for a *structurally correct* graph (PRD §6) |
| Graph storage | SQLite (via `rusqlite`) embedded, file-based | Zero-install embedded DB, handles large graphs fine with proper indices, supports incremental updates, queryable directly for CLI quick-queries without loading everything into memory |
| In-memory graph algorithms | `petgraph` | Standard Rust graph algorithms crate (traversal, cycle detection, shortest path) used transiently on subgraphs pulled from SQLite |
| Local HTTP/WS server | `axum` + `tokio` | Modern, well-supported async Rust web framework; serves both the REST API and a WebSocket channel for live progress/streaming |
| AI provider calls | `reqwest` (HTTP client) | Used by `mapit-ai` to call Ollama's local HTTP API and any OpenAI-compatible REST endpoint |
| Web frontend | React + TypeScript, bundled with Vite | Standard, fast-iterating modern frontend stack; Vite produces a static bundle that gets embedded into the Rust binary |
| Graph rendering | `react-force-graph` (WebGL via three.js) for large/whole-system views, falling back to `reactflow` for focused/small subgraphs (e.g., single feature, single trace) | Force-graph handles thousands of nodes performantly via WebGL; reactflow gives precise, readable layout for small, "explain this to me" views. Both are swappable behind one internal `GraphView` abstraction — see §6.3 |
| Styling | Tailwind CSS | Fast to build consistent, clean UI without hand-rolling CSS infrastructure |
| Embedding web assets into binary | `rust-embed` | Compiles the built `mapit-web/dist` folder directly into the release binary |

---

## 3. Component: `mapit-core` (the engine)

### 3.1 Responsibilities
- Walk the target directory respecting `.gitignore`/`.mapitignore` and sensible default excludes (`node_modules`, `.git`, `target`, `build`, `dist`, binary/media files, etc. — full default list in implementation, user-overridable via `.mapitignore`).
- Detect language per file (extension + content sniffing fallback for ambiguous extensions like `.h`).
- Parse each file with the appropriate `tree-sitter` grammar into a syntax tree.
- Run language-specific **adapters** (see §4) over each syntax tree to extract:
  - Symbol definitions (functions, methods, structs/classes, macros, global variables, constants).
  - Symbol references (calls, instantiations, macro invocations).
  - File-level relationships (`#include`, `import`, `use`, `require`, module declarations).
  - Cross-language boundary markers where detectable (e.g., `extern "C"` blocks, FFI declarations, `asm!()` blocks, `.S`/`.s` files referenced from build files).
- Resolve references to definitions wherever statically resolvable, building edges in the graph (see Graph & Data Model spec, doc 03).
- Persist the resulting graph into the SQLite store under `.mapit/` (see §7).
- On re-run: hash each file's contents; only re-parse files whose hash changed since the last run; remove nodes/edges originating from deleted files; merge updated subgraphs into the existing graph without a full rebuild (see §3.3).
- Expose a clean internal API (Rust trait-based) that both the CLI commands and the embedded server call into — there should be no duplicate logic between "CLI does X" and "server does X."

### 3.2 Parsing & resolution approach (must be deterministic, not AI-based)
- **Definitions** are extracted directly from syntax tree node types per language (e.g., `function_item` in Rust's grammar, `function_definition` in C's grammar).
- **Call edges** are extracted by walking the syntax tree for call-expression nodes and resolving the called name to:
  1. A definition in the same file (highest confidence),
  2. A definition in an explicitly imported/included file (high confidence),
  3. A definition found anywhere else in the project with a matching name and matching language-appropriate scoping rules (medium confidence — flagged as such in the edge metadata),
  4. Unresolved (e.g., calling into a binary library with no source present, or genuinely dynamic dispatch) — kept as a "dangling call" node so the user still sees that the call happens, clearly marked as externally-unresolved rather than silently dropped.
- Each edge stores a **confidence level** (`exact`, `probable`, `dynamic_unresolved`) so the UI can visually distinguish "this is a confirmed static call" from "this is our best guess." This directly serves PRD §6's requirement that the structural graph be real, not hallucinated — uncertainty is represented explicitly rather than hidden.
- Control-flow extraction (for the "execution order" feature) walks each function body's syntax tree to build a simplified control-flow graph: sequential statements, branch points (`if`/`switch`/`match`), and loop constructs, sufficient to answer "in what order are these calls reached, and under what condition." This is **not** a full data-flow/symbolic-execution engine — v1 scope is structural/control-flow order, not value tracking. This limitation must be clearly stated in the UI (e.g., "branches shown; runtime-dependent paths not evaluated").

### 3.3 Incremental re-mapping
- Maintain a per-file manifest: `path -> (content_hash, last_parsed_at, language)`.
- On each run: walk the directory fresh (cheap), compare hashes against manifest, classify files as `unchanged`, `modified`, `added`, or `deleted`.
- For `modified`/`added` files: re-parse and re-extract symbols/edges, replacing only that file's prior contributions in the graph store.
- For `deleted` files: remove their nodes/edges.
- For `unchanged` files: no work done.
- After structural re-sync, recompute affected AI annotations only for nodes whose underlying code actually changed (track a hash per AI-annotated node so unchanged code doesn't get needlessly re-sent to the AI provider — this also controls cost/time for remote providers).

### 3.4 Performance budget (target, not hard guarantee)
- Initial full map of a ~50,000 file / multi-million-line codebase should complete structural parsing within a low number of minutes on a typical modern multi-core development laptop, using parallel file processing (one thread pool worker per file, bounded by CPU core count).
- AI semantic enrichment is the slower phase (bounded by model throughput) and must run **as a separate, resumable phase** after structural parsing completes — structural parsing must never block on AI calls, and a user must be able to use the structural map (browse the graph, see real call relationships) even while AI annotation is still in progress in the background. Progress for both phases is streamed to the CLI and the web UI (see §8).

---

## 4. Component: Language Adapters (pluggable)

### 4.1 Adapter interface (conceptual, Rust trait)
Every supported language implements one adapter exposing:
- `language_id() -> &'static str`
- `file_extensions() -> &'static [&'static str]`
- `tree_sitter_grammar() -> tree_sitter::Language`
- `extract_definitions(tree, source) -> Vec<SymbolDefinition>`
- `extract_references(tree, source) -> Vec<SymbolReference>`
- `extract_imports(tree, source) -> Vec<ImportStatement>`
- `extract_control_flow(tree, source, function_def) -> ControlFlowGraph`

This interface must be defined as a real Rust trait early in implementation (see Implementation Plan doc 06, Phase 1) so that adding a new language later is a matter of implementing one trait, not modifying core engine code.

### 4.2 v1 required language adapters
Given the target users (systems/embedded/kernel-leaning, per PRD §3), v1 **must** ship working adapters for:
- **C**
- **C++**
- **Rust**
- **Assembly** (x86-64 NASM/GAS-style `.asm`/`.s`/`.S` — at minimum: label/function definitions via standard prologue patterns and explicit call/jmp target extraction; full instruction-level semantic understanding is not required, only "what labels exist and what calls/jumps to what")
- **Python**
- **JavaScript / TypeScript**

v1 **should** ship (lower priority, can slip to fast-follow if time-constrained):
- **Go**
- **Java**

### 4.3 Cross-language boundary handling
- C/C++ `extern "C"` blocks and matching symbol names across `.c`/`.h`/`.S` files in the same project must be linked as real edges (critical for the kernel/embedded use case explicitly called out in PRD §3).
- Build files (`Makefile`, `CMakeLists.txt`, `Cargo.toml`, `build.rs`, linker scripts) are parsed at a shallow level — not full build-system semantics — just enough to detect "this source file is compiled/linked into this target," which feeds the feature-classification step (a kernel's `boot.S` and `kernel_main.c` linked into the same image are evidence they're the same subsystem).
- Unresolvable cross-language calls (e.g., a Rust `extern "C"` call to a symbol only defined in a `.S` file using a naming convention the asm adapter didn't recognize) become `dynamic_unresolved` dangling edges rather than silently dropped, per §3.2.

### 4.4 Unknown / unsupported languages
- Files in languages without an adapter are still walked, listed, and included in the file-tree view and dependency view (e.g., a `.toml` config file or a `.md` doc), but do not contribute function/call-level graph nodes — only file-level nodes with whatever import/include relationships can be cheaply detected generically (e.g., simple text pattern matches for common import keywords) clearly marked as low-confidence.
- This must never crash the run. Unknown file types degrade to "shown as a file, not deeply analyzed," never to a fatal error.

---

## 5. Component: `mapit-ai` (AI provider abstraction)

### 5.1 Provider abstraction (conceptual, Rust trait)
```
trait AiProvider {
    fn id(&self) -> &str;                    // e.g. "ollama", "openai-compatible"
    fn list_models(&self) -> Result<Vec<ModelInfo>>;
    fn complete(&self, request: AiRequest) -> Result<AiResponse>;
    fn supports_streaming(&self) -> bool;
}
```

Two concrete implementations cover essentially the entire viable provider landscape per PRD §4.1.7 / user's explicit requirement:

1. **`OllamaProvider`** (default/primary)
   - Talks to a local Ollama instance over its local HTTP API (default `http://localhost:11434`).
   - `list_models()` calls Ollama's local model-list endpoint so the setup wizard (see App Flow doc) can show the user exactly which models they already have pulled, with an option to trigger `ollama pull <model>` for a new one.
   - No API key required. This is the default because it requires no account, no cost, and keeps all code fully local — directly satisfying PRD §7's local-first principle.

2. **`OpenAiCompatibleProvider`** (covers everything else)
   - A single generic adapter implementing the OpenAI Chat Completions request/response shape (`POST /v1/chat/completions` with `model`, `messages`, etc.).
   - Configurable: **base URL**, **API key**, **model name** — three fields cover OpenRouter, opencode-hosted endpoints, Together.ai, Groq, any self-hosted vLLM/llama.cpp server exposing an OpenAI-compatible endpoint, and any future "free provider" the user wants to point at, **without writing a new adapter per vendor**.
   - Ships with a small built-in list of known-good base URL presets (OpenRouter, opencode, etc., each with its standard base URL pre-filled) purely as setup-wizard convenience — the user can always type a fully custom base URL instead. This list lives in a simple config file (not hardcoded deep in logic) so it is trivially extendable by editing one data file, never requiring new code for a new "preset."
   - API keys are stored only in the user's local config (see §7.3), never logged, never sent anywhere except the configured base URL.

### 5.2 What the AI is actually asked to do
The AI is used for **four distinct, separately-promptable tasks**, each with its own prompt template (stored as versioned template files, not hardcoded strings buried in logic — see Implementation Plan):

1. **Summarize** — given a function/file/module's structural context (its code, its callers, its callees, its imports) produce a short human-readable explanation of what it does and why it likely exists.
2. **Classify** — given the whole project's symbol/file list plus structural clustering hints (which files are heavily interconnected, which are linked into the same build target), propose a feature/subsystem grouping (e.g., "networking," "scheduler," "shell") and assign each file/symbol to a group, with a confidence score. This output directly powers the PRD §4.1.4 feature-classification requirement and the "intelligently classified" requirement.
3. **Flag flaws** — given a function/module's code and structural graph context, identify: likely dead code (cross-checked against the structural graph's actual "is this ever called" data — the AI is given the *graph fact* of zero incoming call edges and asked to assess whether that's a true dead-code case or a legitimate entry point/export the static analysis couldn't see, e.g. a function only called via reflection or registered as a callback); structural smells (circular deps, god-objects, inconsistent error handling patterns); and any logic that looks self-contradictory or buggy on inspection.
4. **Answer** — free-form Q&A grounded in retrieved graph context (used for the optional "ask about the codebase" feature, PRD §4.2.11) — this task retrieves the relevant subgraph + relevant source snippets first (deterministically, via the graph store), then asks the model to answer using only that retrieved context, and the answer is returned alongside the exact node IDs used so the UI can highlight them. This avoids ungrounded hallucination about the specific codebase.

### 5.3 Model-agnosticism requirements
- Every prompt template must work reasonably with both small local models (e.g., a 7–8B class Ollama model) and larger remote models — meaning prompts should be explicit, structured, and not rely on extremely strong reasoning, since the user may pick a small local model for speed/privacy.
- All AI responses that are meant to be structured (classification labels, flaw lists, confidence scores) must be requested as strict JSON (per the project's general structured-output pattern) and validated on receipt; on a malformed/unparsable response, retry once with a stricter "return ONLY valid JSON" instruction, then on second failure mark that item as `ai_annotation: unavailable` rather than crashing the whole AI pass. One bad response must never abort the entire enrichment run.
- All AI calls must be individually retryable, individually logged (success/fail/duration), and individually skippable on persistent failure — the enrichment pass is a big batch job over potentially tens of thousands of nodes and must be resilient to partial failure.

### 5.4 Switching provider/model later (PRD §4.1.7)
- Provider + model selection lives in a single config record (see §7.3), editable via `mapit config set-provider` / `mapit config set-model` CLI commands and via a settings panel in the web UI.
- Changing provider/model does **not** invalidate the existing structural graph (which is provider-independent) — it only means the *next* AI enrichment pass (full or incremental) uses the newly selected provider/model. Previously generated AI annotations remain visible, tagged with which provider/model produced them, until/unless the user explicitly requests re-annotation.
- `mapit annotate --all --force` lets the user explicitly re-run AI enrichment for the whole graph with the current provider/model (e.g., after switching from a small local model to a stronger one for a one-time deep pass, per PRD user story 6).

---

## 6. Component: `mapit-server` + `mapit-web` (interactive web app)

### 6.1 Server responsibilities
- On `mapit` (no subcommand, or `mapit open`), after ensuring a structural map exists (running one if not present), start a local HTTP server bound to `127.0.0.1` on a free port (try a default port, fall back to OS-assigned free port if taken), and open the user's default browser to it automatically.
- Serve the embedded built web app's static files.
- Serve a REST API for: project metadata, graph queries (get node, get neighbors to depth N, get subgraph for a feature, search), AI annotation data, and configuration read/update.
- Serve a WebSocket endpoint for: live progress during structural mapping and AI enrichment (so the web UI can show a live "mapping in progress, 4,213 / 50,000 files" bar if the user opens the browser while a map is running), and live updates if the user triggers a re-map from within the UI.
- Server binds to localhost only by default (never exposed on the network) unless the user explicitly passes a flag to bind elsewhere — default must be the private, local-only behavior.

### 6.2 Required REST endpoints (v1 minimum set)
- `GET /api/project` — project root path, last map time, file/symbol counts, current provider/model config (key redacted).
- `GET /api/graph/node/:id` — full node detail (source location, signature, AI summary, confidence flags).
- `GET /api/graph/neighbors/:id?direction=callers|callees|both&depth=N` — subgraph expansion, the core "trace this function" primitive.
- `GET /api/graph/trace/:id?max_depth=N` — execution-order trace from a chosen entry point, returned as an ordered/branching structure (not just an unordered edge list) suitable for the UI's "show execution order" mode.
- `GET /api/graph/features` — the AI-classified feature/subsystem groups and their member nodes.
- `GET /api/graph/flaws` — all AI-flagged dead code / structural flaws / suspected bugs, with severity and the node(s) they apply to.
- `GET /api/graph/search?q=...` — symbol/file/feature name search.
- `POST /api/ask` — free-form question, returns grounded answer + referenced node IDs (PRD §4.2.11 feature).
- `GET /api/config` / `PUT /api/config` — read/update provider, model, API key (write-only, never returned in reads), base URL.
- `POST /api/remap` — trigger a (incremental by default, full if `force=true`) re-map; progress streamed over WebSocket.
- `WS /api/events` — progress/status event stream.

### 6.3 Frontend interaction model (high-level; full detail in App Flow doc)
- A `GraphView` abstraction in the React app picks the rendering backend (`react-force-graph` for large/whole-codebase or whole-feature views; `reactflow` for small, focused, "explain this one function's neighborhood" views) based on the node count of the current view — this threshold-based switch is an internal implementation detail, invisible to the user, who just experiences "the graph is always smooth and readable."
- **Progressive disclosure is mandatory**, not optional polish: the default view on first opening a large codebase must be the **feature/subsystem level** (collapsed clusters, per AI classification), never a raw dump of every function as nodes — directly serving PRD §7's "graceful scaling" and §6's "not a frozen, unreadable hairball" requirement. Users drill down (feature → file → function) by clicking; they are never shown an uncollapsed full-function graph of a huge codebase by default.
- All flaw/dead-code/AI-summary data is shown as overlays/badges on existing graph nodes, never as a separate disconnected report-only view — the visual graph is the primary interface, per PRD §1's design intent ("show it like an interactive graph").

---

## 7. Storage Layout

### 7.1 Per-project cache directory
Inside the analyzed project's root, `mapit` creates (and `.gitignore`s automatically, adding an entry if a `.gitignore` exists, or creating a minimal one if not) a single directory:

```
<project-root>/.mapit/
  graph.sqlite          # the full structural + AI-annotation graph store
  manifest.json         # per-file hash/timestamp manifest for incremental remap
  config.json           # project-local overrides (e.g., extra ignore patterns)
  logs/                 # run logs (structural pass, AI pass), rotated
```

This directory is **never committed by mapit itself to be tracked**, and `mapit` never writes anywhere else inside the user's source tree (PRD §7, read-only principle — read-only means "does not alter source files"; mapit's own metadata directory is the one exception, clearly scoped and `.gitignore`d).

### 7.2 Global config directory
Following OS-appropriate conventions (e.g., `~/.config/mapit/` on Linux via the XDG base directory spec, since the primary target user runs Fedora Linux):

```
~/.config/mapit/
  global_config.json    # default provider/model, default ignore patterns, UI prefs
  credentials.json       # API keys for configured remote providers (file permissions restricted to user-read-only, e.g. 0600)
  projects.json          # list of previously-mapped project paths, for the "switch project" feature (PRD §4.2.10)
```

### 7.3 Config precedence
Project-local `.mapit/config.json` overrides `~/.config/mapit/global_config.json` for any key both define; CLI flags override both for that single invocation. Provider/model selection defaults to the global config but can be overridden per-project.

---

## 8. Progress & Streaming Requirements

Because structural mapping and AI enrichment of a large codebase can take real time, the tool must never appear frozen:

- CLI: a live progress display (current file count / total, current phase name, estimated rate) using a standard terminal progress-bar approach, falling back to plain line-by-line logging when not running in an interactive TTY (e.g., when piped/redirected or run in CI).
- Web: the same progress info pushed over the WebSocket channel, rendered as a progress UI (see App Flow doc) that automatically transitions to the graph view once the structural pass completes (AI enrichment may still be filling in annotations live in the background — partial annotation states must render cleanly, e.g., "AI summary pending" rather than blank/broken UI).

---

## 9. Error Handling Principles

- Every failure mode must degrade gracefully and be visible to the user, never silent and never a raw panic/stack trace in normal use:
  - A file that fails to parse (e.g., genuinely malformed source) is recorded as `parse_failed` for that file with the underlying error, and mapping continues for all other files.
  - A provider/model that is unreachable (Ollama not running, remote API down, bad API key) surfaces a clear, specific error at the point of first AI call, with a suggested fix (e.g., "Ollama doesn't appear to be running — start it with `ollama serve`, or switch provider with `mapit config set-provider`"), and the structural map remains fully usable without AI annotation in the meantime.
  - Port-in-use, permission errors on `.mapit/` creation, and similar environment issues get specific, actionable messages — never a generic crash.
- `mapit` must never crash the user's terminal session or leave orphaned background processes; the embedded server must shut down cleanly on Ctrl+C, cleaning up its port binding.

---

## 10. Security & Privacy Requirements

- No telemetry, no analytics, no "phone home" of any kind in v1. This is not just a preference but a hard requirement consistent with the local-first principle in PRD §7.
- Remote provider calls only ever send: the minimum code/context needed for the specific prompt task (function body + immediate structural context, not the entire codebase in one blob), never the user's API key in logs, never the full file tree unless that specific feature (classification) genuinely requires file-path/name context (which it does, by design — this must be disclosed to the user in the setup wizard for any non-Ollama provider, per PRD §4.1.7/§7).
- The local web server binds to localhost only by default, with no authentication needed for v1 (single local user, single local machine threat model) — but the implementer must not expose it on `0.0.0.0` by default under any circumstance.
- Credentials file permissions restricted at creation time (§7.2).

---

## 11. Extensibility Hooks (for the open questions in PRD §8, not built in v1)

To avoid foreclosing future work without over-building now:

- The `AiProvider` trait (§5.1) is the only integration point for new providers — no provider-specific code should leak into `mapit-core` or the server layer.
- The language adapter trait (§4.1) is the only integration point for new languages.
- The `ControlFlowGraph` representation (§3.2) is intentionally a clean intermediate structure so a future dynamic-tracing data source could, in principle, populate the same structure from runtime data rather than static analysis, without requiring a UI rewrite — this is a design note for future-proofing, not a v1 deliverable.
