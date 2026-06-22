import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { FunctionNode, Node } from "../types";

export function FunctionDetailPanel() {
  const { state, dispatch } = useAppState();
  const [fetchedNode, setFetchedNode] = useState<Node | null>(null);

  if (!state.overlay || state.overlay.kind !== "function_detail") return null;
  const nodeId = state.overlay.node_id;

  useEffect(() => {
    const fromStore = state.allNodes.get(nodeId);
    if (fromStore) {
      setFetchedNode(fromStore);
      return;
    }
    api.node(nodeId).then(setFetchedNode).catch(console.error);
  }, [nodeId, state.allNodes]);

  const node = (state.allNodes.get(nodeId) || fetchedNode) as FunctionNode | undefined;
  if (!node) return <div className="p-4 text-mapit-muted">Loading…</div>;

  const callers = state.allEdges
    .filter((e) => e.to_id === node.id && e.type === "calls")
    .map((e) => state.allNodes.get(e.from_id))
    .filter(Boolean);

  const callees = state.allEdges
    .filter((e) => e.from_id === node.id && e.type === "calls")
    .map((e) => state.allNodes.get(e.to_id))
    .filter(Boolean);

  return (
    <div className="w-96 bg-mapit-surface border-l border-mapit-border h-full overflow-y-auto">
      <div className="p-4">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-mapit-text truncate">{node.name}</h2>
          <button
            className="text-mapit-muted hover:text-mapit-text"
            onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          >
            ✕
          </button>
        </div>

        <div className="space-y-4 text-sm">
          <div>
            <p className="font-mono text-xs text-mapit-accent">{node.signature}</p>
            {node.file_path && (
              <p className="text-mapit-muted text-xs mt-1">
                {node.file_path}
                {node.span && `:${node.span.start_line}`}
              </p>
            )}
          </div>

          <div>
            <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">AI Summary</h3>
            {node.ai_summary_status === "ready" && node.ai_summary ? (
              <p className="text-mapit-text">{node.ai_summary}</p>
            ) : node.ai_summary_status === "pending" ? (
              <p className="text-mapit-muted italic">Summary pending…</p>
            ) : (
              <p className="text-mapit-muted italic">
                No AI summary — configure a provider with `mapit config set-provider` to enable
                explanations
              </p>
            )}
          </div>

          <div>
            <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
              Callers ({callers.length})
            </h3>
            <div className="space-y-1">
              {callers.map((c) => (
                <button
                  key={c!.id}
                  className="block w-full text-left px-2 py-1 rounded bg-mapit-bg hover:bg-mapit-border
                             text-mapit-text text-xs transition-colors"
                  onClick={() =>
                    dispatch({
                      type: "SET_OVERLAY",
                      overlay: { kind: "function_detail", node_id: c!.id },
                    })
                  }
                >
                  {c!.name}
                </button>
              ))}
              {callers.length === 0 && (
                <p className="text-mapit-muted text-xs italic">No incoming calls</p>
              )}
            </div>
          </div>

          <div>
            <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
              Callees ({callees.length})
            </h3>
            <div className="space-y-1">
              {callees.map((c) => (
                <button
                  key={c!.id}
                  className="block w-full text-left px-2 py-1 rounded bg-mapit-bg hover:bg-mapit-border
                             text-mapit-text text-xs transition-colors"
                  onClick={() =>
                    dispatch({
                      type: "SET_OVERLAY",
                      overlay: { kind: "function_detail", node_id: c!.id },
                    })
                  }
                >
                  {c!.name}
                </button>
              ))}
              {callees.length === 0 && (
                <p className="text-mapit-muted text-xs italic">No outgoing calls</p>
              )}
            </div>
          </div>

          {node.flaws.length > 0 && (
            <div>
              <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
                Flaws ({node.flaws.length})
              </h3>
              <div className="space-y-1">
                {node.flaws.map((f) => (
                  <div
                    key={f.id}
                    className={`px-2 py-1 rounded text-xs ${
                      f.severity === "high"
                        ? "bg-red-900/30 text-red-300"
                        : f.severity === "warning"
                          ? "bg-yellow-900/30 text-yellow-300"
                          : "bg-gray-800 text-mapit-muted"
                    }`}
                  >
                    <span className="font-semibold">{f.kind}</span>: {f.description}
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="flex gap-2">
            <button
              className="flex-1 px-3 py-1.5 text-xs rounded bg-mapit-accent text-white hover:opacity-90 transition-opacity"
              onClick={() =>
                dispatch({
                  type: "SET_OVERLAY",
                  overlay: { kind: "trace_view", node_id: node.id },
                })
              }
            >
              Trace from here
            </button>
            <button
              className="flex-1 px-3 py-1.5 text-xs rounded bg-mapit-bg border border-mapit-border
                         text-mapit-text hover:border-mapit-accent transition-colors"
              onClick={() =>
                dispatch({
                  type: "SET_OVERLAY",
                  overlay: { kind: "neighbors", node_id: node.id },
                })
              }
            >
              Show call tree
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
