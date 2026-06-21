# Backend & Storage Schema Document
## Project: `mapit`

**Document version:** 1.0
**Depends on:** 02-TRD.md, 03-graph-data-model.md
**Audience:** AI coding agent (implementer)

This document specifies exact file formats and the exact REST/WebSocket contract. The SQLite table schema itself lives in 03-graph-data-model.md §6 and is not repeated here.

---

## 1. Global Config File — `~/.config/mapit/global_config.json`

```json
{
  "schema_version": 1,
  "default_provider": "ollama",
  "default_model": "qwen2.5-coder:7b",
  "ollama_base_url": "http://localhost:11434",
  "ui_preferences": {
    "preferred_port": 7780,
    "theme": "system"
  },
  "default_ignore_patterns": [
    ".git", "node_modules", "target", "dist", "build",
    "*.min.js", "*.lock", "venv", "__pycache__", ".mapit"
  ]
}
```

- `schema_version` must be checked on load; if a future version bumps this, `mapit` must run a defined migration step rather than failing to parse — even in v1, write the version-check scaffolding now so this isn't a breaking retrofit later.
- This file contains **no secrets**. API keys live only in `credentials.json` (§2), kept separate so the (non-secret) config file could safely be shared/dotfiles-synced by a user without leaking a key.

---

## 2. Credentials File — `~/.config/mapit/credentials.json`

```json
{
  "schema_version": 1,
  "providers": {
    "openai-compatible": {
      "base_url": "https://openrouter.ai/api/v1",
      "api_key": "sk-...",
      "model": "..."
    }
  }
}
```

- File must be created with restrictive permissions (`0600` on Unix) at write time, and `mapit` should verify/re-assert these permissions on every write (defensive, in case the user's umask or an editor changed them).
- Keyed by provider id so a user can configure multiple `openai-compatible`-style endpoints with different presets in the future without a schema change (v1 UI/CLI only exposes switching between one active provider at a time per TRD §5.4, but storing as a map rather than a single flat object avoids a migration if multi-provider profiles are added later).
- Never logged. Any error message involving this file must redact the `api_key` value (e.g., show only the last 4 characters).

---

## 3. Known Provider Presets — `~/.config/mapit/provider_presets.json` (bundled default, user-extendable)

This is the "small built-in list of known-good base URL presets" referenced in TRD §5.1. Shipped as a default bundled file, copied into the user's config directory on first run if not already present, and freely user-editable thereafter (so adding a new "free provider" the user discovers later is a one-line JSON edit, never a code change — directly fulfilling the requirement that new OpenAI-compatible providers don't need new adapter code).

```json
{
  "schema_version": 1,
  "presets": [
    {
      "id": "openrouter",
      "label": "OpenRouter",
      "base_url": "https://openrouter.ai/api/v1",
      "notes": "Wide range of free and paid models; requires an OpenRouter API key."
    },
    {
      "id": "opencode",
      "label": "Opencode-hosted endpoint",
      "base_url": "https://opencode.ai/zen/v1",
      "notes": "Confirm current base URL in opencode's docs before relying on this preset; verify against opencode's own documentation if it has changed."
    },
    {
      "id": "ollama-remote",
      "label": "Remote Ollama instance",
      "base_url": "http://<your-remote-host>:11434",
      "notes": "For an Ollama instance running on another machine on your network, not localhost."
    }
  ]
}
```

**Implementer note:** the exact current base URL for any specific third-party preset (especially `opencode`, which the implementer should re-verify rather than assume) should be confirmed against that provider's own current documentation at build time, since these endpoints can change. The architecture must not depend on any single preset value being correct forever — the "Other / custom" option in setup (App Flow §2 Step 2) is the permanent fallback that always works regardless of preset drift.

---

## 4. Project-Local Config — `<project-root>/.mapit/config.json`

```json
{
  "schema_version": 1,
  "extra_ignore_patterns": [],
  "provider_override": null,
  "model_override": null,
  "last_full_map_at": "2026-06-20T10:14:00Z",
  "last_incremental_map_at": "2026-06-22T09:01:12Z"
}
```

- `provider_override`/`model_override` being non-null means this specific project uses a different provider/model than the global default (TRD §7.3 precedence rule).

---

## 5. Manifest File — `<project-root>/.mapit/manifest.json`

```json
{
  "schema_version": 1,
  "files": {
    "src/drivers/net/e1000.c": {
      "content_hash": "sha256:...",
      "language": "c",
      "last_parsed_at": "2026-06-20T10:13:58Z",
      "parse_status": "ok"
    }
  }
}
```

Note: this manifest is also fully reconstructable from the `files_manifest` SQLite table (data model §6) — keeping it as a separate JSON file is a deliberate redundancy for fast incremental-diff checks on startup without opening the SQLite file at all (a cheap file read + JSON parse beats a DB connection for the very first "anything changed?" check on every single `mapit` invocation). The SQLite table remains the authoritative source if the two ever disagree (e.g., a crash mid-write) — on detecting a mismatch, `mapit` rebuilds `manifest.json` from the SQLite table rather than trusting a possibly-stale JSON file.

---

## 6. REST API — Full Request/Response Contract

Base URL: `http://127.0.0.1:<port>/api`

### `GET /api/project`
**Response 200:**
```json
{
  "project_root": "/home/daksh/projects/opertur",
  "last_full_map_at": "2026-06-20T10:14:00Z",
  "last_incremental_map_at": "2026-06-22T09:01:12Z",
  "file_count": 9540,
  "symbol_count": 41203,
  "edge_count": 118930,
  "languages": ["c", "asm", "rust"],
  "provider": "ollama",
  "model": "qwen2.5-coder:7b",
  "ai_annotation_coverage_pct": 87.4
}
```

### `GET /api/graph/node/:id`
**Response 200:** a single node object matching the relevant `*Node` interface from data model §1 (discriminated by `type`). **404** if id not found, with `{ "error": "node_not_found", "id": "..." }`.

### `GET /api/graph/neighbors/:id?direction=callers|callees|both&depth=N`
**Response 200:**
```json
{
  "center_id": "a1b2c3...",
  "nodes": [ /* Node[] */ ],
  "edges": [ /* Edge[] */ ]
}
```
`depth` defaults to 1 if omitted; server enforces a sane max (e.g., 10) to avoid an accidental denial-of-service on a deeply connected graph from a single request — values above the max are clamped, not rejected, and the response indicates the clamped value used.

### `GET /api/graph/trace/:id?max_depth=N`
**Response 200:** the `ControlFlowGraph`-derived branching structure (data model §5), pre-resolved with full node objects inlined at each step (not just IDs) so the frontend can render without N follow-up requests:
```json
{
  "entry_node_id": "...",
  "steps": [
    {
      "block_id": "...",
      "calls": [ { "node": { /* FunctionNode */ }, "order_hint": 0 } ],
      "branches": [
        { "condition": "if (status == ERROR)", "next_block_id": "..." },
        { "condition": null, "next_block_id": "..." }
      ]
    }
  ],
  "truncated_at_depth": false
}
```

### `GET /api/graph/features`
**Response 200:** `{ "features": [ /* FeatureNode[] */ ] }`

### `GET /api/graph/flaws?severity=high|warning|info` (severity optional, omit for all)
**Response 200:** `{ "flaws": [ /* FlawFlag[], each with primary_node_id and a denormalized primary_node_name+file_path for display without a follow-up call */ ] }`

### `GET /api/graph/search?q=<term>&limit=N`
**Response 200:** `{ "results": [ { "node": { /* Node */ }, "match_reason": "name" | "file_path" | "ai_summary" } ] }`, ranked best-match-first.

### `POST /api/ask`
**Request:**
```json
{ "question": "which functions touch the network driver?" }
```
**Response 200:**
```json
{
  "answer": "Three functions directly touch the network driver layer...",
  "referenced_node_ids": ["...", "..."],
  "grounding_status": "ok" | "partial" | "no_relevant_context_found"
}
```
If `grounding_status` is `no_relevant_context_found`, `answer` must explicitly say so rather than letting the model guess freely — this is the enforcement point for TRD §5.2 item 4's grounding requirement.

### `GET /api/config`
**Response 200:** same shape as `mapit config show` (App Flow §1) — provider, model, base_url, redacted key indicator (`"api_key_set": true/false`, never the actual key), ignore patterns.

### `PUT /api/config`
**Request:** any subset of `{ provider, model, base_url, api_key, extra_ignore_patterns }`. **Response 200:** updated config (redacted). On invalid provider/model combination, **400** with a specific `error` field.

### `POST /api/remap`
**Request:** `{ "force": false }` (optional, default false). **Response 202 Accepted** immediately (work proceeds async; progress via WebSocket): `{ "status": "started", "mode": "incremental" | "full" }`.

### `POST /api/annotate`
**Request:** `{ "all": false, "force": false }`. Same async pattern as `/api/remap`.

### `WS /api/events`
Server pushes JSON messages of this shape (discriminated union on `event`):
```json
{ "event": "map_progress", "phase": "structural", "current": 6612, "total": 9732, "current_file": "drivers/net/e1000.c" }
{ "event": "map_progress", "phase": "ai_enrichment", "current": 4980, "total": 41203 }
{ "event": "map_phase_complete", "phase": "structural" }
{ "event": "map_phase_complete", "phase": "ai_enrichment" }
{ "event": "node_updated", "node_id": "...", "fields_changed": ["ai_summary", "ai_summary_status"] }
{ "event": "error", "scope": "file_parse" | "ai_call" | "remap", "message": "...", "detail": "..." }
```
The frontend's WebSocket handler dispatches `node_updated` to whatever local store/cache holds graph nodes (e.g., a normalized client-side store keyed by `id`), so any currently-open detail panel or visible node re-renders live without a full data refetch — this is the mechanism behind App Flow §4.3's "live-updates without manual refresh" requirement.

---

## 7. CLI-to-Server Relationship

The CLI binary and the server are **the same compiled binary** (TRD §1) — `mapit` (no args) starts an in-process `axum` server bound to localhost and, in the same process, kicks off (or resumes) the mapping/annotation pipeline, emitting WebSocket events from the same in-memory progress-tracking state that the CLI's own progress bar renders from. There is no separate "daemon" process to manage in v1: the server's lifetime is tied to the foreground `mapit` process, and it shuts down when the user Ctrl+Cs the terminal (TRD §9). Headless commands (`mapit map`, `mapit annotate`, `mapit find`, etc.) never start the HTTP server at all — only `mapit` (bare), `mapit open`, and any command that explicitly needs live browser interaction does.

This single-process design is a deliberate v1 simplification; a background-daemon mode (so the web UI stays live after the terminal closes) is a reasonable fast-follow but is explicitly **not** required for v1 — note this assumption here so the implementer does not over-build a daemon/service-manager layer prematurely.
