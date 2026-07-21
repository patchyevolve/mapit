# mapit — bro just run it and it maps your whole project

```
                    __  __    _    ____ ___ ___
                   |  \/  |  / \  |  _ \_ _/ _ \
                   | |\/| | / _ \ | |_) | | | | |
                   | |  | |/ ___ \|  __/| | |_| |
                   |_|  |_/_/   \_\_|  |___\___/
```

so basically you just run `mapit` in any project folder and it figures out everything — what calls what, which files depend on what, even how functions flow into each other at runtime. no bs, purely built on tree-sitter so the graph is actually real, not AI hallucinating stuff.

## Quick start

```bash
# Run in any project directory:
mapit
```

first time you run it, it shows a cool splash screen, asks if you wanna set up AI (skip if you want — mapping works without it), then just goes brrr — parses everything, opens a browser with a live graph, and drops you into an interactive terminal where you can type commands and stuff.

structural mapping (files, symbols, call edges) works **without any AI provider**. AI is only needed if you want summaries and flaw detection.

## Features

### 🗺 Structural mapping
- Parses source files with tree-sitter (Rust, C, C++, assembly, Python, JavaScript/TypeScript)
- Builds a complete call graph, include graph, define/reference edges
- Incremental re-mapping — only re-parses changed files
- Control-flow extraction per function (blocks, branches, loops)

### 🤖 AI enrichment
- Batch summarization by file (1 AI call per file, not per function → 11× fewer calls)
- Cross-file context: caller summaries injected into each batch prompt, callee-first ordering
- Project overview phase: single AI call describing the whole system before per-file work
- Flaw detection (dead code, circular deps, structural smells, suspected bugs, etc.)
- Structural dead-code gate: AI never flags dead code without `has_incoming_calls == false`
- Skip AI entirely with `mapit annotate --no-flaws`

### 🎬 Execution simulation
Animated DFS through the static call graph — no AI involved, purely structural:

| Scope | Where to trigger | What it does |
|---|---|---|
| **Function** | Function Detail panel → "Simulate execution from here" | DFS from one function |
| **File** | File view header → "Simulate file" | DFS from every function in that file |
| **Subsystem** | Feature/subsystem view → "Simulate subsystem" | DFS from every function in a feature group |
| **Module** | File browser directory header → "Simulate" | DFS from every function under a directory |
| **Project** | System Overview stats bar → "Simulate project" | DFS from all entry-point candidates |

### 🖥 Interactive CLI

```
                    mapit> help

  Commands  (connected to http://127.0.0.1:7780)
  ─────────────────────────────────────────────
  annotate   – Run AI enrichment (summaries + flaws)
  remap       – Re-run structural mapping
  status      – Show project stats
  flaws       – List AI-detected flaws
  search <q>  – Search symbols
  open        – Open web UI in browser
  help        – Show this help
  exit        – Stop server and quit
```

## CLI reference

| Command | What it does |
|---|---|
| `mapit` | Splash → map → server → browser → interactive CLI |
| `mapit init` | Set up AI provider without mapping |
| `mapit map` | Structural mapping only |
| `mapit map --force` | Force full re-map (ignore cache) |
| `mapit annotate` | Run AI enrichment against existing map |
| `mapit annotate --no-flaws` | Skip flaw-flagging pass |
| `mapit open` | Start web server without re-mapping |
| `mapit status` | Print summary: files, symbols, edges, coverage |
| `mapit find <name>` | Search symbols by name |
| `mapit explain <name>` | Show signature, callers, callees, summary |
| `mapit trace <name> [--depth N]` | Print execution trace from an entry point |
| `mapit flaws [--severity high\|warning\|info]` | List AI-flagged issues |
| `mapit ask "<question>"` | Ask about the codebase (uses AI) |
| `mapit config show` | Show current config |
| `mapit config set-provider <provider>` | Switch AI provider |
| `mapit config set-model <model>` | Change AI model |
| `mapit projects list` | List previously mapped projects |
| `mapit projects remove <path>` | Remove a project from history |
| `mapit simulate <name> [--level function\|file\|module\|project]` | AI textual simulation |

## Example walkthrough

```bash
# 1. Run mapit in a project:
cd ~/my-project
mapit

# Splash screen appears, mapping runs, browser opens.
# You're now in the interactive CLI:

mapit> status
mapit> annotate      # AI enrichment (takes a moment)
mapit> flaws         # list detected issues
mapit> search parse  # find all "parse" symbols
mapit> exit
```

In the web UI:
- Click any function node → opens detail panel with AI summary, callers, callees
- Click "Simulate execution from here" → animated DFS through the call graph
- Browse files in the file browser → open any file → click "Simulate file"
- Navigate to a subsystem view → click "Simulate subsystem"
- Click "Simulate project" in the top bar

## Architecture

```
mapit/
  Cargo.toml
  crates/
    mapit-core/     — walker, language adapters (6 langs), graph builder,
    |                 SQLite store, control-flow extraction
    mapit-ai/       — AI provider trait, Ollama + OpenAI-compatible, prompts
    mapit-server/   — REST + WebSocket API, embeds web UI via rust-embed
    mapit-cli/      — binary entry point, all subcommands, interactive CLI
  web/
    mapit-web/      — React + TypeScript + Tailwind, 3D force graph
  docs/             — Full specification docs (data model, schema, etc.)
```

- **6 language adapters:** Rust, C, C++, assembly, Python, JavaScript/TypeScript
- **Storage:** SQLite via `rusqlite`
- **Web UI:** Vite + React 19 + react-force-graph-2d
- **AI providers:** Ollama (local) or any OpenAI-compatible API

## Requirements

- **macOS** or **Linux** (Windows via MSVC)
- No other runtime dependencies — the binary is statically linked with embedded web UI.

## Install

### macOS / Linux (one-liner)

```bash
curl -sfSL https://raw.githubusercontent.com/patchyevolve/mapit/main/install.sh | sh
```

### Homebrew

```bash
brew install patchyevolve/tap/mapit
```

### Windows (PowerShell)

```powershell
powershell -c "irm https://github.com/patchyevolve/mapit/releases/latest/download/install.ps1 | iex"
```

### From source

```bash
git clone https://github.com/patchyevolve/mapit.git
cd mapit
cargo build --release
./target/release/mapit
```

## Development

```bash
# Build everything (one command):
cargo build --release

# Run in the current folder:
./target/release/mapit

# Run integration tests:
cargo test --test phase1_integration
cargo test --test phase2_integration
cargo test --test phase3_integration
cargo test --test phase4_integration
cargo test --test phase5_integration

# Frontend only (watch mode):
cd web/mapit-web && npm run dev
```

## License

MIT
