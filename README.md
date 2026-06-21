# `mapit` — Project Documentation Set

This folder contains the complete planning documentation for **`mapit`**, an AI-powered, interactive codebase mapping tool. These documents are written to be handed directly to an AI coding agent (e.g. an opencode-driven model, Claude Code, or similar) so it can build the entire project end-to-end without needing to guess at architecture, data shapes, or scope.

## Read order

| # | Document | What it covers |
|---|---|---|
| 0 | [`AGENTS.md`](AGENTS.md) | **Strict rulebook for any coding agent.** Hard "never" rules, phase order, repo layout, compaction-recovery steps. Read this first, every session. |
| 1 | [`01-PRD.md`](docs/01-PRD.md) | What `mapit` is, who it's for, problem statement, goals, success criteria, explicit non-goals |
| 2 | [`02-TRD.md`](docs/02-TRD.md) | Full technical architecture: components, tech stack, language adapters, AI provider abstraction, server, storage, security |
| 3 | [`03-graph-data-model.md`](docs/03-graph-data-model.md) | The exact node/edge/flaw data model, SQLite schema, and JSON contract — the single source of truth for "what is a node" |
| 4 | [`04-app-flow.md`](docs/04-app-flow.md) | Every CLI command and every web app screen/state, written out concretely with example output |
| 5 | [`05-backend-schema.md`](docs/05-backend-schema.md) | Exact config file formats and the full REST/WebSocket API contract |
| 6 | [`06-implementation-plan.md`](docs/06-implementation-plan.md) | Phased build order, repository layout, and "done when" criteria per phase — **the execution checklist** |
| 7 | [`07-MASTER-AI-PROMPT.md`](docs/07-MASTER-AI-PROMPT.md) | The single prompt to hand a coding agent to start (or resume) the build, with operating rules |

## One-line summary of the product

Run `mapit` inside any project folder, of any size, in any language(s). It parses the entire codebase for real (tree-sitter — never AI-hallucinated structure), builds a true call/dependency graph and execution-order model, uses a configurable AI provider (Ollama by default and local, or any OpenAI-compatible remote/free provider like OpenRouter) to classify the code into features, explain it, and flag dead code/flaws/bugs — then shows the user essential progress and quick answers in the terminal, and the full interactive, zoomable, traceable graph in a local web app it opens automatically.

## How to use this doc set

Place `AGENTS.md` at the repository root — most agentic coding tools (opencode, Claude Code, etc.) scan for it automatically and treat it as standing instructions for every session. It is short and imperative by design, with hard rules and a strict phase order, and it points into the rest of `docs/` only when deeper "why" context is needed. Give the agent `docs/07-MASTER-AI-PROMPT.md` as its initial task prompt for the very first session; after that, `AGENTS.md` alone is enough to keep it on track across every future session, including after context compaction.
