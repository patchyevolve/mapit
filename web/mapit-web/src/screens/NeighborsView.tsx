import { useEffect, useState, useCallback } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { Node, NodeType } from "../types";

// ─── Data model ────────────────────────────────────────────────────────────────

const MAX_PER_NODE = 15; // children shown per node before "show more"
const MAX_DEPTH = 6; // hard stop on tree depth to prevent infinite drilling

type Direction = "callees" | "callers";

interface TreeEntry {
  entryId: string; // unique per position, not per node (same fn can appear in multiple branches)
  nodeId: string;
  name: string;
  filePath?: string;
  startLine?: number;
  nodeType: NodeType;
  depth: number;
  direction: "callee" | "caller" | "root";
  expanded: boolean;
  loading: boolean;
  childrenLoaded: boolean;
  children: TreeEntry[];
  totalChildren: number;
  visibleCount: number;
}

function entryFromNode(
  node: Node,
  depth: number,
  dir: TreeEntry["direction"],
  parentEntryId: string,
): TreeEntry {
  const span = "span" in node ? (node as any).span : undefined;
  return {
    entryId: `${parentEntryId}/${node.id}`,
    nodeId: node.id,
    name: node.name,
    filePath: node.file_path,
    startLine: span?.start_line,
    nodeType: node.type,
    depth,
    direction: dir,
    expanded: false,
    loading: false,
    childrenLoaded: false,
    children: [],
    totalChildren: 0,
    visibleCount: MAX_PER_NODE,
  };
}

/** Immutable recursive update of a single entry by id */
function patchEntry(
  root: TreeEntry,
  entryId: string,
  updater: (e: TreeEntry) => TreeEntry,
): TreeEntry {
  if (root.entryId === entryId) return updater(root);
  return {
    ...root,
    children: root.children.map((c) => patchEntry(c, entryId, updater)),
  };
}

// ─── Sub-components ────────────────────────────────────────────────────────────

const NODE_TYPE_COLOR: Partial<Record<NodeType, string>> = {
  function: "text-mapit-node-function",
  type: "text-mapit-node-type",
  macro: "text-mapit-node-macro",
  global: "text-mapit-node-global",
  external: "text-mapit-node-external",
};

function TypePill({ kind }: { kind: NodeType }) {
  const labels: Partial<Record<NodeType, string>> = {
    function: "fn",
    type: "type",
    macro: "macro",
    global: "let",
    external: "ext",
    file: "file",
    feature: "feat",
    module: "mod",
  };
  return (
    <span
      className={`flex-shrink-0 font-mono text-xs ${NODE_TYPE_COLOR[kind] ?? "text-mapit-muted"}`}
    >
      {labels[kind] ?? kind}
    </span>
  );
}

const INDENT_PX = 22;

interface RowProps {
  entry: TreeEntry;
  direction: Direction;
  maxDepth: number;
  onExpand: (e: TreeEntry) => void;
  onShowMore: (entryId: string) => void;
  onNavigate: (nodeId: string) => void;
}

function TreeRow({
  entry,
  direction,
  maxDepth,
  onExpand,
  onShowMore,
  onNavigate,
}: RowProps) {
  const isExtern = entry.nodeType === "external";
  const isLeaf = entry.childrenLoaded && entry.children.length === 0;
  const canExpand = !isExtern && !isLeaf && entry.depth < maxDepth;

  const dirArrow =
    entry.direction === "callee" ? (
      <span className="flex-shrink-0 text-xs font-bold text-mapit-accent">
        →
      </span>
    ) : entry.direction === "caller" ? (
      <span className="flex-shrink-0 text-xs font-bold text-mapit-warning">
        ←
      </span>
    ) : null;

  const fileName = entry.filePath
    ? entry.filePath.split("/").slice(-1)[0]
    : undefined;

  return (
    <>
      {/* Row */}
      <div
        className={`flex items-center gap-2 py-1 px-2 rounded group hover:bg-mapit-surface2 transition-colors ${
          entry.depth === 0
            ? "bg-mapit-surface2 border border-mapit-border mb-1"
            : ""
        }`}
        style={{ paddingLeft: 8 + entry.depth * INDENT_PX }}
      >
        {/* Expand / collapse toggle */}
        <button
          type="button"
          className="flex-shrink-0 w-4 text-center text-mapit-muted hover:text-mapit-accent disabled:opacity-30 disabled:cursor-default transition-colors"
          onClick={() => canExpand && onExpand(entry)}
          disabled={!canExpand}
          title={
            canExpand ? (entry.expanded ? "Collapse" : "Expand") : undefined
          }
        >
          {entry.loading ? (
            <span className="inline-block w-3 h-3 border border-mapit-accent border-t-transparent rounded-full animate-spin" />
          ) : isLeaf || isExtern ? (
            <span className="text-mapit-border">─</span>
          ) : entry.expanded ? (
            "▾"
          ) : (
            "▸"
          )}
        </button>

        {dirArrow}

        <TypePill kind={entry.nodeType} />

        {/* Function name — navigate to detail on click */}
        <button
          type="button"
          className="flex-1 text-left text-sm font-mono text-mapit-text hover:text-mapit-accent truncate focus:ring-1 focus:ring-mapit-accent focus:outline-none rounded"
          onClick={() => onNavigate(entry.nodeId)}
          title={`${entry.name}${entry.filePath ? `  —  ${entry.filePath}` : ""}`}
        >
          {entry.name}
        </button>

        {/* File:line (rightmost, secondary) */}
        {fileName && (
          <span className="flex-shrink-0 text-xs text-mapit-muted font-mono opacity-70 group-hover:opacity-100 transition-opacity">
            {fileName}
            {entry.startLine ? `:${entry.startLine}` : ""}
          </span>
        )}

        {/* Unloaded children count badge */}
        {!entry.childrenLoaded && !entry.loading && canExpand && (
          <span
            className="flex-shrink-0 text-xs text-mapit-muted bg-mapit-surface border border-mapit-border rounded-full px-1.5 cursor-pointer hover:border-mapit-accent/50"
            onClick={() => onExpand(entry)}
          >
            ···
          </span>
        )}

        {/* Loaded children count badge */}
        {entry.childrenLoaded && entry.children.length > 0 && (
          <span className="flex-shrink-0 text-xs text-mapit-muted bg-mapit-surface border border-mapit-border rounded-full px-1.5">
            {entry.children.length}
          </span>
        )}
      </div>

      {/* Children (recursively rendered when expanded) */}
      {entry.expanded &&
        entry.children
          .slice(0, entry.visibleCount)
          .map((child) => (
            <TreeRow
              key={child.entryId}
              entry={child}
              direction={direction}
              maxDepth={maxDepth}
              onExpand={onExpand}
              onShowMore={onShowMore}
              onNavigate={onNavigate}
            />
          ))}

      {/* "Show N more" when visibleCount < total */}
      {entry.expanded && entry.children.length > entry.visibleCount && (
        <div
          style={{ paddingLeft: 8 + (entry.depth + 1) * INDENT_PX + 6 }}
          className="py-0.5"
        >
          <button
            type="button"
            className="text-xs text-mapit-accent hover:text-mapit-text transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none rounded"
            onClick={() => onShowMore(entry.entryId)}
          >
            + {entry.children.length - entry.visibleCount} more…
          </button>
        </div>
      )}
    </>
  );
}

// ─── Main component ────────────────────────────────────────────────────────────

export function NeighborsView() {
  const { state, dispatch } = useAppState();
  const [tree, setTree] = useState<TreeEntry | null>(null);
  const [rootLoading, setRootLoading] = useState(false);
  const [direction, setDirection] = useState<Direction>("callees");
  const [maxDepth, setMaxDepth] = useState(4);
  const [searchQuery, setSearch] = useState("");

  const nodeId =
    state.overlay?.kind === "neighbors" ? state.overlay.node_id : null;
  const centerNode = nodeId ? (state.allNodes.get(nodeId) ?? null) : null;

  // ── Load root when nodeId or direction changes ──
  useEffect(() => {
    if (!nodeId) return;
    let cancelled = false;
    setRootLoading(true);
    setTree(null);

    const apiDir = direction === "callees" ? "callees" : "callers";

    Promise.all([
      centerNode ?? api.node(nodeId),
      api.neighbors(nodeId, apiDir, 1),
    ])
      .then(([center, neighbors]) => {
        if (cancelled) return;
        const cn = center as Node;
        const childDir: TreeEntry["direction"] =
          direction === "callees" ? "callee" : "caller";
        const span = "span" in cn ? (cn as any).span : undefined;

        const children = neighbors.nodes
          .filter((n) => n.id !== nodeId)
          .map((n) => entryFromNode(n, 1, childDir, "root"));

        const root: TreeEntry = {
          entryId: "root",
          nodeId,
          name: cn.name,
          filePath: cn.file_path,
          startLine: span?.start_line,
          nodeType: cn.type,
          depth: 0,
          direction: "root",
          expanded: true,
          loading: false,
          childrenLoaded: true,
          children,
          totalChildren: children.length,
          visibleCount: MAX_PER_NODE,
        };
        setTree(root);
      })
      .catch(console.error)
      .finally(() => {
        if (!cancelled) setRootLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [nodeId, direction]);

  // ── Expand a node (lazy-load its children) ──
  const handleExpand = useCallback(
    async (entry: TreeEntry) => {
      if (!tree) return;

      // Collapse if already expanded
      if (entry.expanded) {
        setTree((prev) =>
          prev
            ? patchEntry(prev, entry.entryId, (e) => ({
                ...e,
                expanded: false,
              }))
            : null,
        );
        return;
      }

      // Re-expand already-loaded children
      if (entry.childrenLoaded) {
        setTree((prev) =>
          prev
            ? patchEntry(prev, entry.entryId, (e) => ({ ...e, expanded: true }))
            : null,
        );
        return;
      }

      // Lazy-load children
      setTree((prev) =>
        prev
          ? patchEntry(prev, entry.entryId, (e) => ({ ...e, loading: true }))
          : null,
      );

      const apiDir = direction === "callees" ? "callees" : "callers";
      try {
        const neighbors = await api.neighbors(entry.nodeId, apiDir, 1);
        const childDir: TreeEntry["direction"] =
          direction === "callees" ? "callee" : "caller";
        const children = neighbors.nodes
          .filter((n) => n.id !== entry.nodeId)
          .map((n) =>
            entryFromNode(n, entry.depth + 1, childDir, entry.entryId),
          );

        setTree((prev) =>
          prev
            ? patchEntry(prev, entry.entryId, (e) => ({
                ...e,
                loading: false,
                expanded: true,
                childrenLoaded: true,
                children,
                totalChildren: children.length,
                visibleCount: MAX_PER_NODE,
              }))
            : null,
        );
      } catch {
        setTree((prev) =>
          prev
            ? patchEntry(prev, entry.entryId, (e) => ({ ...e, loading: false }))
            : null,
        );
      }
    },
    [tree, direction],
  );

  // ── Show more children ──
  const handleShowMore = useCallback((entryId: string) => {
    setTree((prev) =>
      prev
        ? patchEntry(prev, entryId, (e) => ({
            ...e,
            visibleCount: e.visibleCount + MAX_PER_NODE,
          }))
        : null,
    );
  }, []);

  // ── Navigate to function detail ──
  const handleNavigate = useCallback(
    (nid: string) => {
      const node = state.allNodes.get(nid);
      if (!node) return;
      if (
        node.type === "function" ||
        node.type === "type" ||
        node.type === "macro" ||
        node.type === "global"
      ) {
        dispatch({
          type: "SET_OVERLAY",
          overlay: { kind: "function_detail", node_id: nid },
        });
      } else if (node.type === "external") {
        dispatch({
          type: "SET_OVERLAY",
          overlay: { kind: "external_detail", node_id: nid },
        });
      }
    },
    [state.allNodes, dispatch],
  );

  // ── Search filter: flatten tree and filter ──
  const totalNodes = tree ? countNodes(tree) - 1 : 0; // -1 for root

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* ── Header ── */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border gap-3 flex-wrap">
        <div className="flex items-center gap-3">
          <button
            type="button"
            className="text-mapit-muted hover:text-mapit-text text-sm focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
            onClick={() =>
              dispatch({
                type: "SET_OVERLAY",
                overlay: nodeId
                  ? { kind: "function_detail", node_id: nodeId }
                  : null,
              })
            }
          >
            ← Back
          </button>
          <div>
            <span className="text-sm font-semibold text-mapit-text font-mono">
              {centerNode?.name ?? "…"}
            </span>
            <span className="text-xs text-mapit-muted ml-2">
              {direction === "callees" ? "→ what it calls" : "← who calls it"}
            </span>
          </div>
          {!rootLoading && tree && (
            <span className="text-xs text-mapit-muted bg-mapit-surface2 border border-mapit-border rounded px-2 py-0.5">
              {totalNodes} reachable
            </span>
          )}
        </div>

        <div className="flex items-center gap-3 flex-wrap">
          {/* Direction toggle */}
          <div className="flex rounded border border-mapit-border overflow-hidden text-xs">
            <button
              type="button"
              className={`px-3 py-1 transition-colors ${
                direction === "callees"
                  ? "bg-mapit-accent text-white"
                  : "bg-mapit-surface2 text-mapit-muted hover:text-mapit-text"
              }`}
              onClick={() => setDirection("callees")}
            >
              → Callees
            </button>
            <button
              type="button"
              className={`px-3 py-1 transition-colors border-l border-mapit-border ${
                direction === "callers"
                  ? "bg-mapit-accent text-white"
                  : "bg-mapit-surface2 text-mapit-muted hover:text-mapit-text"
              }`}
              onClick={() => setDirection("callers")}
            >
              ← Callers
            </button>
          </div>

          {/* Depth limit */}
          <label className="flex items-center gap-2 text-xs text-mapit-muted">
            Max depth
            <input
              type="range"
              min={1}
              max={MAX_DEPTH}
              value={maxDepth}
              onChange={(e) => setMaxDepth(Number(e.target.value))}
              className="w-20 accent-mapit-accent"
            />
            <span className="w-3 text-mapit-text">{maxDepth}</span>
          </label>

          <button
            type="button"
            className="text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
            onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          >
            ✕
          </button>
        </div>
      </div>

      {/* ── Search ── */}
      <div className="px-4 py-2 border-b border-mapit-border bg-mapit-surface">
        <input
          type="text"
          placeholder="Filter by name…"
          value={searchQuery}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
        />
      </div>

      {/* ── Legend ── */}
      <div className="px-4 py-1.5 border-b border-mapit-border bg-mapit-surface flex items-center gap-4 text-xs text-mapit-muted">
        <span>
          <span className="text-mapit-accent font-bold">▸</span> expand node
        </span>
        <span>
          <span className="text-mapit-accent">→</span> calls
        </span>
        <span>
          <span className="text-mapit-warning">←</span> called by
        </span>
        <span>click name → open detail panel</span>
        <span className="ml-auto italic">
          stops at depth {maxDepth} — expand nodes to go deeper
        </span>
      </div>

      {/* ── Tree body ── */}
      <div className="flex-1 overflow-y-auto p-3 font-mono text-sm">
        {rootLoading ? (
          <div className="flex items-center justify-center h-32 gap-2 text-mapit-muted">
            <div className="w-5 h-5 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
            Loading…
          </div>
        ) : !tree ? (
          <div className="text-center text-mapit-muted py-8">No data</div>
        ) : searchQuery.trim() ? (
          <FlatSearch
            tree={tree}
            query={searchQuery.trim().toLowerCase()}
            direction={direction}
            maxDepth={maxDepth}
            onExpand={handleExpand}
            onShowMore={handleShowMore}
            onNavigate={handleNavigate}
          />
        ) : (
          <TreeRow
            entry={tree}
            direction={direction}
            maxDepth={maxDepth}
            onExpand={handleExpand}
            onShowMore={handleShowMore}
            onNavigate={handleNavigate}
          />
        )}
      </div>
    </div>
  );
}

// ── Flat search results ─────────────────────────────────────────────────────────

function collectAll(entry: TreeEntry, out: TreeEntry[]) {
  out.push(entry);
  entry.children.forEach((c) => collectAll(c, out));
}

function countNodes(entry: TreeEntry): number {
  return 1 + entry.children.reduce((s, c) => s + countNodes(c), 0);
}

function FlatSearch({
  tree,
  query,
  onNavigate,
}: {
  tree: TreeEntry;
  query: string;
  direction: Direction;
  maxDepth: number;
  onExpand: (e: TreeEntry) => void;
  onShowMore: (id: string) => void;
  onNavigate: (id: string) => void;
}) {
  const all: TreeEntry[] = [];
  collectAll(tree, all);
  const matches = all.filter(
    (e) =>
      e.entryId !== "root" &&
      (e.name.toLowerCase().includes(query) ||
        e.filePath?.toLowerCase().includes(query)),
  );

  if (matches.length === 0) {
    return (
      <div className="text-center text-mapit-muted py-8 text-sm">
        No matches for "{query}"
      </div>
    );
  }

  return (
    <div className="space-y-0.5">
      <div className="text-xs text-mapit-muted px-2 pb-2">
        {matches.length} match{matches.length !== 1 ? "es" : ""}
      </div>
      {matches.map((e) => (
        <div
          key={e.entryId}
          className="flex items-center gap-2 py-1.5 px-2 rounded hover:bg-mapit-surface2 cursor-pointer"
          onClick={() => onNavigate(e.nodeId)}
        >
          <TypePill kind={e.nodeType} />
          <span className="flex-1 text-sm text-mapit-text hover:text-mapit-accent font-mono truncate">
            {e.name}
          </span>
          {e.filePath && (
            <span className="flex-shrink-0 text-xs text-mapit-muted font-mono">
              {e.filePath.split("/").slice(-1)[0]}
              {e.startLine ? `:${e.startLine}` : ""}
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
