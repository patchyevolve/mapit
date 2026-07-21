import {
  useState,
  useEffect,
  useRef,
  useMemo,
  useCallback,
} from "react";
import ForceGraph2D from "react-force-graph-2d";
import { useAppState } from "../store";
import type { Node, Edge } from "../types";

// ─── Step model ────────────────────────────────────────────────────────────────

type StepKind = "enter" | "call" | "return" | "cycle" | "maxdepth" | "external";
type SimScope = "function" | "file" | "module" | "project";

interface SimStep {
  id:            number;
  kind:          StepKind;
  nodeId:        string;
  nodeName:      string;
  filePath?:     string;
  startLine?:    number;
  depth:         number;
  calleeNodeId?: string;
  calleeName?:   string;
}

interface SimState {
  callStack:    string[];
  activeNodeId: string | null;
  visitedIds:   Set<string>;
  completedIds: Set<string>;
}

// ─── Step generation (DFS over static call graph, multiple roots) ─────────────

function generateSteps(
  rootIds:  string[],
  allNodes: Map<string, Node>,
  allEdges: Edge[],
  maxDepth: number,
  maxSteps = 700,
): SimStep[] {
  const steps: SimStep[] = [];
  let nextId = 0;

  const calleeMap = new Map<string, string[]>();
  allEdges.forEach((e) => {
    if (e.type === "calls") {
      const list = calleeMap.get(e.from_id) ?? [];
      list.push(e.to_id);
      calleeMap.set(e.from_id, list);
    }
  });

  function push(s: Omit<SimStep, "id">) {
    steps.push({ ...s, id: nextId++ });
  }

  function dfs(nodeId: string, depth: number, pathSet: Set<string>) {
    if (steps.length >= maxSteps) return;
    const node     = allNodes.get(nodeId);
    const nodeName = node?.name ?? nodeId.slice(0, 12);
    const filePath = node?.file_path ?? undefined;
    const startLine =
      node && "span" in node ? (node as any).span?.start_line : undefined;

    if (depth > maxDepth) {
      push({ kind: "maxdepth", nodeId, nodeName, filePath, startLine, depth });
      return;
    }
    if (pathSet.has(nodeId)) {
      push({ kind: "cycle", nodeId, nodeName, filePath, startLine, depth });
      return;
    }
    if (!node) {
      push({ kind: "external", nodeId, nodeName, depth });
      return;
    }

    push({ kind: "enter", nodeId, nodeName, filePath, startLine, depth });

    const nextPath = new Set(pathSet).add(nodeId);
    for (const calleeId of calleeMap.get(nodeId) ?? []) {
      if (steps.length >= maxSteps) break;
      const callee = allNodes.get(calleeId);
      push({
        kind:          "call",
        nodeId,
        nodeName,
        depth,
        calleeNodeId:  calleeId,
        calleeName:    callee?.name ?? calleeId.slice(0, 12),
      });
      dfs(calleeId, depth + 1, nextPath);
    }

    push({ kind: "return", nodeId, nodeName, filePath, startLine, depth });
  }

  for (const rootId of rootIds) {
    dfs(rootId, 0, new Set());
  }
  return steps;
}

// ─── Compute simulation state at a given step index ───────────────────────────

function computeSimState(steps: SimStep[], upTo: number): SimState {
  const callStack:    string[]    = [];
  const visitedIds:   Set<string> = new Set();
  const completedIds: Set<string> = new Set();

  for (let i = 0; i <= upTo && i < steps.length; i++) {
    const s = steps[i];
    if (s.kind === "enter") {
      callStack.push(s.nodeId);
      visitedIds.add(s.nodeId);
    } else if (s.kind === "return") {
      const rev = [...callStack].reverse().findIndex((id) => id === s.nodeId);
      if (rev !== -1) {
        const from = callStack.length - 1 - rev;
        callStack.splice(from).forEach((id) => completedIds.add(id));
      }
    }
  }

  return {
    callStack,
    activeNodeId: callStack[callStack.length - 1] ?? null,
    visitedIds,
    completedIds,
  };
}

// ─── Constants ─────────────────────────────────────────────────────────────────

const SPEEDS = [
  { label: "0.5×", ms: 2000 },
  { label: "1×",   ms: 1000 },
  { label: "2×",   ms: 500  },
  { label: "5×",   ms: 200  },
  { label: "10×",  ms: 80   },
  { label: "20×",  ms: 30   },
];

const STEP_META: Record<StepKind, { sym: string; color: string }> = {
  enter:    { sym: "▶",  color: "text-mapit-accent"  },
  call:     { sym: "→",  color: "text-mapit-text"    },
  return:   { sym: "←",  color: "text-mapit-muted"   },
  cycle:    { sym: "↺",  color: "text-mapit-warning"  },
  maxdepth: { sym: "⋯",  color: "text-mapit-muted"   },
  external: { sym: "⊕",  color: "text-mapit-muted"   },
};

// Node state colours — must match legend
const COLOR = {
  active:    "#5b8def",    // bright accent
  inStack:   "#5b8def99",  // accent 60%
  completed: "#3ecf8e55",  // success very dim
  visited:   "#e0a44055",  // warn dim (entered but not returned yet, not active)
  inactive:  "#1b1f2a",    // near-invisible bg colour
};

// ─── Main component ────────────────────────────────────────────────────────────

export function SimulationView() {
  const { state, dispatch } = useAppState();

  // ── Determine root IDs from overlay scope (memoized) ──
  const overlay   = state.overlay;
  const scopeInfo = useMemo(() => {
    if (overlay?.kind === "simulation") {
      const nid = overlay.node_id;
      return {
        scope: "function" as SimScope,
        rootIds: [nid],
        title: state.allNodes.get(nid)?.name ?? nid.slice(0, 12),
        color: "#5b8def",
      };
    }
    if (overlay?.kind === "file_simulation") {
      const ids = [...state.allNodes.values()]
        .filter((n) => n.type === "function" && n.file_path === overlay.file_path)
        .map((n) => n.id);
      return {
        scope: "file" as SimScope,
        rootIds: ids,
        title: overlay.title,
        color: "#3ecf8e",
      };
    }
    if (overlay?.kind === "module_simulation") {
      const ids = [...state.allNodes.values()]
        .filter((n) => n.type === "function" && n.file_path?.startsWith(overlay.path))
        .map((n) => n.id);
      return {
        scope: "module" as SimScope,
        rootIds: ids,
        title: overlay.title,
        color: "#e0a440",
      };
    }
    if (overlay?.kind === "feature_simulation") {
      const feat = state.allNodes.get(overlay.node_id);
      const members: string[] = feat?.type === "feature" ? (feat as any).member_node_ids ?? [] : [];
      const memberFiles = members
        .map((id: string): Node | undefined => state.allNodes.get(id))
        .filter((n): n is Node => !!n && n.type === "file");
      const memberPaths = new Set(memberFiles.map((f: Node): string | undefined => f.file_path).filter(Boolean));
      const ids = [...state.allNodes.values()]
        .filter((n: Node): boolean => n.type === "function" && !!n.file_path && memberPaths.has(n.file_path))
        .map((n: Node): string => n.id);
      return {
        scope: "module" as SimScope,
        rootIds: ids,
        title: overlay.title,
        color: "#d05bce",
      };
    }
    if (overlay?.kind === "project_simulation") {
      const candidates = [...state.allNodes.values()]
        .filter((n) => n.type === "function" && (n as any).is_entry_point_candidate)
        .map((n) => n.id);
      const ids = candidates.length > 0
        ? candidates
        : [...state.allNodes.values()]
            .filter((n) => n.type === "function" && n.file_path)
            .map((n) => n.id);
      return {
        scope: "project" as SimScope,
        rootIds: ids,
        title: "Project (all entry points)",
        color: "#d05bce",
      };
    }
    return null;
  }, [overlay, state.allNodes]);

  const rootIds   = scopeInfo?.rootIds ?? [];
  const scopeKind = scopeInfo?.scope ?? "function";

  const [maxDepth,  setMaxDepth]  = useState(4);
  const [stepIdx,   setStepIdx]   = useState(0);
  const [playing,   setPlaying]   = useState(false);
  const [speedIdx,  setSpeedIdx]  = useState(2);  // default 2×
  const [dims,      setDims]      = useState({ w: 600, h: 500 });

  const containerRef  = useRef<HTMLDivElement>(null);
  const fgRef         = useRef<any>(null);
  const stepListRef   = useRef<HTMLDivElement>(null);
  const timerRef      = useRef<ReturnType<typeof setInterval> | null>(null);

  // ── Resize observer for graph panel ──
  useEffect(() => {
    if (!containerRef.current) return;
    const obs = new ResizeObserver((entries) => {
      const e = entries[0];
      if (e) setDims({ w: e.contentRect.width, h: e.contentRect.height });
    });
    obs.observe(containerRef.current);
    return () => obs.disconnect();
  }, []);

  // ── Dynamic max steps based on scope ──
  const maxSteps = useMemo(() => {
    const n = state.allNodes.size;
    const r = rootIds.length;
    if (r <= 1) return 600;
    if (r <= 10) return 2000;
    return Math.min(n * 4, 10000);
  }, [rootIds.length, state.allNodes.size]);

  // ── Pre-compute steps when root / depth changes ──
  const steps = useMemo(() => {
    if (rootIds.length === 0) return [];
    return generateSteps(rootIds, state.allNodes, state.allEdges, maxDepth, maxSteps);
  }, [rootIds, state.allNodes, state.allEdges, maxDepth, maxSteps]);

  // ── Reachable subgraph for graph panel ──
  const { nodes: graphNodes, edges: graphEdges } = useMemo(() => {
    if (rootIds.length === 0) return { nodes: [], edges: [] };
    // union reachable from all roots
    const reachable = new Set<string>();
    const calleeMap = new Map<string, string[]>();
    state.allEdges.forEach((e) => {
      if (e.type === "calls") {
        const list = calleeMap.get(e.from_id) ?? [];
        list.push(e.to_id);
        calleeMap.set(e.from_id, list);
      }
    });
    for (const rid of rootIds) {
      const queue: [string, number][] = [[rid, 0]];
      while (queue.length > 0) {
        const [id, d] = queue.shift()!;
        if (reachable.has(id) || d > maxDepth) continue;
        reachable.add(id);
        (calleeMap.get(id) ?? []).forEach((c) => queue.push([c, d + 1]));
      }
    }
    const nodes = [...reachable]
      .map((id) => state.allNodes.get(id))
      .filter(Boolean) as Node[];
    const edges = state.allEdges.filter(
      (e) => e.type === "calls" && reachable.has(e.from_id) && reachable.has(e.to_id),
    );
    return { nodes, edges };
  }, [rootIds, state.allNodes, state.allEdges, maxDepth]);

  // ── Simulation state at current step ──
  const simState = useMemo(
    () => computeSimState(steps, stepIdx),
    [steps, stepIdx],
  );

  // ── Reset when steps change (depth changed) ──
  useEffect(() => {
    setStepIdx(0);
    setPlaying(false);
  }, [steps]);

  // ── Auto-play timer ──
  useEffect(() => {
    if (timerRef.current) clearInterval(timerRef.current);
    if (!playing) return;
    timerRef.current = setInterval(() => {
      setStepIdx((prev) => {
        if (prev >= steps.length - 1) {
          setPlaying(false);
          return prev;
        }
        return prev + 1;
      });
    }, SPEEDS[speedIdx].ms);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [playing, speedIdx, steps.length]);

  // ── Auto-scroll step list ──
  useEffect(() => {
    const el = stepListRef.current?.querySelector(`[data-step="${stepIdx}"]`);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [stepIdx]);

  // ── Graph data ──
  const rootSet = useMemo(() => new Set(rootIds), [rootIds]);
  const graphData = useMemo(() => {
    const nodeSet = new Set(graphNodes.map((n) => n.id));
    return {
      nodes: graphNodes.map((n) => ({
        id:       n.id,
        name:     n.name,
        filePath: n.file_path,
        isRoot:   rootSet.has(n.id),
        val:      rootSet.has(n.id) ? 9 : 5,
      })),
      links: graphEdges
        .filter((e) => nodeSet.has(e.from_id) && nodeSet.has(e.to_id))
        .map((e) => ({ id: e.id, source: e.from_id, target: e.to_id })),
    };
  }, [graphNodes, graphEdges, rootSet]);

  // ── Node colour ──
  const nodeColor = useCallback(
    (node: any): string => {
      const id = node.id as string;
      if (id === simState.activeNodeId)           return COLOR.active;
      if (simState.callStack.includes(id))        return COLOR.inStack;
      if (simState.completedIds.has(id))          return COLOR.completed;
      if (simState.visitedIds.has(id))            return COLOR.visited;
      return COLOR.inactive;
    },
    [simState],
  );

  // ── Link colour ──
  const linkColor = useCallback(
    (link: any): string => {
      const src =
        typeof link.source === "object" ? link.source.id : (link.source as string);
      const tgt =
        typeof link.target === "object" ? link.target.id : (link.target as string);
      const srcActive =
        src === simState.activeNodeId || simState.callStack.includes(src);
      const tgtActive =
        tgt === simState.activeNodeId || simState.callStack.includes(tgt);
      return srcActive && tgtActive ? "#5b8def99" : "#262b3844";
    },
    [simState],
  );

  // ── Canvas node renderer ──
  const nodeCanvasObject = useCallback(
    (node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const id      = node.id as string;
      const r       = (node.val as number) ?? 5;
      const color   = nodeColor(node);
      const active  = id === simState.activeNodeId;
      const inStack = simState.callStack.includes(id);

      // Glow for active node
      if (active) {
        ctx.beginPath();
        ctx.arc(node.x, node.y, r + 5 / globalScale, 0, Math.PI * 2);
        ctx.fillStyle = "#5b8def22";
        ctx.fill();
        ctx.beginPath();
        ctx.arc(node.x, node.y, r + 2.5 / globalScale, 0, Math.PI * 2);
        ctx.fillStyle = "#5b8def44";
        ctx.fill();
      }

      // Ring for call-stack members
      if (inStack && !active) {
        ctx.beginPath();
        ctx.arc(node.x, node.y, r + 1.5 / globalScale, 0, Math.PI * 2);
        ctx.strokeStyle = "#5b8def88";
        ctx.lineWidth   = 1.5 / globalScale;
        ctx.stroke();
      }

      // Node fill
      ctx.beginPath();
      ctx.arc(node.x, node.y, r, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();

      // Label
      const label = node.name as string;
      const fontSize = Math.min(r * 0.75, 11 / globalScale);
      if (fontSize > 1.8) {
        const short = label.length > 13 ? label.slice(0, 12) + "…" : label;
        ctx.font          = `${fontSize}px system-ui, sans-serif`;
        ctx.textAlign     = "center";
        ctx.textBaseline  = "middle";
        ctx.fillStyle     = active
          ? "#ffffff"
          : inStack
            ? "#e8eaf0"
            : simState.completedIds.has(id)
              ? "#3ecf8e"
              : "#8b91a3";
        ctx.fillText(short, node.x, node.y);
      }
    },
    [nodeColor, simState],
  );

  // ── Derived ──
  const currentStep = steps[stepIdx];
  const pct = steps.length > 1
    ? Math.round((stepIdx / (steps.length - 1)) * 100)
    : 0;

  // ── Particles on active edges ──
  const linkParticles = useCallback(
    (link: any): number => {
      const src =
        typeof link.source === "object" ? link.source.id : (link.source as string);
      return simState.callStack.includes(src) && simState.callStack.length > 1 ? 3 : 0;
    },
    [simState],
  );

  if (rootIds.length === 0) return null;

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* ── Header ── */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border gap-3 flex-wrap">
        <div className="flex items-center gap-3 min-w-0">
          <button
            type="button"
            className="flex-shrink-0 text-mapit-muted hover:text-mapit-text text-sm focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
            onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          >
            ← Back
          </button>
          <span className="text-sm font-semibold text-mapit-text truncate">
            🎬 <span className="uppercase text-[10px] tracking-widest text-mapit-muted mr-1">{scopeKind}</span>
            {scopeInfo?.title}
          </span>
          <span className="flex-shrink-0 text-xs text-mapit-muted bg-mapit-surface2 border border-mapit-border rounded px-2 py-0.5">
            {steps.length} steps · {graphNodes.length} fns · {scopeInfo?.rootIds.length} root{scopeInfo && scopeInfo.rootIds.length !== 1 ? "s" : ""}
          </span>
        </div>

        <div className="flex items-center gap-3 flex-wrap">
          {/* Depth control */}
          <label className="flex items-center gap-2 text-xs text-mapit-muted">
            Depth
            <input
              type="range"
              min={1}
              max={8}
              value={maxDepth}
              onChange={(e) => setMaxDepth(Number(e.target.value))}
              className="w-16 accent-mapit-accent"
            />
            <span className="w-3 text-mapit-text">{maxDepth}</span>
          </label>

          {/* Speed buttons */}
          <div className="flex rounded border border-mapit-border overflow-hidden text-xs">
            {SPEEDS.map((s, i) => (
              <button
                key={i}
                type="button"
                className={`px-2 py-1 transition-colors ${
                  speedIdx === i
                    ? "bg-mapit-accent text-white"
                    : "bg-mapit-surface2 text-mapit-muted hover:text-mapit-text"
                } ${i > 0 ? "border-l border-mapit-border" : ""}`}
                onClick={() => setSpeedIdx(i)}
              >
                {s.label}
              </button>
            ))}
          </div>

          {/* Playback controls */}
          <div className="flex items-center gap-0.5">
            <CtrlBtn title="Jump to start"   onClick={() => { setStepIdx(0); setPlaying(false); }}>⏮</CtrlBtn>
            <CtrlBtn title="Step back"        onClick={() => setStepIdx((p) => Math.max(0, p - 1))} disabled={stepIdx === 0}>◀</CtrlBtn>
            <button
              type="button"
              className="px-3 py-1.5 rounded bg-mapit-accent text-white hover:opacity-90 transition-opacity text-xs font-semibold min-w-[70px] mx-1"
              onClick={() => setPlaying((p) => !p)}
            >
              {playing ? "⏸ Pause" : "▶ Play"}
            </button>
            <CtrlBtn title="Step forward"     onClick={() => setStepIdx((p) => Math.min(steps.length - 1, p + 1))} disabled={stepIdx >= steps.length - 1}>▶</CtrlBtn>
            <CtrlBtn title="Jump to end"      onClick={() => { setStepIdx(steps.length - 1); setPlaying(false); }}>⏭</CtrlBtn>
          </div>

          <button
            type="button"
            className="text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded p-1"
            onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          >
            ✕
          </button>
        </div>
      </div>

      {/* ── Progress bar ── */}
      <div className="h-1.5 bg-mapit-surface2 relative">
        <div
          className="h-full bg-mapit-accent transition-all duration-150"
          style={{ width: `${pct}%` }}
        />
      </div>

      {/* ── Step status bar ── */}
      <div className="flex items-center gap-3 px-4 py-1.5 bg-mapit-surface border-b border-mapit-border text-xs text-mapit-muted overflow-hidden">
        <span className="font-mono flex-shrink-0">
          Step {stepIdx + 1} / {steps.length}
        </span>
        {simState.callStack.length > 0 && (
          <span className="truncate">
            <span className="text-mapit-accent">Stack:</span>{" "}
            {simState.callStack
              .map((id) => state.allNodes.get(id)?.name ?? id.slice(0, 8))
              .join(" → ")}
          </span>
        )}
        {currentStep && (
          <span className="ml-auto flex-shrink-0 font-mono text-mapit-muted">
            depth {currentStep.depth}
          </span>
        )}
      </div>

      {/* ── Split main view ── */}
      <div className="flex-1 flex min-h-0 overflow-hidden">

        {/* ══ LEFT: Execution timeline ══ */}
        <div className="w-[300px] flex-shrink-0 flex flex-col border-r border-mapit-border min-h-0">
          <div className="px-3 py-1.5 bg-mapit-surface border-b border-mapit-border">
            <span className="text-xs font-semibold text-mapit-muted uppercase tracking-widest">
              Execution Flow
            </span>
          </div>
          <div
            ref={stepListRef}
            className="flex-1 overflow-y-auto text-xs font-mono"
          >
            {steps.map((step, i) => {
              const meta      = STEP_META[step.kind];
              const isCurrent = i === stepIdx;
              const isPast    = i < stepIdx;
              const indent    = 8 + step.depth * 14;

              return (
                <button
                  key={step.id}
                  data-step={i}
                  type="button"
                  style={{ paddingLeft: indent }}
                  className={`w-full text-left flex items-center gap-1.5 py-0.5 pr-2 transition-colors border-l-2 ${
                    isCurrent
                      ? "bg-mapit-accent/15 border-mapit-accent"
                      : "border-transparent hover:bg-mapit-surface2"
                  } ${!isPast && !isCurrent ? "opacity-20" : ""}`}
                  onClick={() => { setStepIdx(i); setPlaying(false); }}
                >
                  {/* Symbol */}
                  <span className={`flex-shrink-0 w-3 text-center font-bold ${meta.color}`}>
                    {meta.sym}
                  </span>

                  {/* Content */}
                  {step.kind === "call" ? (
                    <span className="truncate">
                      <span className="text-mapit-muted">{step.nodeName}</span>
                      <span className="text-mapit-border"> → </span>
                      <span className={isCurrent ? "text-mapit-accent font-semibold" : "text-mapit-text"}>
                        {step.calleeName}
                      </span>
                    </span>
                  ) : (
                    <span className={`truncate ${isCurrent ? "text-mapit-text font-semibold" : "text-mapit-muted"}`}>
                      {step.nodeName}
                    </span>
                  )}

                  {/* File:line */}
                  {step.filePath && (
                    <span className="flex-shrink-0 ml-auto text-mapit-border text-[10px] pl-1">
                      {step.filePath.split("/").slice(-1)[0]}
                      {step.startLine ? `:${step.startLine}` : ""}
                    </span>
                  )}
                </button>
              );
            })}

            {steps.length === 0 && (
              <div className="flex flex-col items-center justify-center h-32 gap-2 text-mapit-muted">
                <span className="text-2xl">∅</span>
                <span className="text-xs">No calls within depth {maxDepth}</span>
              </div>
            )}
          </div>
        </div>

        {/* ══ RIGHT: Live call graph ══ */}
        <div ref={containerRef} className="flex-1 relative min-h-0 overflow-hidden">
          {graphNodes.length === 0 ? (
            <div className="flex items-center justify-center h-full text-mapit-muted text-sm">
              No reachable functions within depth {maxDepth}
            </div>
          ) : (
            <>
              <ForceGraph2D
                ref={fgRef}
                graphData={graphData}
                width={dims.w}
                height={dims.h}
                nodeVal="val"
                nodeColor={nodeColor}
                nodeCanvasObject={nodeCanvasObject}
                nodeCanvasObjectMode={() => "replace"}
                linkColor={linkColor}
                linkWidth={1}
                linkDirectionalParticles={linkParticles}
                linkDirectionalParticleWidth={2.5}
                linkDirectionalParticleColor={() => "#5b8def"}
                linkDirectionalParticleSpeed={0.008}
                onNodeClick={(node: any) => {
                  dispatch({
                    type: "SET_OVERLAY",
                    overlay: { kind: "function_detail", node_id: node.id as string },
                  });
                }}
                backgroundColor="#0b0d12"
                d3AlphaDecay={0.018}
                d3VelocityDecay={0.35}
                warmupTicks={80}
                cooldownTicks={200}
              />

              {/* Legend */}
              <div className="absolute top-3 left-3 bg-mapit-surface/90 border border-mapit-border rounded-lg px-3 py-2 text-xs space-y-1.5 backdrop-blur-sm pointer-events-none">
                <p className="text-mapit-muted font-semibold mb-0.5">Node state</p>
                {[
                  { color: COLOR.active,    label: "Active (executing now)"  },
                  { color: COLOR.inStack,   label: "On call stack"           },
                  { color: COLOR.completed, label: "Returned"                },
                  { color: COLOR.inactive,  label: "Not yet reached"         },
                ].map(({ color, label }) => (
                  <div key={label} className="flex items-center gap-1.5">
                    <span className="w-2.5 h-2.5 rounded-full inline-block flex-shrink-0" style={{ background: color }} />
                    <span className="text-mapit-muted">{label}</span>
                  </div>
                ))}
                <p className="text-mapit-muted/60 mt-1 text-[10px]">Click any node → detail panel</p>
              </div>

              {/* Current step info banner */}
              {currentStep && (
                <div className="absolute bottom-3 left-3 right-3 pointer-events-none">
                  <div className="bg-mapit-surface/95 border border-mapit-border rounded-lg px-3 py-2 flex items-center gap-2">
                    <span
                      className={`text-lg flex-shrink-0 font-bold leading-none ${STEP_META[currentStep.kind].color}`}
                    >
                      {STEP_META[currentStep.kind].sym}
                    </span>
                    <div className="flex-1 min-w-0">
                      <span className="text-sm font-semibold text-mapit-text font-mono">
                        {currentStep.kind === "call"
                          ? `${currentStep.nodeName} → ${currentStep.calleeName}`
                          : currentStep.nodeName}
                      </span>
                      {currentStep.filePath && (
                        <span className="text-xs text-mapit-muted font-mono ml-2">
                          {currentStep.filePath}
                          {currentStep.startLine ? `:${currentStep.startLine}` : ""}
                        </span>
                      )}
                    </div>
                    <span className="flex-shrink-0 text-xs text-mapit-muted bg-mapit-surface2 border border-mapit-border rounded px-1.5 py-0.5 font-mono">
                      {
                        { enter: "ENTER", call: "CALL", return: "RETURN",
                          cycle: "CYCLE", maxdepth: "DEPTH LIMIT", external: "EXTERNAL"
                        }[currentStep.kind]
                      }
                    </span>
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Small control button helper ────────────────────────────────────────────────

function CtrlBtn({
  children,
  onClick,
  disabled = false,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      className="p-1.5 rounded text-mapit-muted hover:text-mapit-text hover:bg-mapit-surface2 transition-colors disabled:opacity-30 disabled:cursor-not-allowed focus:ring-1 focus:ring-mapit-accent focus:outline-none"
      onClick={onClick}
      disabled={disabled}
      title={title}
    >
      {children}
    </button>
  );
}
