# mapit — AI-Powered Interactive Codebase Mapper

Run `mapit` inside any project folder. It parses the entire codebase (tree-sitter, not AI-hallucinated), builds a true call/dependency graph and execution-order model, and shows it as an interactive 3D web graph.

## Quick start

```bash
# Build everything (one command):
cargo build --release

# Run in the current folder:
./target/release/mapit
```

This builds the Rust binary + web UI in one step and launches the interactive map.

On first run, `mapit` prompts for AI provider setup (can skip). Structural mapping (files, symbols, call edges) works without any AI provider.

## Requirements

- **Rust** 1.75+ (for building the binary)
- **Node.js** 18+ (for the web UI — auto-built via `build.rs`)

## CLI commands

| Command | What it does |
|---|---|
| `mapit` | Full flow: init (if first run) → map → open browser |
| `mapit init` | Set up AI provider without mapping yet |
| `mapit map` | Structural mapping only (no browser) |
| `mapit map --force` | Force full re-map (ignore cache) |
| `mapit annotate` | Run AI enrichment against existing map |
| `mapit open` | Start web server without re-mapping |
| `mapit status` | Print summary: files, symbols, edges, coverage |
| `mapit find <name>` | Search symbols by name |
| `mapit explain <name>` | Show signature, callers, callees, summary |
| `mapit trace <name> [--depth N]` | Print execution trace from an entry point |
| `mapit flaws [--severity high|warning|info]` | List AI-flagged issues |
| `mapit ask "<question>"` | Ask about the codebase |
| `mapit config show` | Show current config |
| `mapit config set-provider <ollama|openai-compatible>` | Switch AI provider |
| `mapit config set-model <model>` | Change AI model |

## Architecture

```
mapit/
  crates/
    mapit-core/    — walker, language adapters, graph builder, SQLite store, CFG
    mapit-ai/      — AI provider trait, Ollama + OpenAI-compatible, tasks
    mapit-server/  — REST + WebSocket API, rust-embed web assets
    mapit-cli/     — binary entry point, all subcommands
  web/
    mapit-web/     — React + TypeScript + Tailwind, 3D force graph + ReactFlow
  docs/            — Full specification documents (AGENTS.md-driven build)
```

- **6 language adapters:** Rust, C, C++, assembly, Python, JavaScript/TypeScript
- **Storage:** SQLite via `rusqlite`
- **Web UI:** Vite + React 19 + react-force-graph-3d + ReactFlow
- **AI providers:** Ollama (local, default) or any OpenAI-compatible API

## Development

```bash
# Build just the web UI (for iteration):
cd web/mapit-web && npm run build

# Build the Rust binary (auto-rebuilds web if dist/ missing):
cargo build --release

# Run tests:
cargo test --test phase1_integration
cargo test --test phase2_integration
cargo test --test phase3_integration
cargo test --test phase4_integration
cargo test --test phase5_integration
```

All 37 integration tests pass: parsing, graph building, incremental remap, CLI queries, AI task round-trip.

## License

Proprietary — internal tool.
