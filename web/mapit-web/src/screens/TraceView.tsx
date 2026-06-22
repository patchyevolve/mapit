import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { Node, TraceResponse, TraceStep } from "../types";

// ── Tree structure ────────────────────────────────────────────────────────────

interface TreeNode {
  step: TraceStep;
  children: { condition: string | null | undefined; node: TreeNode }[];
}

function buildTree(steps: TraceStep[]): TreeNode | null {
  if (steps.length === 0) return null;
  const stepMap = new Map(steps.map((s) => [s.block_id, s]));

  function buildNode(blockId: string, visited: Set<string>): TreeNode | null {
    if (visited.has(blockId)) return null; // cycle guard
    const step = stepMap.get(blockId);
    if (!step) return null;
    const nextVisited = new Set(visited);
    nextVisited.add(blockId);

    const children = (step.branches ?? [])
      .map((b) => ({
        condition: b.condition,
        node: buildNode(b.next_block_id, new Set(nextVisited)),
      }))
      .filter(
        (c): c is { condition: string | null | undefined; node: TreeNode } =>
          c.node !== null,
      );

    return { step, children };
  }

  return buildNode(steps[0].block_id, new Set());
}

// ── Step box ──────────────────────────────────────────────────────────────────

function StepBox({ step, stepNum }: { step: TraceStep; stepNum: number }) {
  const calls = step.calls ?? [];
  return (
    <div className="bg-mapit-surface border border-mapit-border rounded-lg p-3 min-w-[200px] max-w-[280px]">
      <div className="flex items-center gap-2 mb-1">
        <span className="flex-shrink-0 w-5 h-5 rounded-full bg-mapit-accent text-white text-xs flex items-center justify-center font-bold">
          {stepNum}
        </span>
        <span className="text-xs font-mono text-mapit-text truncate">
          {step.label || step.block_id}
        </span>
      </div>
      {calls.length > 0 && (
        <div className="mt-1 space-y-0.5 ml-7">
          {calls
            .slice()
            .sort((a, b) => a.order_hint - b.order_hint)
            .map((call, i) => (
              <div
                key={i}
                className="flex items-center gap-1 text-xs text-mapit-muted"
              >
                <span className="text-mapit-accent">→</span>
                <span className="text-mapit-text font-mono">
                  {call.node?.name ?? "(unknown)"}
                </span>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

// ── Recursive tree renderer ───────────────────────────────────────────────────

function TreeNodeView({
  node,
  depth,
  maxDepth,
  stepNumMap,
}: {
  node: TreeNode;
  depth: number;
  maxDepth: number;
  stepNumMap: Map<string, number>;
}) {
  const stepNum = stepNumMap.get(node.step.block_id) ?? 0;

  if (depth > maxDepth) {
    return (
      <div className="flex flex-col items-center">
        <div className="bg-mapit-surface2 border border-mapit-border rounded px-3 py-1 text-xs text-mapit-muted italic">
          … (depth limit reached)
        </div>
      </div>
    );
  }

  const hasBranch = node.children.length > 1;
  const hasSingle = node.children.length === 1;

  return (
    <div className="flex flex-col items-center">
      <StepBox step={node.step} stepNum={stepNum} />

      {hasSingle && (
        <>
          <div className="w-px h-4 bg-mapit-border" />
          <TreeNodeView
            node={node.children[0].node}
            depth={depth + 1}
            maxDepth={maxDepth}
            stepNumMap={stepNumMap}
          />
        </>
      )}

      {hasBranch && (
        <>
          {/* Vertical stem before fork */}
          <div className="w-px h-4 bg-mapit-border" />
          <div className="flex items-start">
            {node.children.map((child, i) => (
              <div key={i} className="flex flex-col items-center px-4">
                {/* Condition label */}
                <div className="bg-mapit-surface2 border border-mapit-warning/40 rounded px-2 py-0.5 text-xs font-mono text-mapit-warning max-w-[180px] truncate text-center">
                  {child.condition || (i === 0 ? "true / yes" : "false / no")}
                </div>
                <div className="w-px h-3 bg-mapit-border" />
                <TreeNodeView
                  node={child.node}
                  depth={depth + 1}
                  maxDepth={maxDepth}
                  stepNumMap={stepNumMap}
                />
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

// ── Fallback when CFG is trivial ─────────────────────────────────────────────

function CalleesFallback({
  nodeId,
  entryNode,
}: {
  nodeId: string;
  entryNode: Node | undefined;
}) {
  const { dispatch } = useAppState();
  const [callees, setCallees] = useState<Node[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api
      .neighbors(nodeId, "callees", 1)
      .then((res) => {
        setCallees(res.nodes.filter((n) => n.id !== nodeId));
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [nodeId]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-32 gap-2 text-mapit-muted">
        <div className="w-5 h-5 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
        Loading call data…
      </div>
    );
  }

  if (callees.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 text-mapit-muted">
        <svg
          width="40"
          height="40"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
        >
          <path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z" />
          <path d="M12 8v4m0 4h.01" />
        </svg>
        <p className="text-sm">No outgoing calls recorded.</p>
        <p className="text-xs max-w-xs text-center">
          This function appears to be a leaf — it either makes no calls to
          parsed functions, or detailed control flow data was not extracted for
          it.
        </p>
      </div>
    );
  }

  return (
    <div className="max-w-xl mx-auto space-y-3 p-4">
      <div className="text-xs text-mapit-muted mb-4 italic">
        No detailed control flow data was extracted for this function. Showing
        direct callee edges from the graph.
      </div>

      {/* Entry function box */}
      <div className="bg-mapit-surface border-2 border-mapit-accent rounded-lg p-3 flex items-center gap-2">
        <span className="text-xs text-mapit-muted font-mono">entry</span>
        <span className="text-sm font-mono font-semibold text-mapit-text">
          {entryNode?.name ?? nodeId.slice(0, 12)}
        </span>
        {entryNode?.file_path && (
          <span className="text-xs text-mapit-muted font-mono ml-auto">
            {entryNode.file_path.split("/").slice(-1)[0]}
            {entryNode.span ? `:${entryNode.span.start_line}` : ""}
          </span>
        )}
      </div>

      {/* Arrow */}
      <div className="flex flex-col items-center gap-0.5">
        <div className="w-px h-4 bg-mapit-accent/50" />
        <div className="text-xs text-mapit-muted">calls ({callees.length})</div>
        <div className="w-px h-2 bg-mapit-accent/50" />
      </div>

      {/* Callees grid */}
      <div className="grid gap-1.5">
        {callees.map((callee, i) => (
          <button
            key={callee.id}
            type="button"
            className="flex items-center gap-2 bg-mapit-surface border border-mapit-border rounded-lg px-3 py-2 hover:border-mapit-accent/50 hover:bg-mapit-surface2 transition-colors text-left focus:ring-1 focus:ring-mapit-accent focus:outline-none"
            onClick={() =>
              dispatch({
                type: "SET_OVERLAY",
                overlay:
                  callee.type === "external"
                    ? { kind: "external_detail", node_id: callee.id }
                    : { kind: "function_detail", node_id: callee.id },
              })
            }
          >
            <span className="flex-shrink-0 w-5 h-5 rounded-full bg-mapit-accent/20 text-mapit-accent text-xs flex items-center justify-center font-bold">
              {i + 1}
            </span>
            <span
              className={`text-xs font-mono flex-shrink-0 ${
                callee.type === "external"
                  ? "text-mapit-muted"
                  : "text-mapit-node-function"
              }`}
            >
              {callee.type === "external" ? "ext" : "fn"}
            </span>
            <span className="text-sm font-mono text-mapit-text flex-1 truncate">
              {callee.name}
            </span>
            {callee.file_path && (
              <span className="flex-shrink-0 text-xs text-mapit-muted font-mono">
                {callee.file_path.split("/").slice(-1)[0]}
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Screen ────────────────────────────────────────────────────────────────────

export function TraceView() {
  const { state, dispatch } = useAppState();
  const [trace, setTrace] = useState<TraceResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [maxDepth, setMaxDepth] = useState(5);

  const nodeId =
    state.overlay?.kind === "trace_view" ? state.overlay.node_id : null;

  useEffect(() => {
    if (!nodeId) return;
    setLoading(true);
    api
      .trace(nodeId, 10)
      .then(setTrace)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [nodeId]);

  const entryNode = nodeId ? state.allNodes.get(nodeId) : undefined;

  const isTrivial =
    !trace ||
    trace.steps.length === 0 ||
    (trace.steps.length === 1 &&
      (trace.steps[0].calls ?? []).length === 0 &&
      (trace.steps[0].branches ?? []).length === 0);

  const tree = !isTrivial && trace ? buildTree(trace.steps) : null;

  // Pre-compute step numbers from flat steps array — stable, no mutation needed
  const stepNumMap = trace
    ? new Map(trace.steps.map((s, i) => [s.block_id, i + 1]))
    : new Map<string, number>();

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
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
          <span className="text-sm font-semibold text-mapit-text">
            Execution Trace:{" "}
            <span className="font-mono">
              {entryNode?.name ?? nodeId?.slice(0, 12)}
            </span>
          </span>
          {trace && (
            <span className="text-xs text-mapit-muted">
              {trace.steps.length} block{trace.steps.length !== 1 ? "s" : ""}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-xs text-mapit-muted">
            Depth
            <input
              type="range"
              min={1}
              max={10}
              value={maxDepth}
              onChange={(e) => setMaxDepth(Number(e.target.value))}
              className="w-20 accent-mapit-accent"
            />
            <span className="w-4 text-mapit-text">{maxDepth}</span>
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

      {/* Body */}
      <div className="flex-1 overflow-auto p-6">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <div className="w-6 h-6 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
          </div>
        ) : isTrivial ? (
          <CalleesFallback nodeId={nodeId!} entryNode={entryNode} />
        ) : (
          <div className="flex flex-col items-center">
            {trace?.truncated_at_depth && (
              <div className="mb-4 text-xs text-mapit-warning bg-mapit-warning/10 border border-mapit-warning/30 px-3 py-1.5 rounded">
                Trace truncated at max depth. Increase depth or use "Show full
                call tree."
              </div>
            )}
            {tree && (
              <TreeNodeView
                node={tree}
                depth={0}
                maxDepth={maxDepth}
                stepNumMap={stepNumMap}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}
