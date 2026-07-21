# mapit

```
                    __  __    _    ____ ___ ___
                   |  \/  |  / \  |  _ \_ _/ _ \
                   | |\/| | / _ \ | |_) | | | | |
                   | |  | |/ ___ \|  __/| | |_| |
                   |_|  |_/_/   \_\_|  |___\___/
```

mapit scans your source code, builds a complete call graph using tree-sitter, and lets you explore it through a web UI and an interactive terminal. Think of it like a live map of your codebase — what calls what, which files depend on what, how functions flow into each other.

Works with Rust, C, C++, Python, JavaScript, TypeScript, and assembly.

## Quick start

```bash
cd some-project
mapit
```

It'll walk your source tree, parse everything, build the graph, open a browser at `http://127.0.0.1:7780`, and drop you into an interactive prompt. No database to set up, no server to configure. Just runs.

## What it does

The core feature is structural mapping — parsing source files with tree-sitter and resolving symbols, calls, includes, and references into a queryable graph. All of that works offline with zero configuration.

On top of the graph, there are optional features that use an LLM (bring your own — Ollama, OpenAI, or anything compatible):
- **Summaries** — one-line descriptions for every function, with cross-file context
- **Flaw detection** — flags dead code, circular deps, suspicious patterns, missing error handling
- **Text simulation** — describes the runtime flow of a function or the whole project

The web UI lets you explore the graph visually, and the interactive CLI lets you query things without leaving the terminal.

## Install

**macOS / Linux (one-liner)**
```bash
curl -sfSL https://raw.githubusercontent.com/patchyevolve/mapit/main/install.sh | sh
```

**Homebrew**
```bash
brew install patchyevolve/tap/mapit
```

**Windows (PowerShell)**
```powershell
powershell -c "irm https://github.com/patchyevolve/mapit/releases/latest/download/install.ps1 | iex"
```

**From source**
```bash
git clone https://github.com/patchyevolve/mapit.git
cd mapit
cargo build --release
./target/release/mapit
```

Pre-built binaries are on the [releases page](https://github.com/patchyevolve/mapit/releases) for Linux, macOS (Intel + Apple Silicon), and Windows.

## Walkthrough

```bash
# Point it at a project
cd ~/my-project
mapit

# You'll see the splash screen, parsing happens,
# then a browser opens. Back in the terminal:

mapit> status
Parsed: 143 files, 892 symbols, 1241 call edges, 3403 reference edges

mapit> annotate
# AI enrichment runs (takes a minute depending on project size)

mapit> flaws
Found 12 flaws:
  dead_code unused_helper — function is never called  (src/utils.rs:45)
  missing_error_handling read_config — unwrap on file read  (src/config.rs:30)

mapit> search parse
  parse_config    (src/config.rs:10)
  parse_request   (src/server/handler.rs:55)
  parse_args      (src/cli.rs:22)

mapit> exit
```

## CLI reference

| Command | What it does |
|---|---|
| `mapit` | Map → server → browser → interactive prompt |
| `mapit init` | Configure LLM provider interactively |
| `mapit map` | Structural mapping only |
| `mapit map --force` | Re-map everything from scratch |
| `mapit annotate` | Run AI enrichment (summary + flaws) |
| `mapit annotate --no-flaws` | Skip flaw detection |
| `mapit open` | Start web server without re-mapping |
| `mapit status` | Files, symbols, edges, coverage stats |
| `mapit find <name>` | Search symbols by name |
| `mapit explain <name>` | Signature, callers, callees, summary |
| `mapit trace <name> [--depth N]` | Execution trace from an entry point |
| `mapit flaws [--severity ...]` | List flagged issues (filter by severity) |
| `mapit ask "<question>"` | Free-form question about the codebase |
| `mapit simulate <name> [--level ...]` | Text-based runtime simulation |
| `mapit config show` | Print current config |
| `mapit config set-provider <name>` | Switch LLM provider |
| `mapit config set-model <name>` | Change LLM model |
| `mapit projects list` | Previously mapped projects |
| `mapit projects remove <path>` | Remove from history |

## Interactive CLI

After `mapit` starts, you get a prompt connected to the running server:

```
mapit> help

  Commands  (connected to http://127.0.0.1:7780)
  ─────────────────────────────────────────────
  annotate          Run AI enrichment
  simulate <name>   Text-based simulation
  remap             Re-run structural mapping
  status            Show project stats
  flaws             List detected issues
  search <query>    Search symbols
  open              Open web UI in browser
  help              Show this help
  exit              Stop server and quit
```

## Web UI

The web UI has a few sections:

**Graph view** — force-directed layout of all symbols. Each node is a function, file, or module. You can click any node to see details, pan and zoom around.

**File browser** — tree view on the left. Click a file to see all its functions, their signatures, and AI summaries (if annotated).

**Function detail panel** — shows the signature, list of callers and callees (clickable), AI summary, any flagged flaws, and the control-flow graph (blocks, branches, loops).

**System overview** — lists entry points (main functions, pub exports), groups files by directory into feature clusters, shows project-wide stats.

**Simulation** — click "Simulate" on any function, file, or the whole project. You get an animated DFS traversal. In the browser it's visual; in the CLI via `mapit simulate` it's a text breakdown with steps, inputs, outputs, and error conditions.

**Settings** — configure your LLM endpoint and model from within the UI.

## Architecture

```
mapit/
├── Cargo.toml
├── crates/
│   ├── mapit-core/     — walker, tree-sitter parsers (6 languages), graph builder,
│   │                     SQLite store, control-flow extraction
│   ├── mapit-ai/       — LLM provider trait (Ollama, OpenAI-compatible), prompt templates
│   ├── mapit-server/   — REST + WebSocket API, embeds the frontend
│   └── mapit-cli/      — binary entry point, all subcommands, interactive loop
└── web/
    └── mapit-web/      — React + TypeScript frontend, force-directed graph
```

The entire web UI is compiled into the binary at build time — the server serves it from memory. No separate deployment needed.

Language adapters are standalone per-language modules that implement a shared `LanguageAdapter` trait. Adding a new language means writing a new adapter file that maps tree-sitter CST nodes to the graph schema.

## Requirements

- macOS or Linux (Windows works via MSVC/MSYS2)
- The binary is statically linked with the frontend embedded — nothing else to install

## Development

```bash
# Build everything
cargo build --release

# Run tests (125+ across all crates)
cargo test --release

# Frontend dev server (hot reload)
cd web/mapit-web && npm run dev

# Just the backend
./target/release/mapit
```

The frontend auto-rebuilds when source files change under `web/`. If the built frontend already exists, the Rust build skips the npm step so iteration is fast.

## Built with

- [tree-sitter](https://tree-sitter.github.io/) — parsing for all supported languages
- [rusqlite](https://github.com/rusqlite/rusqlite) — graph storage
- [react-force-graph-2d](https://github.com/vasturiano/react-force-graph-2d) — graph visualization
- [axum](https://github.com/tokio-rs/axum) — web server
- [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) — WebSocket for live progress

## License

MIT
