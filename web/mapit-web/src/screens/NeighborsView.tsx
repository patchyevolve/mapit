import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import { GraphView } from "../components/GraphView";
import type { Node, Edge } from "../types";

export function NeighborsView() {
  const { state, dispatch } = useAppState();
  const [centerNode, setCenterNode] = useState<Node | null>(null);
  const [neighborNodes, setNeighborNodes] = useState<Node[]>([]);
  const [neighborEdges, setNeighborEdges] = useState<Edge[]>([]);
  const [loading, setLoading] = useState(true);

  const nodeId = state.overlay?.kind === "neighbors" ? state.overlay.node_id : null;

  useEffect(() => {
    if (!nodeId) return;
    setLoading(true);

    Promise.all([
      api.node(nodeId),
      api.neighbors(nodeId, "both", 2),
    ])
      .then(([node, neighbors]) => {
        setCenterNode(node);
        setNeighborNodes(neighbors.nodes);
        setNeighborEdges(neighbors.edges);
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [nodeId]);

  const allNodes = centerNode ? [centerNode, ...neighborNodes] : neighborNodes;

  const handleNodeClick = (n: Node) => {
    if (n.type === "function") {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: n.id },
      });
    }
  };

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-mapit-text">
            Call Tree: {centerNode?.name || nodeId?.slice(0, 12)}
          </span>
          {!loading && (
            <span className="text-xs text-mapit-muted">
              {neighborNodes.length} neighbors · {neighborEdges.length} edges
            </span>
          )}
        </div>
        <button
          className="text-mapit-muted hover:text-mapit-text"
          onClick={() =>
            dispatch({
              type: "SET_OVERLAY",
              overlay: centerNode
                ? { kind: "function_detail", node_id: centerNode.id }
                : null,
            })
          }
        >
          Back
        </button>
      </div>

      <div className="flex-1 relative">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <div className="w-6 h-6 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
          </div>
        ) : (
          <GraphView
            nodes={allNodes}
            edges={neighborEdges}
            onNodeClick={handleNodeClick}
            onBackgroundClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          />
        )}
      </div>
    </div>
  );
}
