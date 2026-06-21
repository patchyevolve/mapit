# App Flow Document
## Project: `mapit`

**Document version:** 1.0
**Depends on:** 01-PRD.md, 02-TRD.md, 03-graph-data-model.md
**Audience:** AI coding agent (implementer)

This document walks through every user-facing flow, command, and screen state. It is the contract for "what should this actually feel like to use" — implement exactly these flows; if a flow seems to need a screen/command not listed here, add it to this document first.

---

## 1. CLI Command Reference (complete v1 surface)

| Command | Behavior |
|---|---|
| `mapit` | The main entry point. If no map exists yet for this folder, runs first-time setup (see §2) then a full map, then opens the web app. If a map exists, runs an incremental re-map (fast, silent if nothing changed) then opens the web app. |
| `mapit init` | Runs first-time setup explicitly without mapping or opening anything yet — for users who want to configure before the first (possibly long) full map. |
| `mapit map` | Runs (or re-runs) the structural mapping pass only, with live CLI progress. Does not open the browser. Useful for CI/headless use or pre-warming a map before later opening it. |
| `mapit map --force` | Forces a full re-map of every file, ignoring the incremental manifest (TRD §3.3). |
| `mapit annotate` | Runs (or resumes) the AI enrichment pass only, against the most recent structural map. |
| `mapit annotate --all --force` | Re-runs AI enrichment for every node regardless of whether it changed (TRD §5.4) — used after switching to a different/stronger model. |
| `mapit open` | Opens the web app for the current project without re-mapping (fails with a clear message if no map exists yet, suggesting `mapit map` first). |
| `mapit status` | Prints a concise terminal summary: file/symbol/edge counts, last map time, AI annotation coverage %, current provider/model, count of flagged flaws by severity. |
| `mapit find <name>` | Quick terminal symbol search; prints matching nodes with file:line and a one-line AI summary if available. |
| `mapit explain <name>` | Prints the full AI summary, signature, callers, and callees for a single matched symbol directly in the terminal — no browser needed for a quick lookup. |
| `mapit trace <name> [--depth N]` | Prints a textual execution-order trace from the given entry point in the terminal (the CLI-only counterpart to the web UI's visual trace mode), default depth reasonable (e.g., 6) and overridable. |
| `mapit flaws [--severity high\|warning\|info]` | Lists AI-flagged flaws in the terminal, optionally filtered by severity. |
| `mapit config show` | Prints current provider, model, base URL (key redacted), and ignore patterns. |
| `mapit config set-provider <ollama\|openai-compatible>` | Switches provider; if switching to `openai-compatible`, interactively prompts for base URL + API key + model unless provided via flags. |
| `mapit config set-model <model-name>` | Changes the model for the currently selected provider. |
| `mapit projects list` | Lists previously mapped project paths (global config, TRD §7.2) for quick switching. |
| `mapit ask "<question>"` | Terminal counterpart to the web UI's "ask about the codebase" feature; prints a grounded answer and the referenced file:line locations. |
| `mapit --help` / `mapit <command> --help` | Standard `clap`-generated help. |

All commands accept a `--path <dir>` flag to target a folder other than the current working directory; default is `.`.

---

## 2. First-Run Setup Flow (`mapit init`, or auto-triggered by first `mapit`)

This is the flow that satisfies PRD §4.1.7 — "initial setup requires API key of any provider and selection of any model."

**Step 1 — Welcome.** Terminal prints a short welcome and explains in 2–3 lines what's about to happen (provider setup, then a one-time full scan).

**Step 2 — Provider selection (interactive prompt).**
```
Which AI provider would you like to use for codebase analysis?

  > 1. Ollama (local, private, free — recommended if installed)
    2. OpenRouter (remote, free + paid models available)
    3. Opencode-hosted endpoint
    4. Other OpenAI-compatible endpoint (custom)
```

- **If Ollama selected:** the tool checks if Ollama is reachable locally (`GET http://localhost:11434`). 
  - If reachable: calls `list_models()` (TRD §5.1) and shows the user's already-pulled models as a selectable list, plus an option "pull a new model" which shells out to `ollama pull <name>` with a name the user types (with a few suggested model names shown as examples, e.g. general-purpose code-capable models — the tool does not hardcode a single "blessed" model, since Ollama's available model list changes over time).
  - If not reachable: prints clear instructions to install/start Ollama, with a link, and offers to fall back to selecting a different provider instead, or to skip AI setup entirely for now (structural mapping works without any AI provider — see §2 Step 4 note).
- **If OpenRouter / Opencode / Other selected:** prompts for:
  1. Base URL (pre-filled with the known preset for OpenRouter/Opencode; free-text for "Other").
  2. API key (input hidden/masked in terminal).
  3. Model name (free text; tool offers to call the endpoint's model-list if it exposes one, else just accepts the typed name).
  - Tool makes one lightweight test call (a minimal "ping" completion) to confirm the key/URL/model work together, and reports clearly if it fails (e.g., "401 Unauthorized — check your API key") rather than silently accepting bad config.

**Step 3 — Privacy notice (only shown for non-Ollama providers).** A short, explicit statement of what will be sent to the remote provider (function bodies + structural context for the prompts described in TRD §5.2 — not the entire repository at once), consistent with TRD §10. User confirms to proceed.

**Step 4 — Confirm and save.** Config is written to `~/.config/mapit/global_config.json` and `~/.config/mapit/credentials.json` (TRD §7.2). Tool states clearly: *"You can change this anytime with `mapit config set-provider` / `mapit config set-model`, or skip AI and just use structural mapping by choosing 'skip' here."* A "skip AI setup for now" option must always be available at Step 2 — the structural graph (calls, includes, files) is fully useful on its own per TRD §3.4, and AI enrichment can be added later with `mapit annotate` once a provider is configured.

**Step 5 — Proceed to mapping** (if invoked via plain `mapit`, not standalone `mapit init`).

---

## 3. Mapping Flow (CLI experience)

```
$ mapit
Scanning project structure...
  12,408 files found · applying .gitignore and default excludes
  9,732 source files to analyze across 5 languages: c, asm, python, javascript, markdown(unparsed)

Structural mapping  [████████████████████░░░░░░░░░░]  68%   6,612 / 9,732 files   (rust, c)
  current: drivers/net/e1000.c

✓ Structural mapping complete in 3m 42s
  9,540 files parsed successfully · 192 unsupported/unparsed · 0 fatal errors
  41,203 symbols found · 118,930 structural edges

Starting AI enrichment with provider: ollama (model: qwen2.5-coder:7b)
  This runs in the background — you can open the map now and summaries will fill in live.

AI enrichment  [████░░░░░░░░░░░░░░░░░░░░░░░░░░░░]  12%   4,980 / 41,203 symbols

Opening map in your browser: http://127.0.0.1:7780
```

- Progress bars only render when stdout is an interactive TTY; otherwise the tool prints periodic plain-text status lines (TRD §8), suitable for logs/CI.
- The browser opens as soon as structural mapping completes — the user is never blocked waiting for full AI enrichment, per TRD §3.4's "must never block" requirement. The web UI's own state machine (§5 below) handles showing the partially-annotated graph correctly.
- If a previous map exists, this becomes the much shorter incremental flow:

```
$ mapit
Checking for changes since last map (2 days ago)...
  3 files changed · 1 file added · 0 deleted

Structural mapping  [████████████████████████████████]  100%   4 / 4 files

✓ Structural map updated in 1.8s
Re-annotating 7 affected symbols with ollama (qwen2.5-coder:7b)...
✓ Done.

Opening map in your browser: http://127.0.0.1:7780
```

---

## 4. Web App — Screen-by-Screen Flow

### 4.1 Screen: Loading / Connecting
Shown the instant the browser opens, before the first API response arrives. Simple centered spinner + "Connecting to mapit..." If structural mapping is still running (user opened the browser manually mid-scan via `mapit open` racing a background `mapit map`), this screen instead shows a live progress view fed by the WebSocket event stream (TRD §6.1/§8) — file count, current phase, current file — visually consistent with the CLI's progress bar, then auto-transitions to §4.2 the moment structural mapping completes.

### 4.2 Screen: System Overview (default landing view)
This is the **feature-level, collapsed-by-default** view required by TRD §6.3's progressive-disclosure mandate.

- Central canvas: a force-directed graph where each node is a **feature/subsystem** (`FeatureNode`, data model §1.1), sized by member count, colored by a stable palette (consistent color per feature across sessions, keyed off feature name hash — not random each load).
- Edges between feature nodes are aggregated (if any function in feature A calls any function in feature B, draw one edge A→B, with edge thickness scaled by how many underlying calls it represents).
- Top bar: project name/path, last-mapped time, search box (global search across all node types, TRD §6.2 `/api/graph/search`), a "Flaws" badge showing total count by severity (click → §4.6), provider/model indicator (click → §4.7 settings), and a "Re-map" button (→ triggers `/api/remap`, shows live progress overlay).
- Clicking a feature node **expands it in place**: it morphs into its member file nodes (still within the same canvas, smooth transition), with intra-feature edges now shown at file-level (`includes`/`links_into`), and the feature becomes a faint bounding boundary/label around its files rather than disappearing — preserving spatial context (PRD §4.1.5 "easily understand the codebase from start to end").
- Clicking a file node expands further into its function/type/macro/global nodes (now switching, per TRD §6.3, to the `reactflow`-based focused renderer for readability at this granularity, since a single file's internal call structure benefits from precise layout over force-physics).
- A persistent breadcrumb (System → Feature: Networking → File: e1000.c) lets the user collapse back up at any level instantly.

### 4.3 Screen / Mode: Function Detail Panel
Clicking any single function node (at any zoom level) opens a side panel (canvas stays visible/interactive behind it) showing:
- Signature, file:line, language.
- AI summary (or "Summary pending..." with a subtle loading indicator if `ai_summary_status == "pending"`, which live-updates via WebSocket the moment it's ready — no manual refresh needed).
- Direct callers list and direct callees list, each row clickable to jump the panel to that function, and each also has a small "highlight on graph" icon that pulses that node on the canvas without losing the current panel.
- Any `FlawFlag`s attached to this node, shown inline with severity coloring and the AI-generated description.
- Two action buttons: **"Trace from here"** (→ §4.4) and **"Show full call tree"** (expands canvas to a "both directions, depth N" neighbor view per `/api/graph/neighbors`, with a depth slider, default depth 3, adjustable live).

### 4.4 Mode: Execution Order / Trace View
Triggered from a function's detail panel ("Trace from here") or from a top-level "Trace an entry point" search action.
- Canvas switches to a **vertically/sequentially laid out** branching diagram (not the free-form force graph) generated from `/api/graph/trace/:id` (data model §5) — numbered steps down the page, branch points clearly forked into labeled parallel paths (per data model §5's "Path A / Path B" requirement), each step showing the function name and, on hover, its one-line AI summary.
- A depth control limits how far the trace expands by default (avoiding an overwhelming wall on deeply recursive code), with a clear "expand further" affordance at the cut-off point rather than silently truncating.
- Conditional edges show their extracted condition text inline on the connecting line (e.g., the `condition` field from data model §2) so the user can see *why* a path forks, directly satisfying PRD §1's "what are the work order when that function is called when other is called."
- This is the direct visual realization of PRD's central ask: tracing a function's life cycle through the system.

### 4.5 Mode: "Trace this function" callers/callees radial view
A secondary, simpler trace mode available from the detail panel: a radial layout with the selected function at center, callers fanning out to one side and callees to the other, expandable ring by ring (depth 1, 2, 3...) — this serves the PRD §1 ask of "trace any function under the particular feature or sub unit" when the user wants neighborhood context rather than a full sequential execution path.

### 4.6 Screen: Flaws / Issues Report
Accessible from the top-bar "Flaws" badge.
- A filterable, sortable list (by severity, by kind, by feature) of every `FlawFlag` in the graph, each row showing description, confidence, and basis (`structural`, `ai`, or `structural+ai` per data model §3).
- Each row links directly back into the graph view, jumping to and highlighting the relevant node(s) — this list is never a dead-end disconnected report; it's a navigation surface into the same graph, consistent with TRD §6.3's "never a separate disconnected report-only view" requirement.
- A clear, persistent disclaimer banner at the top of this screen: flaws are AI-assisted heuristics, not guaranteed facts — directly reflecting PRD §6's success-criteria framing ("plausible and useful... never as guaranteed facts").

### 4.7 Screen: Settings
- Provider/model selection (mirrors `mapit config set-provider`/`set-model`, same underlying `/api/config` endpoint) — changing it here takes effect for the next `mapit annotate` run; an inline note explains existing AI annotations are kept until the user explicitly re-runs enrichment (TRD §5.4).
- Ignore-pattern editor (project-local `.mapitignore` equivalent of the structural excludes).
- A "Re-annotate everything with current model" button (maps to `mapit annotate --all --force`), with a confirmation step since this can be slow/costly on a remote provider.

### 4.8 Mode: Ask the Codebase
A persistent, lightweight chat-style input (e.g., docked at the bottom of the screen, collapsible) available from any screen.
- User types a free-form question; calls `/api/ask` (TRD §6.2); answer renders in a small panel with the answer text plus a list of "based on" node chips that, when clicked, jump into the graph at that node (grounding requirement from TRD §5.2 item 4 — the UI must always show its sources, never present an answer as if it came from nowhere).

---

## 5. Frontend State Machine (high-level)

```
INITIAL_CONNECTING
  → (structural map missing or running) → SHOWING_MAP_PROGRESS
  → (structural map ready) → SYSTEM_OVERVIEW

SHOWING_MAP_PROGRESS
  → (structural phase complete, via WS event) → SYSTEM_OVERVIEW
     [AI enrichment may continue in background; individual nodes show
      ai_summary_status transitioning "pending" → "ready" live via WS,
      handled by per-node state, not a global blocking screen]

SYSTEM_OVERVIEW
  → click feature node → EXPANDED_FEATURE (same screen, in-place expansion)
  → click file node (within expanded feature) → EXPANDED_FILE
  → click function node (any level) → FUNCTION_DETAIL_PANEL (overlay, canvas state preserved underneath)
  → click "Flaws" badge → FLAWS_REPORT
  → click settings icon → SETTINGS
  → use search → jumps directly to the matched node at the correct expansion depth, auto-expanding ancestors as needed

FUNCTION_DETAIL_PANEL
  → "Trace from here" → TRACE_VIEW
  → "Show full call tree" → NEIGHBOR_EXPANSION_VIEW
  → close panel → returns to whatever SYSTEM_OVERVIEW/EXPANDED_* state was active underneath

TRACE_VIEW / NEIGHBOR_EXPANSION_VIEW / FLAWS_REPORT / SETTINGS
  → breadcrumb / back action → returns to SYSTEM_OVERVIEW at the last collapsed/expanded state (state is preserved, not reset, so the user never loses their place)
```

Re-map progress (triggered by the top-bar "Re-map" button, §4.2) overlays the current screen non-destructively — it must not force the user back to `INITIAL_CONNECTING`; the existing graph stays visible/interactive (slightly dimmed) under a progress toast until the incremental update completes, then nodes that changed visually pulse once to indicate "this was just updated."

---

## 6. Empty / Edge-Case States (must be explicitly handled, not left to "whatever happens")

- **Empty folder / no recognized source files:** clear message, no crash, suggests checking the path or `.mapitignore`.
- **AI provider not configured at all (user chose "skip" in setup):** System Overview still renders fully from structural data; `ai_summary` fields show "No AI summary — configure a provider with `mapit config set-provider` to enable explanations and flaw detection" instead of a blank or loading state.
- **A function with zero callers AND zero callees** (fully isolated): rendered normally, not specially hidden, but naturally a leaf with no edges — still searchable and clickable.
- **Extremely large single file** (e.g., a generated file with thousands of functions): file-level expansion for such a file defaults to a paginated/searchable list view instead of attempting to force-render thousands of nodes in one view, with a clear note why ("This file has 3,402 functions — showing search/list view instead of a full graph for readability").
- **Provider call failures mid-enrichment:** affected nodes show `ai_summary_status: "unavailable"` with a small retry affordance in the detail panel, not a silent gap.
