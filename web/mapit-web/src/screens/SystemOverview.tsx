
import { useState, useEffect, useMemo } from "react";
import { TopBar } from "../components/TopBar";
import { Breadcrumb } from "../components/Breadcrumb";
import { GraphView } from "../components/GraphView";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { Node, Edge } from "../types";

export function SystemOverview() {
  const { state, dispatch } = useAppState();

  // Use state.breadcrumb directly (no local ZoomEntry!)
  const zoomStack = state.breadcrumb;

  const [zoomNodes, setZoomNodes] = useState<Node[]>([]);
  const [zoomEdges, setZoomEdges] = useState<Edge[]>([]);
  const [zoomLoading, setZoomLoading] = useState(false);

  const allNodesArr = useMemo(() => Array.from(state.allNodes.values()), [state.allNodes]);

  // Root nodes = features, or files if no features (§2.1)
  const rootNodes = useMemo(() => {
    if (state.features.length > 0) {
      return state.features;
    }
    return allNodesArr.filter((n): n is Node => n.type === "file");
  }, [state.features, allNodesArr]);

  // Root edges = edges between root nodes and others
  const rootEdges = useMemo(() => {
    const nodeIds = new Set(rootNodes.map(n => n.id));
    return state.allEdges.filter(e => nodeIds.has(e.from_id) || nodeIds.has(e.to_id));
  }, [rootNodes, state.allEdges]);

  const isZoomed = zoomStack.length > 0;
  const displayNodes = isZoomed ? zoomNodes : rootNodes;
  const displayEdges = isZoomed ? zoomEdges : rootEdges;

  // Load neighbors when zoom stack changes
  useEffect(() => {
    if (zoomStack.length === 0) {
      setZoomNodes([]);
      setZoomEdges([]);
      return;
    }
    const entry = zoomStack[zoomStack.length - 1];
    if (!entry.node_id) return;

    let cancelled = false;
    setZoomLoading(true);
    api
      .neighbors(entry.node_id, "both", 1)
      .then((neighbors) => {
        if (cancelled) return;
        setZoomNodes(neighbors.nodes);
        setZoomEdges(neighbors.edges);
      })
      .catch(console.error)
      .finally(() => {
        if (!cancelled) setZoomLoading(false);
      });
    return () => { cancelled = true; };
  }, [zoomStack]);

  const handleNodeClick = (node: Node) => {
    if (node.type === "feature" || node.type === "file") {
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [...zoomStack, { label: node.name, node_id: node.id }]
      });
    } else if (node.type === "function") {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: node.id },
      });
    }
  };

  return (
    <div className="flex flex-col h-screen bg-mapit-bg">
      <TopBar />
      <Breadcrumb />

      <div className="flex-1 relative">
        {zoomLoading ? (
          <div className="flex items-center justify-center h-full">
            <div className="flex flex-col items-center gap-3">
              <div className="w-10 h-10 border-3 border-mapit-accent border-t-transparent rounded-full animate-spin" />
              <div className="text-sm text-mapit-muted">Loading...</div>
            </div>
          </div>
        ) : displayNodes.length > 0 ? (
          <GraphView
            nodes={displayNodes}
            edges={displayEdges}
            onNodeClick={handleNodeClick}
            onBackgroundClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
          />
        ) : (
          <div className="flex flex-col items-center justify-center h-full text-mapit-muted gap-3">
            <div className="text-xl font-medium">No map data found</div>
            <div className="text-sm">
              Run <code className="bg-mapit-surface2 px-2 py-1 rounded border border-mapit-border font-mono">mapit map</code> first.
            </div>
          </div>
        )}

        {/* Stats Bar */}
        <div className="absolute bottom-4 left-4 flex gap-3">
          <div className="text-xs text-mapit-muted bg-mapit-surface border border-mapit-border px-4 py-2 rounded shadow">
            {state.project ? (
              isZoomed ? (
                <span>
                  {zoomStack[zoomStack.length - 1].label} · {zoomNodes.length} symbols
                </span>
              ) : (
                <span>
                  {state.project.file_count} files · {state.project.symbol_count} symbols · {state.project.edge_count} edges
                </span>
              )
            ) : (
              ""
            )}
          </div>
          {state.project?.ai_annotation_coverage_pct && (
            <div className="text-xs text-mapit-muted bg-mapit-surface border border-mapit-border px-4 py-2 rounded shadow">
              AI Coverage: {state.project.ai_annotation_coverage_pct.toFixed(0)}%
            </div>
          )}
        </div>
      </div>

      {state.flaws.length > 0 && (
        <div className="absolute bottom-4 right-4">
          <button
            onClick={() => dispatch({ type: "SET_SCREEN", screen: "flaws_report" })}
            className="flex items-center gap-2 bg-mapit-surface border border-mapit-border rounded-lg px-5 py-3 shadow-xl hover:bg-mapit-surface2 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-mapit-danger"
            >
              <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
              <line x1="12" y1="9" x2="12" y2="13" />
              <line x1="12" y1="17" x2="12.01" y2="17" />
            </svg>
            <div className="text-left">
              <div className="text-xs font-semibold text-mapit-muted">Flaws Found</div>
              <div className="text-sm font-medium text-mapit-text">
                {state.flaws.length} issue{state.flaws.length !== 1 ? "s" : ""}
              </div>
            </div>
          </button>
        </div>
      )}
    </div>
  );
}
