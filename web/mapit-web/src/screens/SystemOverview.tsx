import { useState, useMemo, useCallback, useRef, useEffect } from "react";
import ForceGraph2D from "react-force-graph-2d";
import { GraphView } from "../components/GraphView";
import { useAppState } from "../store";
import type { Node, FileNode, FeatureNode } from "../types";

// ─── helpers ───────────────────────────────────────────────────────────────────

const NODE_TYPE_LABEL: Partial<Record<string, string>> = {
  function: "fn",
  type: "type",
  macro: "macro",
  global: "let",
  module: "mod",
};
const NODE_TYPE_COLOR: Partial<Record<string, string>> = {
  function: "text-mapit-node-function",
  type: "text-mapit-node-type",
  macro: "text-mapit-node-macro",
  global: "text-mapit-node-global",
  module: "text-mapit-muted",
};

// ─── File-level symbol list ────────────────────────────────────────────────────
// Shown when the user has drilled into a specific file.

function FileView() {
  const { state, dispatch } = useAppState();
  const [search, setSearch] = useState("");

  const crumb = state.breadcrumb;
  const fileNode =
    crumb.length > 0
      ? state.allNodes.get(crumb[crumb.length - 1].node_id ?? "")
      : null;

  const symbols = useMemo(() => {
    if (!fileNode || fileNode.type !== "file") return [];
    const fp = fileNode.file_path;
    return Array.from(state.allNodes.values())
      .filter((n) => n.file_path === fp && n.type !== "file")
      .sort((a, b) => {
        const al = "span" in a ? ((a as any).span?.start_line ?? 9999) : 9999;
        const bl = "span" in b ? ((b as any).span?.start_line ?? 9999) : 9999;
        return al - bl;
      });
  }, [fileNode, state.allNodes]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return q
      ? symbols.filter((s) => s.name.toLowerCase().includes(q))
      : symbols;
  }, [symbols, search]);

  const file = fileNode as FileNode | undefined;

  // Group by type for a summary pill bar
  const counts = useMemo(() => {
    const m: Record<string, number> = {};
    symbols.forEach((s) => {
      m[s.type] = (m[s.type] ?? 0) + 1;
    });
    return m;
  }, [symbols]);

  const openDetail = (node: Node) => {
    if (
      node.type === "function" ||
      node.type === "type" ||
      node.type === "macro" ||
      node.type === "global"
    ) {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: node.id },
      });
    } else if (node.type === "external") {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "external_detail", node_id: node.id },
      });
    }
  };

  if (!file) {
    return (
      <div className="flex items-center justify-center h-full text-mapit-muted">
        File not found.
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* File header */}
      <div className="px-4 py-3 bg-mapit-surface border-b border-mapit-border">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <p
              className="text-xs text-mapit-muted font-mono truncate"
              title={file.file_path}
            >
              {file.file_path}
            </p>
            <div className="flex items-center gap-2 mt-1 flex-wrap">
              {file.language && (
                <span className="text-xs bg-mapit-surface2 border border-mapit-border rounded px-2 py-0.5">
                  {file.language}
                </span>
              )}
              <span className="text-xs text-mapit-muted">
                {symbols.length} symbols
              </span>
              {file.parse_status !== "ok" && (
                <span className="text-xs text-mapit-danger bg-mapit-danger/10 border border-mapit-danger/30 rounded px-2 py-0.5">
                  {file.parse_status}
                </span>
              )}
              {/* Type summary pills */}
              {Object.entries(counts).map(([type, count]) => (
                <span
                  key={type}
                  className={`text-xs font-mono ${NODE_TYPE_COLOR[type] ?? "text-mapit-muted"}`}
                >
                  {NODE_TYPE_LABEL[type] ?? type} ×{count}
                </span>
              ))}
            </div>
          </div>
        </div>
        {/* Search within file */}
        <input
          type="text"
          placeholder="Search symbols in this file…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="mt-2 w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
        />
      </div>

      {/* Symbol list */}
      <div className="flex-1 overflow-y-auto">
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-mapit-muted text-sm">
            {search
              ? `No symbols match "${search}"`
              : "No symbols found in this file."}
          </div>
        ) : (
          <table className="w-full text-sm">
            <tbody>
              {filtered.map((sym) => {
                const span = "span" in sym ? (sym as any).span : undefined;
                const summary = sym.ai_summary;
                const flaws = sym.flaws ?? [];
                return (
                  <tr
                    key={sym.id}
                    className="border-b border-mapit-border hover:bg-mapit-surface2 cursor-pointer group transition-colors"
                    onClick={() => openDetail(sym)}
                  >
                    {/* Type badge */}
                    <td className="px-3 py-2 w-12 text-right">
                      <span
                        className={`font-mono text-xs ${NODE_TYPE_COLOR[sym.type] ?? "text-mapit-muted"}`}
                      >
                        {NODE_TYPE_LABEL[sym.type] ?? sym.type}
                      </span>
                    </td>

                    {/* Name */}
                    <td className="px-2 py-2 font-mono text-mapit-text group-hover:text-mapit-accent transition-colors font-medium">
                      {sym.name}
                    </td>

                    {/* AI summary (truncated) */}
                    <td className="px-2 py-2 text-xs text-mapit-muted max-w-xs hidden md:table-cell">
                      {sym.ai_summary_status === "pending" ? (
                        <span className="italic flex items-center gap-1">
                          <span className="w-1.5 h-1.5 rounded-full bg-mapit-accent animate-pulse inline-block" />
                          pending
                        </span>
                      ) : summary ? (
                        <span className="truncate block max-w-xs">
                          {summary}
                        </span>
                      ) : null}
                    </td>

                    {/* Flaw indicator */}
                    <td className="px-2 py-2 w-8 text-center">
                      {flaws.length > 0 && (
                        <span
                          className="text-xs text-mapit-danger"
                          title={`${flaws.length} flaw${flaws.length !== 1 ? "s" : ""}`}
                        >
                          ⚠
                        </span>
                      )}
                    </td>

                    {/* Line number */}
                    <td className="px-3 py-2 w-16 text-right font-mono text-xs text-mapit-muted">
                      {span?.start_line ? `L${span.start_line}` : ""}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {/* Hint */}
      <div className="px-4 py-2 border-t border-mapit-border bg-mapit-surface text-xs text-mapit-muted">
        Click any symbol to open its detail panel → then use "Show call tree" or
        "▶ Trace execution"
      </div>
    </div>
  );
}

// ─── Feature-level file list ───────────────────────────────────────────────────
// Shown when a feature has been clicked. Lists its member files.

function FeatureView() {
  const { state, dispatch } = useAppState();
  const [search, setSearch] = useState("");

  const crumb = state.breadcrumb;
  const featureNode =
    crumb.length > 0
      ? state.allNodes.get(crumb[crumb.length - 1].node_id ?? "")
      : null;

  const memberFiles = useMemo(() => {
    if (!featureNode || featureNode.type !== "feature") return [];
    const feat = featureNode as FeatureNode;
    return feat.member_node_ids
      .map((id) => state.allNodes.get(id))
      .filter((n): n is Node => !!n && n.type === "file")
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [featureNode, state.allNodes]);

  // If no member_node_ids populated, fall back to all files
  const allFiles = useMemo(() => {
    if (memberFiles.length > 0) return memberFiles;
    return Array.from(state.allNodes.values())
      .filter((n): n is Node => n.type === "file")
      .sort((a, b) => (a.file_path ?? "").localeCompare(b.file_path ?? ""));
  }, [memberFiles, state.allNodes]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return q
      ? allFiles.filter(
          (f) =>
            f.name.toLowerCase().includes(q) ||
            f.file_path?.toLowerCase().includes(q),
        )
      : allFiles;
  }, [allFiles, search]);

  const openFile = (node: Node) => {
    dispatch({
      type: "SET_BREADCRUMB",
      breadcrumb: [...crumb, { label: node.name, node_id: node.id }],
    });
    dispatch({ type: "SET_SCREEN", screen: "expanded_file" });
  };

  // Count symbols per file
  const symbolCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    state.allNodes.forEach((n) => {
      if (n.file_path && n.type !== "file") {
        counts[n.file_path] = (counts[n.file_path] ?? 0) + 1;
      }
    });
    return counts;
  }, [state.allNodes]);

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* Header */}
      <div className="px-4 py-3 bg-mapit-surface border-b border-mapit-border">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm font-semibold text-mapit-text">
              {featureNode?.name ?? "Files"}
            </p>
            <p className="text-xs text-mapit-muted mt-0.5">
              {filtered.length} files
            </p>
          </div>
        </div>
        <input
          type="text"
          placeholder="Search files…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="mt-2 w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
        />
      </div>

      {/* File list */}
      <div className="flex-1 overflow-y-auto divide-y divide-mapit-border">
        {filtered.map((f) => {
          const file = f as FileNode;
          const symCount = symbolCounts[file.file_path ?? ""] ?? 0;
          return (
            <button
              key={f.id}
              type="button"
              className="w-full text-left px-4 py-2.5 hover:bg-mapit-surface2 transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none group flex items-center gap-3"
              onClick={() => openFile(f)}
            >
              {/* File icon */}
              <svg
                className="flex-shrink-0 text-mapit-node-file w-4 h-4"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                viewBox="0 0 24 24"
              >
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                <polyline points="14 2 14 8 20 8" />
              </svg>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-mono text-mapit-text group-hover:text-mapit-accent transition-colors truncate">
                  {file.file_path ?? file.name}
                </p>
                <div className="flex items-center gap-2 mt-0.5">
                  {file.language && (
                    <span className="text-xs text-mapit-muted">
                      {file.language}
                    </span>
                  )}
                  {symCount > 0 && (
                    <span className="text-xs text-mapit-muted">
                      {symCount} symbols
                    </span>
                  )}
                  {file.parse_status !== "ok" && (
                    <span className="text-xs text-mapit-danger">
                      ⚠ {file.parse_status}
                    </span>
                  )}
                </div>
              </div>
              <svg
                className="flex-shrink-0 text-mapit-muted group-hover:text-mapit-accent w-4 h-4 transition-colors"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                viewBox="0 0 24 24"
              >
                <polyline points="9 18 15 12 9 6" />
              </svg>
            </button>
          );
        })}
      </div>

      <div className="px-4 py-2 border-t border-mapit-border bg-mapit-surface text-xs text-mapit-muted">
        Click a file to see its symbols
      </div>
    </div>
  );
}

// ─── Stable color palette for directory-based bubble coloring ────────────────

const DIR_PALETTE = [
  "#5b8def",
  "#3ecf8e",
  "#e5566d",
  "#a684e8",
  "#e0a440",
  "#c792ea",
  "#4fc3d9",
  "#e07840",
  "#5ec4b0",
  "#d47fff",
  "#ff8c69",
  "#68b5e8",
];

function hashStr(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

function dirColor(dir: string): string {
  return DIR_PALETTE[hashStr(dir) % DIR_PALETTE.length];
}

interface BubbleNode {
  id: string;
  name: string;
  filePath: string;
  dir: string;
  color: string;
  val: number;
  symbolCount: number;
  x?: number;
  y?: number;
}

function FileBubbleGraph({
  files,
  symbolCounts,
  onFileClick,
}: {
  files: Node[];
  symbolCounts: Record<string, number>;
  onFileClick: (fileId: string) => void;
}) {
  const fgRef = useRef<any>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 800, h: 600 });

  // Resize observer
  useEffect(() => {
    if (!containerRef.current) return;
    const obs = new ResizeObserver((entries) => {
      const e = entries[0];
      if (e) setDims({ w: e.contentRect.width, h: e.contentRect.height });
    });
    obs.observe(containerRef.current);
    return () => obs.disconnect();
  }, []);

  // Get unique dirs and assign stable positions around a circle
  const dirs = useMemo(() => {
    const d = new Set<string>();
    files.forEach((f) => {
      d.add(((f.file_path ?? f.name) + "/").split("/")[0]);
    });
    return Array.from(d).sort();
  }, [files]);

  // Compute target position per directory (arranged in a circle)
  const dirTargets = useMemo(() => {
    const m: Record<string, { x: number; y: number }> = {};
    const spread = Math.min(dims.w, dims.h) * 0.3;
    dirs.forEach((dir, i) => {
      const angle = (i / Math.max(dirs.length, 1)) * Math.PI * 2 - Math.PI / 2;
      m[dir] = { x: Math.cos(angle) * spread, y: Math.sin(angle) * spread };
    });
    return m;
  }, [dirs, dims]);

  const graphData = useMemo(() => {
    const nodes: BubbleNode[] = files.map((f) => {
      const fp = f.file_path ?? f.name;
      const dir = (fp + "/").split("/")[0];
      const count = symbolCounts[fp] ?? 0;
      const target = dirTargets[dir] ?? { x: 0, y: 0 };
      return {
        id: f.id,
        name: fp.split("/").slice(-1)[0],
        filePath: fp,
        dir,
        color: dirColor(dir),
        val: Math.max(5, Math.cbrt(count) * 4 + 4),
        symbolCount: count,
        // Initial position near group target with jitter
        x: target.x + (hashStr(f.id) % 80) - 40,
        y: target.y + (hashStr(f.id + "y") % 80) - 40,
      };
    });
    return { nodes, links: [] as never[] };
  }, [files, symbolCounts, dirTargets]);

  // Add cluster force after mount / when targets change
  const nodesRef = useRef<BubbleNode[]>(graphData.nodes);
  nodesRef.current = graphData.nodes;

  useEffect(() => {
    const fg = fgRef.current;
    if (!fg) return;

    fg.d3Force("cluster", (alpha: number) => {
      const nodes = nodesRef.current as any[];
      // Compute live centroids
      const sums: Record<string, { sx: number; sy: number; n: number }> = {};
      nodes.forEach((node) => {
        if (!sums[node.dir]) sums[node.dir] = { sx: 0, sy: 0, n: 0 };
        sums[node.dir].sx += node.x ?? 0;
        sums[node.dir].sy += node.y ?? 0;
        sums[node.dir].n++;
      });
      // Also pull toward fixed target to keep groups separated
      nodes.forEach((node) => {
        const tgt = dirTargets[node.dir];
        if (!tgt) return;
        const liveCx = sums[node.dir]
          ? sums[node.dir].sx / sums[node.dir].n
          : tgt.x;
        const liveCy = sums[node.dir]
          ? sums[node.dir].sy / sums[node.dir].n
          : tgt.y;
        // Pull toward live centroid (keep group tight)
        node.vx = (node.vx ?? 0) - (node.x - liveCx) * alpha * 0.04;
        node.vy = (node.vy ?? 0) - (node.y - liveCy) * alpha * 0.04;
        // Pull centroid toward fixed target (keep groups separated from each other)
        node.vx -= (liveCx - tgt.x) * alpha * 0.025;
        node.vy -= (liveCy - tgt.y) * alpha * 0.025;
      });
    });

    // Weaken charge so bubbles don't fly apart
    fg.d3Force("charge")?.strength(-40);
    fg.d3ReheatSimulation?.();
  }, [dirTargets, graphData]);

  const handleNodeClick = useCallback(
    (node: any) => onFileClick(node.id),
    [onFileClick],
  );

  const nodeCanvasObject = useCallback(
    (node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const r = node.val as number;
      const alpha = 0.85;

      // Bubble fill
      ctx.beginPath();
      ctx.arc(node.x, node.y, r, 0, Math.PI * 2);
      ctx.fillStyle =
        node.color +
        Math.round(alpha * 255)
          .toString(16)
          .padStart(2, "0");
      ctx.fill();

      // Bubble border
      ctx.strokeStyle = node.color;
      ctx.lineWidth = 1.2 / globalScale;
      ctx.stroke();

      // Label (only when large enough to read)
      const minLabelR = 10 / globalScale;
      if (r > minLabelR) {
        const maxChars = Math.floor((r * 2 * globalScale) / 7);
        let label = node.name as string;
        if (label.length > maxChars && maxChars > 3)
          label = label.slice(0, maxChars - 1) + "\u2026";
        if (label.length > 2) {
          const fontSize = Math.min(r * 0.55, 13 / globalScale);
          ctx.font = `${fontSize}px system-ui, sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillStyle = "#e8eaf0";
          ctx.fillText(label, node.x, node.y);
        }
      }

      // Symbol count badge for large bubbles
      if (r > 18 / globalScale && node.symbolCount > 0) {
        const badgeFontSize = Math.min(r * 0.3, 9 / globalScale);
        ctx.font = `${badgeFontSize}px system-ui, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillStyle = node.color + "cc";
        ctx.fillText(String(node.symbolCount), node.x, node.y + r * 0.58);
      }
    },
    [],
  );

  // Build legend
  const legendDirs = useMemo(() => {
    const seen = new Set<string>();
    const out: { dir: string; color: string; count: number }[] = [];
    files.forEach((f) => {
      const dir = ((f.file_path ?? f.name) + "/").split("/")[0];
      if (!seen.has(dir)) {
        seen.add(dir);
        out.push({
          dir,
          color: dirColor(dir),
          count: files.filter(
            (g) => ((g.file_path ?? g.name) + "/").split("/")[0] === dir,
          ).length,
        });
      }
    });
    return out.sort((a, b) => b.count - a.count);
  }, [files]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      {/* Legend */}
      <div className="absolute top-3 left-3 z-10 bg-mapit-surface/90 border border-mapit-border rounded-lg p-2.5 backdrop-blur-sm max-w-[200px]">
        <p className="text-xs text-mapit-muted mb-1.5 font-semibold">
          Subsystems
        </p>
        <div className="space-y-1">
          {legendDirs.map(({ dir, color, count }) => (
            <div key={dir} className="flex items-center gap-1.5">
              <span
                className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                style={{ background: color }}
              />
              <span className="text-xs text-mapit-text font-mono truncate flex-1">
                {dir || "root"}
              </span>
              <span className="text-xs text-mapit-muted">{count}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Hint */}
      <div className="absolute bottom-3 right-3 z-10 text-xs text-mapit-muted bg-mapit-surface/80 border border-mapit-border rounded px-2 py-1 backdrop-blur-sm">
        click bubble → open file
      </div>

      <ForceGraph2D
        ref={fgRef}
        graphData={graphData}
        width={dims.w}
        height={dims.h}
        nodeVal="val"
        nodeColor="color"
        nodeLabel={(n: any) => `${n.filePath}\n${n.symbolCount} symbols`}
        nodeCanvasObject={nodeCanvasObject}
        nodeCanvasObjectMode={() => "replace"}
        onNodeClick={handleNodeClick}
        backgroundColor="#0b0d12"
        d3AlphaDecay={0.018}
        d3VelocityDecay={0.35}
        warmupTicks={60}
        cooldownTicks={150}
        linkDirectionalParticles={0}
        enableNodeDrag={true}
      />
    </div>
  );
}

// ─── Top-level file browser (no features) ─────────────────────────────────────
// Used when no AI feature classification has run yet.

function FileBrowser() {
  const { state, dispatch } = useAppState();
  const [search, setSearch] = useState("");
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(new Set([""]));

  const allFiles = useMemo(
    () =>
      Array.from(state.allNodes.values())
        .filter((n): n is Node => n.type === "file")
        .sort((a, b) => (a.file_path ?? "").localeCompare(b.file_path ?? "")),
    [state.allNodes],
  );

  // Group files by top-level directory
  const groups = useMemo(() => {
    const map = new Map<string, Node[]>();
    allFiles.forEach((f) => {
      const parts = (f.file_path ?? f.name).split("/");
      const dir = parts.length > 1 ? parts[0] : "";
      if (!map.has(dir)) map.set(dir, []);
      map.get(dir)!.push(f);
    });
    return Array.from(map.entries()).sort(([a], [b]) => a.localeCompare(b));
  }, [allFiles]);

  const symbolCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    state.allNodes.forEach((n) => {
      if (n.file_path && n.type !== "file") {
        counts[n.file_path] = (counts[n.file_path] ?? 0) + 1;
      }
    });
    return counts;
  }, [state.allNodes]);

  const openFile = (node: Node) => {
    dispatch({
      type: "SET_BREADCRUMB",
      breadcrumb: [{ label: node.file_path ?? node.name, node_id: node.id }],
    });
    dispatch({ type: "SET_SCREEN", screen: "expanded_file" });
  };

  const q = search.toLowerCase();
  const flatFiltered = q
    ? allFiles.filter(
        (f) =>
          (f.file_path ?? f.name).toLowerCase().includes(q) ||
          f.name.toLowerCase().includes(q),
      )
    : null;

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* Search */}
      <div className="px-4 py-3 bg-mapit-surface border-b border-mapit-border">
        <div className="flex items-center justify-between mb-2">
          <span className="text-sm font-semibold text-mapit-text">
            {allFiles.length} files
          </span>
          <span className="text-xs text-mapit-muted">
            Run{" "}
            <code className="font-mono bg-mapit-surface2 px-1 rounded">
              Annotate
            </code>{" "}
            to get AI feature groups
          </span>
        </div>
        <input
          type="text"
          placeholder="Search files by path or name…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="w-full px-3 py-1.5 text-sm bg-mapit-bg border border-mapit-border rounded text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent"
        />
      </div>

      <div className="flex-1 overflow-y-auto">
        {flatFiltered ? (
          // Search results: flat list
          <div className="divide-y divide-mapit-border">
            {flatFiltered.length === 0 ? (
              <div className="text-center py-8 text-mapit-muted text-sm">
                No files match "{search}"
              </div>
            ) : (
              flatFiltered.map((f) => (
                <FileRow
                  key={f.id}
                  file={f}
                  symCount={symbolCounts[f.file_path ?? ""] ?? 0}
                  onClick={() => openFile(f)}
                />
              ))
            )}
          </div>
        ) : (
          // Directory groups
          groups.map(([dir, files]) => (
            <div key={dir || "__root"}>
              <button
                type="button"
                className="w-full flex items-center gap-2 px-4 py-2 bg-mapit-surface hover:bg-mapit-surface2 transition-colors text-left border-b border-mapit-border"
                onClick={() =>
                  setExpandedDirs((prev) => {
                    const next = new Set(prev);
                    next.has(dir) ? next.delete(dir) : next.add(dir);
                    return next;
                  })
                }
              >
                <span className="text-xs text-mapit-muted">
                  {expandedDirs.has(dir) ? "▾" : "▸"}
                </span>
                <span className="text-sm font-mono text-mapit-muted">
                  {dir || "/ (root)"}
                </span>
                <span className="text-xs text-mapit-muted ml-auto">
                  {files.length} files
                </span>
              </button>
              {expandedDirs.has(dir) && (
                <div className="divide-y divide-mapit-border/50">
                  {files.map((f) => (
                    <FileRow
                      key={f.id}
                      file={f}
                      symCount={symbolCounts[f.file_path ?? ""] ?? 0}
                      onClick={() => openFile(f)}
                      indent
                    />
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </div>

      <div className="px-4 py-2 border-t border-mapit-border bg-mapit-surface text-xs text-mapit-muted">
        Click a file → see its symbols · click a symbol → detail panel
      </div>
    </div>
  );
}

// ─── File browser with bubble/list toggle ────────────────────────────────────

function FileBrowserWithToggle() {
  const { state, dispatch } = useAppState();
  const [viewMode, setViewMode] = useState<"bubble" | "list">("bubble");

  const allFiles = useMemo(
    () =>
      Array.from(state.allNodes.values()).filter(
        (n): n is Node => n.type === "file",
      ),
    [state.allNodes],
  );

  const symbolCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    state.allNodes.forEach((n) => {
      if (n.file_path && n.type !== "file") {
        counts[n.file_path] = (counts[n.file_path] ?? 0) + 1;
      }
    });
    return counts;
  }, [state.allNodes]);

  const openFile = useCallback(
    (fileId: string) => {
      const node = state.allNodes.get(fileId);
      if (!node) return;
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [{ label: node.file_path ?? node.name, node_id: node.id }],
      });
      dispatch({ type: "SET_SCREEN", screen: "expanded_file" });
    },
    [state.allNodes, dispatch],
  );

  const subsystemCount = useMemo(() => {
    const d = new Set<string>();
    allFiles.forEach((f) =>
      d.add(((f.file_path ?? f.name) + "/").split("/")[0]),
    );
    return d.size;
  }, [allFiles]);

  return (
    <div className="flex flex-col h-full">
      {/* Toggle bar */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <span className="text-xs text-mapit-muted">
          {allFiles.length} files · {subsystemCount} subsystem
          {subsystemCount !== 1 ? "s" : ""}
        </span>
        <div className="flex rounded border border-mapit-border overflow-hidden text-xs">
          <button
            type="button"
            className={`px-3 py-1 transition-colors ${viewMode === "bubble" ? "bg-mapit-accent text-white" : "bg-mapit-surface2 text-mapit-muted hover:text-mapit-text"}`}
            onClick={() => setViewMode("bubble")}
          >
            ⬡ Bubble
          </button>
          <button
            type="button"
            className={`px-3 py-1 transition-colors border-l border-mapit-border ${viewMode === "list" ? "bg-mapit-accent text-white" : "bg-mapit-surface2 text-mapit-muted hover:text-mapit-text"}`}
            onClick={() => setViewMode("list")}
          >
            ≡ List
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0">
        {viewMode === "bubble" ? (
          <FileBubbleGraph
            files={allFiles}
            symbolCounts={symbolCounts}
            onFileClick={openFile}
          />
        ) : (
          <FileBrowser />
        )}
      </div>
    </div>
  );
}

function FileRow({
  file,
  symCount,
  onClick,
  indent = false,
}: {
  file: Node;
  symCount: number;
  onClick: () => void;
  indent?: boolean;
}) {
  const f = file as FileNode;
  return (
    <button
      type="button"
      className={`w-full text-left py-2 hover:bg-mapit-surface2 transition-colors flex items-center gap-3 group focus:ring-1 focus:ring-mapit-accent focus:outline-none ${indent ? "px-8" : "px-4"}`}
      onClick={onClick}
    >
      <svg
        className="flex-shrink-0 text-mapit-node-file w-3.5 h-3.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        viewBox="0 0 24 24"
      >
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
      </svg>
      <span className="flex-1 text-sm font-mono text-mapit-text group-hover:text-mapit-accent transition-colors truncate">
        {f.file_path?.split("/").slice(1).join("/") || f.name}
      </span>
      {f.language && (
        <span className="flex-shrink-0 text-xs text-mapit-muted">
          {f.language}
        </span>
      )}
      {symCount > 0 && (
        <span className="flex-shrink-0 text-xs text-mapit-muted">
          {symCount} sym
        </span>
      )}
      {f.parse_status !== "ok" && (
        <span className="flex-shrink-0 text-xs text-mapit-danger">⚠</span>
      )}
      <svg
        className="flex-shrink-0 text-mapit-muted group-hover:text-mapit-accent w-3.5 h-3.5 transition-colors"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        viewBox="0 0 24 24"
      >
        <polyline points="9 18 15 12 9 6" />
      </svg>
    </button>
  );
}

// ─── Top-level feature graph ───────────────────────────────────────────────────
// Only used when AI features are available and we're at the root level.

function FeatureGraph() {
  const { state, dispatch } = useAppState();

  const features = state.features;
  const edges = useMemo(() => {
    const ids = new Set(features.map((f) => f.id));
    return state.allEdges.filter((e) => ids.has(e.from_id) && ids.has(e.to_id));
  }, [features, state.allEdges]);

  const handleClick = (node: Node) => {
    if (node.type === "feature") {
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [{ label: node.name, node_id: node.id }],
      });
      dispatch({ type: "SET_SCREEN", screen: "expanded_feature" });
    }
  };

  return (
    <div className="relative h-full w-full">
      <GraphView
        nodes={features}
        edges={edges}
        onNodeClick={handleClick}
        onBackgroundClick={() =>
          dispatch({ type: "SET_OVERLAY", overlay: null })
        }
      />
      <div className="absolute bottom-4 left-4 text-xs text-mapit-muted bg-mapit-surface border border-mapit-border px-3 py-1.5 rounded pointer-events-none">
        {features.length} features · click to drill in
      </div>
    </div>
  );
}

// ─── Main component (router) ───────────────────────────────────────────────────

export function SystemOverview() {
  const { state, dispatch } = useAppState();

  const allNodesArr = useMemo(
    () => Array.from(state.allNodes.values()),
    [state.allNodes],
  );

  // Stats bar (shown at every level except expanded_file)
  const statsBar =
    state.project && state.screen !== "expanded_file" ? (
      <div className="flex items-center gap-4 px-4 py-1.5 bg-mapit-surface border-b border-mapit-border text-xs text-mapit-muted">
        <span>{state.project.file_count} files</span>
        <span>{state.project.symbol_count} symbols</span>
        <span>{state.project.edge_count} edges</span>
        {state.project.ai_annotation_coverage_pct > 0 && (
          <span className="text-mapit-success">
            {state.project.ai_annotation_coverage_pct.toFixed(0)}% AI coverage
          </span>
        )}
        {state.flaws.length > 0 && (
          <button
            type="button"
            className="ml-auto flex items-center gap-1 text-mapit-danger hover:text-mapit-text transition-colors"
            onClick={() =>
              dispatch({ type: "SET_SCREEN", screen: "flaws_report" })
            }
          >
            ⚠ {state.flaws.length} flaw{state.flaws.length !== 1 ? "s" : ""}
          </button>
        )}
      </div>
    ) : null;

  // Route to the correct view
  const body = (() => {
    if (state.screen === "expanded_file") {
      return <FileView />;
    }
    if (state.screen === "expanded_feature") {
      return <FeatureView />;
    }
    // Top-level overview
    if (state.features.length > 0) {
      return <FeatureGraph />;
    }
    if (allNodesArr.some((n) => n.type === "file")) {
      return <FileBrowserWithToggle />;
    }
    // Empty state
    return (
      <div className="flex flex-col items-center justify-center h-full text-mapit-muted gap-3">
        <div className="text-lg font-medium">No map data found</div>
        <div className="text-sm">
          Run{" "}
          <code className="bg-mapit-surface2 px-2 py-1 rounded border border-mapit-border font-mono">
            mapit map
          </code>{" "}
          to scan this project.
        </div>
      </div>
    );
  })();

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {statsBar}
      <div className="flex-1 min-h-0">{body}</div>
    </div>
  );
}
