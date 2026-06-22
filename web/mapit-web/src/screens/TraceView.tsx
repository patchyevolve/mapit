import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { TraceResponse, TraceStep } from "../types";

export function TraceView() {
  const { state, dispatch } = useAppState();
  const [trace, setTrace] = useState<TraceResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const nodeId = state.overlay?.kind === "trace_view" ? state.overlay.node_id : null;

  useEffect(() => {
    if (!nodeId) return;
    setLoading(true);
    api
      .trace(nodeId, 6)
      .then(setTrace)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [nodeId]);

  const entryNode = nodeId ? state.allNodes.get(nodeId) : null;

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <div className="flex items-center gap-2">
          <button
            className="text-mapit-muted hover:text-mapit-text text-sm mr-1"
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
            Execution Trace: {entryNode?.name || nodeId?.slice(0, 12)}
          </span>
          {trace && (
            <span className="text-xs text-mapit-muted">
              {trace.steps.length} steps
            </span>
          )}
        </div>
        <button
          className="text-mapit-muted hover:text-mapit-text"
          onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <div className="w-6 h-6 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
          </div>
        ) : trace ? (
          <div className="space-y-3 max-w-3xl mx-auto">
            <div className="text-xs text-mapit-muted mb-4">
              Entry point: <span className="text-mapit-text font-mono">{entryNode?.name || nodeId}</span>
              {trace.truncated_at_depth && (
                <span className="text-mapit-warning ml-2">(truncated at max depth)</span>
              )}
            </div>

            {trace.steps.map((step, i) => (
              <TraceStepCard key={i} step={step} index={i} />
            ))}
          </div>
        ) : (
          <div className="flex items-center justify-center h-full text-mapit-muted">
            No trace data available for this node
          </div>
        )}
      </div>
    </div>
  );
}

function TraceStepCard({ step, index }: { step: TraceStep; index: number }) {
  const funcName = step.label || step.block_id;
  const calls = step.calls || [];

  return (
    <div className="bg-mapit-surface border border-mapit-border rounded-lg p-3">
      <div className="flex items-start gap-3">
        <span className="flex-shrink-0 w-6 h-6 rounded-full bg-mapit-accent text-white text-xs flex items-center justify-center font-bold">
          {index + 1}
        </span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold text-mapit-text font-mono">
              {funcName}
            </span>
            <span className="text-xs text-mapit-muted">{step.block_id}</span>
          </div>

          {calls.length > 0 && (
            <div className="mt-2 space-y-1">
              {calls.map((call, ci) => (
                <div
                  key={ci}
                  className="flex items-center gap-2 text-xs text-mapit-muted ml-4"
                >
                  <span className="text-mapit-accent">→</span>
                  <span className="text-mapit-text">
                    {call.node?.name || "(unknown)"}
                  </span>
                  <span className="text-mapit-muted">
                    (order: {call.order_hint})
                  </span>
                </div>
              ))}
            </div>
          )}

          {step.branches && step.branches.length > 1 && (
            <div className="mt-2 space-y-1 ml-4 border-l-2 border-mapit-warning/30 pl-2">
              <span className="text-xs text-mapit-warning">Branches:</span>
              {step.branches.map((b, bi) => (
                <div key={bi} className="text-xs text-mapit-muted">
                  {b.condition ? (
                    <span>
                      IF <span className="text-mapit-warning font-mono">{b.condition}</span>
                      {" → "}{b.next_block_id}
                    </span>
                  ) : (
                    <span>→ {b.next_block_id}</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
