# mapit — codebase mapper

```
                    __  __    _    ____ ___ ___
                   |  \/  |  / \  |  _ \_ _/ _ \
                   | |\/| | / _ \ | |_) | | | | |
                   | |  | |/ ___ \|  __/| | |_| |
                   |_|  |_/_/   \_\_|  |___\___/
```

mapit parses your project with tree-sitter, builds a full call graph and dependency map, then serves an interactive web UI and CLI for exploring it. Works for Rust, C, C++, Python, JavaScript, TypeScript, and assembly.

## Quick start

```bash
mapit
```

Run it in any project directory. On first launch it walks your source tree, parses every file, resolves symbols and call edges, opens a browser with the interactive graph, and drops you into a terminal prompt for live queries. No separate server or database setup.

## Install

```bash
curl -sfSL https://raw.githubusercontent.com/patchyevolve/mapit/main/install.sh | sh
```

Or via Homebrew:
```bash
brew install patchyevolve/tap/mapit
```

Windows:
```powershell
powershell -c "irm https://github.com/patchyevolve/mapit/releases/latest/download/install.ps1 | iex"
```

From source:
```bash
git clone https://github.com/patchyevolve/mapit.git
cd mapit
cargo build --release
./target/release/mapit
```

## Features

### Structural mapping
- Tree-sitter parsing for Rust, C, C++, assembly, Python, JavaScript/TypeScript
- Call graph, include graph, define/reference edges — all from static analysis
- Incremental re-mapping: only re-parses files whose content changed
- Control-flow extraction per function: blocks, branches, loops

### AI enrichment (optional)
- Batch summarization: one LLM call per file instead of per function
- Cross-file context: caller summaries and project overview injected into prompts
- Flaw detection: dead code, circular dependencies, structural smells, suspected bugs
- Structural dead-code gate: skips functions that still have incoming calls
- Skip AI entirely with `mapit annotate --no-flaws`

### Execution simulation
Animated traversal through the call graph, purely structural (no AI):

| Scope | Trigger | What it does |
|---|---|---|
| **Function** | Function detail panel → "Simulate execution from here" | DFS from one function |
| **File** | File view header → "Simulate file" | DFS from every function in that file |
| **Subsystem** | Feature/subsystem view → "Simulate subsystem" | DFS from every function in a feature group |
| **Module** | File browser directory header → "Simulate" | DFS from every function under a directory |
| **Project** | System overview stats bar → "Simulate project" | DFS from all entry-point candidates |

### AI simulation (`mapit simulate`)
Text-based simulation describing what happens at runtime — entry points, execution steps, data flow, error conditions, and system effects. Scope can be a single function, file, module, or the whole project.

## CLI reference

| Command | What it does |
|---|---|
| `mapit` | Full pipeline: map → server → browser → interactive prompt |
| `mapit init` | Set up LLM provider without mapping |
| `mapit map` | Structural mapping only |
| `mapit map --force` | Full re-map, ignore cache |
| `mapit annotate` | Run AI enrichment against existing map |
| `mapit annotate --no-flaws` | Skip flaw-flagging pass |
| `mapit open` | Start web server without re-mapping |
| `mapit status` | Print summary: files, symbols, edges, coverage |
| `mapit find <name>` | Search symbols by name |
| `mapit explain <name>` | Show signature, callers, callees, summary |
| `mapit trace <name> [--depth N]` | Print execution trace from an entry point |
| `mapit flaws [--severity high\|warning\|info]` | List detected issues |
| `mapit ask "<question>"` | Ask about the codebase |
| `mapit config show` | Show current configuration |
| `mapit config set-provider <provider>` | Switch LLM provider |
| `mapit config set-model <model>` | Change LLM model |
| `mapit projects list` | List previously mapped projects |
| `mapit projects remove <path>` | Remove a project from history |
| `mapit simulate <name> [--level function\|file\|module\|project]` | AI text simulation |

## Interactive CLI

Once the server starts, you get a `mapit>` prompt connected to `http://127.0.0.1:7780`:

```
mapit> help

  Commands  (connected to http://127.0.0.1:7780)
  ─────────────────────────────────────────────
  annotate          Run AI enrichment
  simulate <name>   AI text simulation
  remap             Re-run structural mapping
  status            Show project stats
  flaws             List detected issues
  search <query>    Search symbols
  open              Open web UI in browser
  help              Show this help
  exit              Stop server and quit
```

## Web UI

The web interface is the main way to navigate the mapped project:

- **Force-directed graph** — explore symbols and their connections visually; pan, zoom, click any node
- **File browser** — tree view of the project with function lists per file
- **Function detail panel** — signature, callers, callees, AI summary, flaws, control-flow graph
- **System overview** — entry points, features grouped by directory, project-wide stats
- **Simulation** — animated DFS traversal starting from any function, file, module, or the whole project
- **Settings** — configure your LLM provider and model from within the UI

## Architecture

```
mapit/
  crates/
    mapit-core/     — walker, tree-sitter parsers (6 languages), graph builder,
    |                 SQLite store, control-flow extraction
    mapit-ai/       — LLM provider trait, Ollama + OpenAI-compatible, prompt templates
    mapit-server/   — REST + WebSocket API, serves embedded web UI
    mapit-cli/      — binary entry point and all subcommands
  web/
    mapit-web/      — React + TypeScript frontend with 3D force-directed graph
```

## Requirements

- macOS or Linux (Windows via MSVC/MSYS2)
- Static binary with embedded web UI — no runtime dependencies beyond the OS

## Development

```bash
cargo build --release
cargo test --release

# Frontend watch mode (rebuilds on save):
cd web/mapit-web && npm run dev
```

## License

MIT
