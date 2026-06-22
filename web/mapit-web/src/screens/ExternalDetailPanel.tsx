import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { ExternalNode, Node } from "../types";

const REASON_LABELS: Record<ExternalNode["reason"], string> = {
  no_source_present: "No source file present in the project",
  dynamic_dispatch: "Dynamic dispatch — target resolved at runtime",
  unrecognized_binding: "Unrecognized binding — could not resolve statically",
};

export function ExternalDetailPanel() {
  const { state, dispatch } = useAppState();
  const [fetchedNode, setFetchedNode] = useState<Node | null>(null);

  const nodeId =
    state.overlay?.kind === "external_detail" ? state.overlay.node_id : null;

  useEffect(() => {
    if (!nodeId) {
      setFetchedNode(null);
      return;
    }
    const fromStore = state.allNodes.get(nodeId);
    if (fromStore) {
      setFetchedNode(fromStore);
      return;
    }
    api.node(nodeId).then(setFetchedNode).catch(console.error);
  }, [nodeId, state.allNodes]);

  if (!nodeId) return null;

  const node = (state.allNodes.get(nodeId) || fetchedNode) as ExternalNode | undefined;
  if (!node) return <div className="p-4 text-mapit-muted">Loading…</div>;

  return (
    <div className="w-96 bg-mapit-surface border-l border-mapit-border h-full overflow-y-auto shadow-xl">
      <div className="p-4">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-mapit-text truncate">{node.name}</h2>
          <button
            type="button"
            className="text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
            onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          >
            ✕
          </button>
        </div>

        <div className="space-y-4 text-sm">
          <div>
            <span className="text-xs font-mono text-mapit-node-external uppercase">external</span>
          </div>

          <div>
            <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">Reason</h3>
            <p className="text-mapit-text">{REASON_LABELS[node.reason] ?? node.reason}</p>
          </div>

          {node.ai_summary_status === "ready" && node.ai_summary && (
            <div>
              <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">AI Summary</h3>
              <p className="text-mapit-text">{node.ai_summary}</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
