# UI Refinement Document
## Project: `mapit`

**Document version:** 1.0
**Depends on:** 03-graph-data-model.md, 04-app-flow.md, 05-backend-schema.md
**Supersedes:** any visual or interaction decision in the current `web/mapit-web` codebase that conflicts with this document
**Audience:** AI coding agent (implementer)

---

## 0. Why this document exists — read this before touching any code

The current web app works end to end (the server, the data model, the API are all correct), but the frontend **drifted from `04-app-flow.md` during Phase 7** and was never walked screen-by-screen against that spec before being called done. This document exists to (a) name every specific drift precisely, so it cannot be quietly reproduced, and (b) lock the visual design hard enough that a future agent has no room to reinvent it differently in a different file.

**This document is not a request for a redesign of intent.** `04-app-flow.md` remains the authority on *what each screen does and when it appears*. This document is the authority on *exactly how it looks* and *the specific bugs that must be fixed while rebuilding it*. If anything here conflicts with `04-app-flow.md` on flow/behavior (not visuals), `04-app-flow.md` wins and this document is wrong — stop and flag it rather than silently picking one.

**Non-negotiable working rule:** after every screen you touch, manually verify it against the relevant `04-app-flow.md` section number AND against §2 of this document (the defect list) before moving to the next screen. "I rewrote the component" is not done. "I rewrote it and walked through the specific defect list line for this screen and confirmed each one is actually fixed in the running app" is done. This exact failure mode — moving on before verifying — is what produced the current state, and `06-implementation-plan.md` already said as much; it was not followed.

---

## 1. Root causes (fix the cause, not just the symptom)

Three structural mistakes produced almost every visible problem. Fix these first, structurally, before touching individual screens — patching individual screens without fixing these will reproduce the same drift again.

### 1.1 Three color systems exist where one must
- `tailwind.config.js` defines a `mapit.*` color palette.
- `src/index.css` separately defines `--mapit-*` CSS variables with **different hex values** for the same names, and nothing in the app actually reads these variables.
- `SystemOverview.tsx`, `GraphView.tsx`, and `TopBar.tsx` bypass both systems entirely and hardcode a third, different palette (GitHub-dark hex values) directly as literal strings.
- **Fix:** delete the CSS variable block in `index.css` entirely. `tailwind.config.js` is the single source of truth for color. Every single hardcoded hex literal in every `.tsx` file must be replaced with the corresponding `mapit-*` Tailwind class. Grep the entire `src/` tree for any string matching `#[0-9a-fA-F]{3,6}` outside of `tailwind.config.js` after this pass — any match other than a one-off severity color documented in §3.6 is a bug.

### 1.2 Duplicate, diverging component implementations
- `components/SearchBar.tsx` is a complete, correctly-themed, correctly-typed search component. It is never imported anywhere.
- `components/TopBar.tsx` independently defines its own inline `SearchBar` function at the bottom of the file, with its imports placed after first use (works only because of bundler import hoisting — this is fragile and wrong regardless of whether it currently compiles). This inline version is hardcoded-hex-themed and, critically, **always opens the function-detail overlay on selection regardless of the selected node's type** — selecting a file from search results opens a function detail panel for it, which is wrong for a node that isn't a function and has no signature/callers/callees.
- **Fix:** delete the inline `SearchBar` function and its trailing imports from `TopBar.tsx` entirely. `TopBar.tsx` imports and renders `components/SearchBar.tsx` only. Before deleting, fix `components/SearchBar.tsx`'s `handleSelect` so it is exhaustive over every `NodeType`, not just `function` and `file` (see §2.2 for the exact required behavior per type).
- **General rule going forward:** before creating any new component, grep `src/components` and `src/screens` for an existing one with the same name or the same rendered purpose. Two implementations of the same UI concept existing simultaneously is always a bug, even if only one is currently wired up.

### 1.3 Breadcrumb data contract mismatch
- `types.ts` defines `AppState.breadcrumb` as `{ label: string; node_id?: string }[]`.
- `components/Breadcrumb.tsx` correctly reads `cr.node_id`.
- `screens/SystemOverview.tsx` defines its own local `ZoomEntry { label: string; nodeId: string }` (camelCase, no underscore, not optional) and force-casts `state.breadcrumb as ZoomEntry[]` to paper over the mismatch, then writes entries back using the `nodeId` key.
- **Net effect:** every breadcrumb entry pushed by `SystemOverview` has `node_id` permanently `undefined`. `Breadcrumb.tsx`'s `cr.node_id ? <button> : <span>` check therefore always takes the `<span>` branch — breadcrumbs are permanently inert text, never clickable, in every build, regardless of how deep the user has zoomed.
- **Fix:** delete the local `ZoomEntry` interface from `SystemOverview.tsx`. Use the canonical `AppState["breadcrumb"]` type from `types.ts` everywhere a breadcrumb entry is constructed or read, with **no local re-declaration of this shape anywhere**, and no `as` cast bridging two different field names for the same concept. After the fix, clicking any non-terminal breadcrumb entry must collapse the view back to that exact level — verify this manually at 3+ levels of depth, not just one.

---

## 2. Defect list — every item below must be independently verified fixed, by screen

This is written against the current code; an implementer rebuilding from scratch will naturally avoid most of these, but each one is listed because it represents a *behavior* that must exist in the new build, not merely "don't do the old broken thing."

### 2.1 System Overview / graph canvas
- Replace `react-force-graph-3d` with a **2D** force-directed renderer (`d3-force` directly, or `react-force-graph-2d` — implementer's choice of library, but the rendering must be 2D, per `04-app-flow.md` §4.2's "force-directed graph," not the 3D canvas currently shipped). 3D added navigation cost (orbit/pan/zoom in three axes) with no corresponding benefit for a graph that has no meaningful third dimension of data, and is a primary contributor to the UI feeling disorienting.
- `reactflow` is an installed dependency that is **never imported anywhere in `src/`**. Per `04-app-flow.md` §4.2 and `02-TRD.md` §6.3, file-internal expansion (function/type/macro/global nodes within one file) must switch to a `reactflow`-based renderer for precise, readable layout — the force-physics renderer is correct at feature/file granularity but wrong at function-internal granularity, exactly as the spec says. Implement this switch; do not leave both renderers as parallel unused options or pick one for everything.
- The current `viewMode` tabs ("Files" / "Functions" / "Features") have no basis in any spec document and must be removed entirely. They currently stand in for the actual required interaction, which is: the canvas starts at the **feature** level always, and **drills down via direct node interaction** (click a feature → it expands in place into its member files; click a file → it expands further into its functions/types/macros/globals), exactly per `04-app-flow.md` §4.2 and the state machine in §5. There is no independent top-level switch between "files," "functions," and "features" as parallel modes — depth is reached by drilling, not by mode-switching.
- "Expands in place" means what it says: the feature node morphs into its member nodes with a smooth transition, and the collapsed feature remains visible as a faint bounding boundary/label around its now-visible files (per §4.2) — it does not vanish and get replaced by an unrelated neighbor-fetch view. The current implementation calls `/api/graph/neighbors` and swaps the entire node/edge set, discarding the spatial context the spec explicitly calls out as the point of this interaction. Re-implement so the previously-visible nodes remain present (de-emphasized) and the newly-revealed nodes appear nested within/near their parent's former position.
- Initial node layout currently uses `Math.random()` jitter for every render, meaning the graph re-shuffles its layout on every reload with no continuity. Positions should be deterministic given the same graph (e.g., seed from node ID hash, not `Math.random()`), so a user's mental map of "where things are" survives a re-map or reload.
- Stats bar and Flaws badge currently use literal hex colors (see §1.1) — convert to theme tokens.

### 2.2 Search
- Fix per §1.2. Required behavior per node type on selection, replacing the current "always open function detail" behavior:
  - `function`, `type`, `macro`, `global` → open the relevant detail overlay (function detail panel for functions; for v1, `type`/`macro`/`global` may reuse a simplified version of the same detail panel — signature/file:line/AI summary only, no callers/callees section if not applicable).
  - `file` → push a breadcrumb entry and zoom the canvas to that file's expanded view (matching what clicking the file node directly on the canvas does — search must produce the exact same outcome as if the user had navigated there manually, never a different, parallel code path).
  - `feature` → same: zoom the canvas into that feature, matching direct-click behavior exactly.
  - `external` → open a minimal detail view showing the `reason` field; there is no source to navigate to.
- Debounce search input (the current inline version in `TopBar.tsx` does this at 300ms; keep that behavior in the single surviving implementation).

### 2.3 Function Detail Panel
- Fix the conditional-hook-call bug: the current code does `if (!state.overlay || state.overlay.kind !== "function_detail") return null;` **before** its `useEffect` call. Hooks must never be called conditionally — restructure so all hooks run unconditionally on every render of this component, and the early-return guard (if still needed at all) happens after all hook calls, or the component is only ever mounted when the overlay kind is already correct (preferred: let the parent only render `<FunctionDetailPanel/>` when the overlay kind matches, and have the component itself assume `node_id` is present).
- Otherwise this screen is closest to spec; preserve its caller/callee click-to-navigate behavior and inline flaw display.

### 2.4 Trace View
- Current implementation is a flat vertical list of cards with branch info rendered as small indented text. `04-app-flow.md` §4.4 requires an actual **branching diagram**: numbered steps flow down the page, and at a branch point the diagram visually forks into labeled parallel paths (e.g., "Path A" / "Path B" side by side, each continuing its own vertical sequence) rather than collapsing both branches into one card's text. Rebuild this as a true diagram (SVG or an absolutely-positioned layout is acceptable; a third-party diagram library is acceptable if it fits the existing stack) — a sequential card list is not an acceptable substitute for a forking diagram, because the entire point of this screen per the PRD is showing *why* execution can take more than one path.
- Conditional edges must show their `condition` text directly on the connecting line/branch label, inline, not buried in secondary text below the step.
- Keep the default-depth-with-"expand further" affordance described in §4.4 rather than silently truncating.

### 2.5 Neighbor / Call Tree View ("Show full call tree")
- Currently hardcodes `depth=2` with no user control. `04-app-flow.md` §4.3 explicitly requires **a depth slider, default depth 3, adjustable live**. Add the slider; re-fetch `/api/graph/neighbors` (or refilter client-side if the full data is already loaded) as the user drags it, without requiring a separate "apply" click.
- This view currently reuses the same generic graph component as System Overview. That is acceptable for this specific mode (§4.3's "Show full call tree" — a neighbor expansion, not specified as radial). Do not conflate this with §4.5's separate radial mode (see §2.6) — they are two different screens for two different purposes and must not collapse into one.

### 2.6 Radial callers/callees view (§4.5)
- This mode does not currently exist as a distinct screen — it currently falls through to the same generic neighbor view as §2.5. Build it as specified: the selected function centered, callers fanning out to one side, callees fanning out to the other, expandable ring by ring (depth 1, 2, 3...). This is explicitly called out in `06-implementation-plan.md` as its own build item ("radial callers/callees view") and must exist as its own component, not be silently merged into the neighbor-expansion view.

### 2.7 Flaws Report
- Currently renders as static, non-interactive `<div>` rows with no click handler at all — a true dead end, which `04-app-flow.md` §4.6 explicitly forbids ("never a separate disconnected report-only view"). Every row must be clickable and must navigate into the graph, highlighting the relevant node(s) (`related_node_ids` plus `primary_node_id`), exactly as specified.
- Add the missing filter/sort controls: by severity, by kind, by feature. None currently exist.
- Keep the existing disclaimer banner — that part is correct and must be preserved exactly as currently worded (or equivalent), not removed during the rebuild.

### 2.8 Settings
- Provider/model are currently read-only text with an instruction to use the CLI instead. Per `04-app-flow.md` §4.7, this screen must provide **actual selection controls** mirroring `mapit config set-provider`/`set-model` via the same `/api/config` endpoint — not just a display of current value with a CLI command suggested as the only way to change it.
- "Re-annotate everything" currently fires immediately on click with no confirmation. §4.7 explicitly requires **a confirmation step since this can be slow/costly on a remote provider**. Add a confirmation dialog (see §3.7 for its visual spec) before the request fires.
- Add the missing ignore-pattern editor (project-local `.mapitignore` equivalent) — currently absent entirely.

### 2.9 Ask the Codebase
- Currently discards `referenced_node_ids` from every response entirely — only `answer` and `grounding_status` are kept. `04-app-flow.md` §4.8 requires the answer to render with **"based on" node chips** that jump into the graph when clicked. This is the panel's core trust mechanism per the spec ("the UI must always show its sources, never present an answer as if it came from nowhere") and is currently fully absent from the implementation, not partially implemented. Add it: after each answer, render one small chip per referenced node ID (resolve the name from `state.allNodes`), each clickable, each navigating to that node exactly like search selection does (§2.2).
- Render `grounding_status` as a small visual indicator (see §3.8), not as a bracketed prefix string concatenated into the answer text (current: `` `[${res.grounding_status}] ${res.answer}` ``) — the status is metadata, not part of the answer's prose, and should never be literally embedded in the displayed sentence.

### 2.10 Re-map behavior (cross-cutting, not one screen)
- Currently, triggering a re-map causes a WebSocket-driven full `SET_SCREEN: "map_progress"` transition, which unmounts `SystemOverview` entirely — the user's current zoom depth, breadcrumb, and scroll/pan position are all lost, and they're dropped onto a centered full-screen spinner. `04-app-flow.md` §5 explicitly forbids this: re-map progress must overlay the current screen non-destructively, the existing graph stays visible (dimmed) underneath a progress toast, and nodes that changed should pulse once on completion — the user must never be forced back to a connecting-style screen by a re-map they triggered from an already-loaded view.
- Fix: re-map progress becomes a toast/overlay state that coexists with whatever screen is currently showing, not a `screen` transition. This likely requires adding a new piece of state (e.g., `remapProgress` on `AppState`, separate from the initial-load `mapProgress`/`map_progress` screen, which legitimately is a full-screen state only for the very first map when there is nothing to show underneath yet).

---

## 3. Visual design system — exact and binding

This section is intentionally prescriptive. Do not introduce a color, spacing value, or component pattern not described here. If a new situation arises that isn't covered, extend this document explicitly (add the missing rule here, in writing) before writing the code that needs it — the same rule `03-graph-data-model.md` already applies to data shapes applies here to visual ones.

### 3.1 Color — single source of truth
Replace the current `tailwind.config.js` palette with the table below. This is now the **only** place color is defined. Delete the CSS variable block in `index.css` (§1.1). This table is the complete, binding spec — every row becomes a `mapit-*` Tailwind color token, and `tailwind.config.js` must contain exactly these tokens and no others.

| Token | Hex | Use |
|---|---|---|
| `mapit-bg` | `#0b0d12` | App background, graph canvas background |
| `mapit-surface` | `#14171f` | TopBar, panel backgrounds, overlay backgrounds |
| `mapit-surface2` | `#1b1f2a` | Nested cards/rows inside a panel (e.g. a flaw row, a trace step card) |
| `mapit-border` | `#262b38` | All borders and dividers, no exceptions, no other border color |
| `mapit-text` | `#e8eaf0` | Primary text |
| `mapit-muted` | `#8b91a3` | Secondary text, placeholders, captions, disabled text |
| `mapit-accent` | `#5b8def` | Primary actionable color: active states, primary buttons, links, focus rings, the accent gradient in the logotype |
| `mapit-success` | `#3ecf8e` | Success states, "ready" status, low-severity flaw badges |
| `mapit-warning` | `#e0a440` | Warning severity, "pending" status |
| `mapit-danger` | `#e5566d` | High severity, errors, destructive-action confirmation |

Node-type colors on the graph canvas (these are semantic, not arbitrary — keep them visually distinct from each other and from the five tokens above):

| Node type | Hex | Token name |
|---|---|---|
| `feature` | `#5b8def` (same as accent — features are the "home" level) | `mapit-node-feature` |
| `file` | `#3ecf8e` | `mapit-node-file` |
| `function` | `#e5566d` | `mapit-node-function` |
| `module` | `#a684e8` | `mapit-node-module` |
| `type` | `#e0a440` | `mapit-node-type` |
| `macro` | `#c792ea` | `mapit-node-macro` |
| `global` | `#4fc3d9` | `mapit-node-global` |
| `external` | `#5c6577` | `mapit-node-external` |

Add all of the above (the corrected table, not the malformed placeholder) as `mapit.*` and `mapit.node.*` entries in `tailwind.config.js`. **Every** color used anywhere in `src/` must resolve to one of these tokens via a Tailwind class (e.g. `bg-mapit-surface`, `text-mapit-accent`, `border-mapit-border`). A literal hex string in a `.tsx` file's `className` or inline `style` is a defect, with the sole exception of the per-node-type colors inside `GraphView.tsx`'s node-rendering logic, which may reference the table above as a plain JS object (since Tailwind classes can't be dynamically computed inside canvas/SVG drawing code) — but that object's values must literally match this table, not drift from it.

### 3.2 Typography
- Font stack: keep the current system-font stack in `index.css` (`-apple-system, BlinkMacSystemFont, 'Segoe UI', ...`) — this is correct, do not change it.
- Monospace (for signatures, file paths, code-like content): add `font-mono` usages consistently using Tailwind's default mono stack — currently some screens use `font-mono` (FunctionDetailPanel signature) and others don't where they should (file paths in search results, flaw `file_path` rows). Apply `font-mono` to: function signatures, file:line references, node IDs, condition expressions in trace view. Do not apply it to prose (AI summaries, descriptions, button labels).
- Scale: `text-xs` (12px) for metadata/captions/badges, `text-sm` (14px) for body content and most UI text, `text-lg` (18px) for panel/screen titles only. Do not introduce `text-base`, `text-xl`, or larger anywhere in the app shell — this is a dense information tool, not a marketing page, and the current mix is already close to this; keep it disciplined as new screens are added.

### 3.3 Spacing
- Use Tailwind's default spacing scale exclusively (the `4px` step scale: `1`=4px, `2`=8px, `3`=12px, `4`=16px, etc.) — never an arbitrary value like `px-[13px]`.
- Panel/screen header bars: `px-4 py-2` (matches the current convention already used by most overlay screens — keep it, and bring `TopBar.tsx` in line with it instead of its current `px-6 py-4`, so every header bar in the app is the same height).
- Card/row internal padding: `p-3` for compact rows (flaw entries, search results, trace step cards), `p-4` for panels with more breathing room (function detail panel body).
- Gaps between sibling interactive elements (buttons in a toolbar, badges in a stats row): `gap-2` (8px) as the default; `gap-3` (12px) only where elements are visually larger/heavier (e.g., the stats bar pills on System Overview).

### 3.4 Component states — required for every interactive element
Every clickable element (buttons, rows, node items) must visibly implement all of the following states, using only tokens from §3.1:
- **Default:** `mapit-surface` or `mapit-surface2` background (context-dependent), `mapit-border` border where a border is used.
- **Hover:** background shifts one step lighter (`mapit-surface` → `mapit-surface2`, or `mapit-surface2` → a slightly lighter variant), and/or border shifts to `mapit-accent` at reduced opacity (`border-mapit-accent/50`) — pick one consistent hover treatment per component category (e.g., all list rows hover the same way) rather than inventing a new hover style per screen.
- **Active/selected** (e.g., the currently-active breadcrumb level, the currently-selected severity filter): `mapit-accent` background with white/near-white text, or a `mapit-accent` left-border accent stripe plus `mapit-accent` text — pick one pattern and apply it everywhere "currently selected" needs representing, instead of the current inconsistency (the old `viewMode` tabs used solid-fill active state; nothing else in the app did).
- **Disabled:** `opacity-50`, `cursor-not-allowed`, no hover treatment.
- **Focus** (keyboard navigation, form inputs): `focus:ring-2 focus:ring-mapit-accent focus:outline-none` on every input and every button reachable by Tab — currently only present on form inputs, must be added to interactive buttons and graph-adjacent controls too.

### 3.5 The graph canvas specifically
- Node label text renders directly on the 2D canvas (not as sprites in 3D space, since §2.1 mandates 2D) using the canvas 2D text APIs or the chosen library's built-in label support; label color is always `mapit-text`, label background (if any, for legibility against a busy canvas) is `mapit-surface` at partial opacity.
- Edge color: `confidence: "exact"` edges render in `mapit-accent` at full opacity; `confidence: "probable"` edges render in `mapit-muted` at 60% opacity; `confidence: "dynamic_unresolved"` edges render in `mapit-muted` at 30% opacity with a dashed stroke. This distinction currently collapses `probable` and `dynamic_unresolved` into one identical visual treatment — keep all three visually distinct, since the data model deliberately tracks three confidence levels and the UI should reflect that.
- The currently-highlighted/selected node (if any) gets a `mapit-accent` ring/outline around it, not merely a size bump — size alone is too subtle to read at a glance on a dense graph.

### 3.6 Severity and status colors (flaws, AI summary status)
- Flaw severity: `high` → `mapit-danger`, `warning` → `mapit-warning`, `info` → `mapit-muted`. Background tints for flaw row cards use the same hue at low opacity (e.g. `bg-mapit-danger/10 border-mapit-danger/30`) rather than the current ad-hoc `bg-red-900/20 border-red-800` Tailwind-default-red literals, which are a different red than `mapit-danger` and constitute exactly the kind of token drift §1.1 exists to prevent.
- `ai_summary_status`: `ready` → normal `mapit-text`, `pending` → `mapit-muted` italic with a small pulsing dot in `mapit-accent` (not just static italic text — the spec calls for this to "live-update via WebSocket the moment it's ready," and a pulsing indicator communicates "this is actively being worked on" better than static text does), `unavailable` → `mapit-muted` italic with a small retry icon/button in `mapit-accent`, per the §6 edge-case requirement in `04-app-flow.md` that provider failures show "a small retry affordance," which does not currently exist anywhere in the codebase.

### 3.7 Confirmation dialogs (new pattern — none currently exist)
Required for: re-annotate-everything (§2.8), and any future destructive/costly action. A centered modal, `mapit-surface` background, `mapit-border` border, max-width ~28rem, with: a short title, one or two sentences of consequence explanation, a `mapit-border`-outlined "Cancel" button and a `mapit-danger`-or-`mapit-accent`-filled (accent if merely costly, danger if destructive/irreversible) confirm button on the right. Re-annotate-everything uses the accent treatment (costly, not destructive — existing annotations aren't deleted, per `04-app-flow.md` §4.7's note that they're "kept until explicitly re-run").

### 3.8 Grounding status indicator (Ask panel)
Small inline badge, not text concatenated into the answer: `ok` → a small `mapit-success` dot/checkmark, `partial` → a small `mapit-warning` dot, `no_relevant_context_found` → a small `mapit-danger` dot with the answer area showing the model's answer below a one-line note in `mapit-muted` that grounding was weak/absent for this question.

---

## 4. Verification checklist (run through this in full before calling the rebuild done)

Manually, in a running browser, against a real mapped project:

1. Reload the app from a cold state. Confirm the System Overview, FunctionDetailPanel, FlawsReport, SettingsPanel, TraceView, NeighborsView, and AskPanel all render using only colors from §3.1 — no visual "jump" in palette when moving between the main screen and any overlay.
2. Search for a function, a file, and a feature (one of each) from the TopBar search and confirm each opens the correct, type-appropriate destination per §2.2 — not all three funneling into the function detail panel.
3. Click into a feature node, then into one of its files, then into one of that file's functions. Confirm the breadcrumb shows three clickable levels, and clicking the middle one collapses back to that level exactly, preserving the feature's outer context per §2.1.
4. Confirm file-internal expansion (inside a single file, viewing its functions/types/macros/globals) visibly switches rendering approach to the `reactflow`-based layout, distinguishable from the force-directed canvas used above it.
5. Trigger a re-map while sitting inside an expanded view (not from a fresh load). Confirm the existing graph stays visible and interactive (dimmed) under a progress indicator, and that on completion you are still at the same zoom depth/breadcrumb you started at, with changed nodes pulsing once.
6. Open Trace View on a function with at least one conditional branch in its control flow. Confirm the rendering is a visual fork (two visually distinct parallel paths), not a single linear card list with branch text buried inside a card.
7. Open "Show full call tree" and confirm a depth slider exists, defaults to 3, and live-updates the view as it's dragged.
8. Trigger the dedicated radial callers/callees view (§2.6) and confirm it is visually and structurally distinct from "Show full call tree" — center node, callers to one side, callees to the other, ring-expandable.
9. Open Flaws Report, apply a severity filter, then click a flaw row and confirm it navigates into the graph and highlights the correct node(s).
10. Open Settings, attempt "Re-annotate everything," and confirm a confirmation dialog appears before any request fires; confirm provider/model are changeable via UI controls, not read-only text.
11. Ask a question in the Ask panel and confirm the response renders clickable source chips that navigate to the referenced nodes, and that `grounding_status` renders as a badge, not as bracketed text inside the answer.
12. Resize the browser window narrower and confirm no panel/overlay becomes unusable or clips its content (basic responsiveness was not in scope for the original build but should not actively break).

Every item above must be a genuine "yes" before this phase is considered complete — per AGENTS.md §5, "ran it and it looked plausible" is not sufficient evidence on its own.
