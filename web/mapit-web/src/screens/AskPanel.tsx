import { useState, useEffect, useRef } from "react";
import { api } from "../api-client";
import { useAppState } from "../store";
import type { GroundingStatus } from "../types";

interface HistoryEntry {
  q: string;
  answer: string;
  grounding_status: GroundingStatus;
  referenced_node_ids: string[];
}

function GroundingBadge({ status }: { status: GroundingStatus }) {
  const config = {
    ok: {
      cls: "bg-mapit-success/20 text-mapit-success border-mapit-success/30",
      dot: "bg-mapit-success",
      label: "Grounded",
    },
    partial: {
      cls: "bg-mapit-warning/20 text-mapit-warning border-mapit-warning/30",
      dot: "bg-mapit-warning",
      label: "Partial",
    },
    no_relevant_context_found: {
      cls: "bg-mapit-danger/20 text-mapit-danger border-mapit-danger/30",
      dot: "bg-mapit-danger",
      label: "Weak",
    },
  }[status] ?? {
    cls: "bg-mapit-surface2 text-mapit-muted border-mapit-border",
    dot: "bg-mapit-muted",
    label: status,
  };
  return (
    <span
      className={`inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded border ${config.cls}`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${config.dot}`} />
      {config.label}
    </span>
  );
}

export function AskPanel() {
  const [q, setQ] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const active = useRef(true);
  const { state, dispatch } = useAppState();

  useEffect(() => () => { active.current = false; }, []);

  const handleAsk = async () => {
    if (!q.trim()) return;
    const question = q.trim();
    setQ("");
    setLoading(true);
    try {
      const res = await api.ask({ question });
      if (!active.current) return;
      setHistory((prev) => [
        ...prev,
        {
          q: question,
          answer: res.answer,
          grounding_status: res.grounding_status,
          referenced_node_ids: res.referenced_node_ids ?? [],
        },
      ]);
    } catch (e: unknown) {
      if (!active.current) return;
      setHistory((prev) => [
        ...prev,
        {
          q: question,
          answer: `Error: ${e instanceof Error ? e.message : "request failed"}. Make sure an AI provider is configured in Settings.`,
          grounding_status: "no_relevant_context_found",
          referenced_node_ids: [],
        },
      ]);
    } finally {
      if (active.current) setLoading(false);
    }
  };

  const navigateToNode = (nodeId: string) => {
    const node = state.allNodes.get(nodeId);
    if (!node) return;
    dispatch({ type: "SET_OVERLAY", overlay: null });
    if (
      node.type === "function" ||
      node.type === "type" ||
      node.type === "macro" ||
      node.type === "global"
    ) {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: nodeId },
      });
    } else if (node.type === "file" || node.type === "feature") {
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [{ label: node.name, node_id: nodeId }],
      });
      dispatch({
        type: "SET_SCREEN",
        screen: node.type === "feature" ? "expanded_feature" : "expanded_file",
      });
    }
  };

  return (
    <div className="border-t border-mapit-border bg-mapit-surface h-[50vh] flex flex-col shadow-2xl">
      <div className="flex items-center justify-between px-4 py-2 border-b border-mapit-border">
        <span className="text-sm font-semibold text-mapit-text">
          Ask the Codebase
        </span>
        <button
          type="button"
          className="text-mapit-muted hover:text-mapit-text text-xs focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded"
          onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {history.length === 0 && !loading && (
          <div className="text-center text-mapit-muted text-sm mt-4 space-y-1">
            <p>Ask a natural language question about the codebase.</p>
            <p className="text-xs">
              "Which functions handle network errors?" · "What does the init
              module do?" · "Where is memory allocated?"
            </p>
          </div>
        )}

        {history.map((item, idx) => (
          <div key={idx} className="space-y-2">
            <div className="flex items-start gap-2">
              <span className="flex-shrink-0 text-xs font-bold text-mapit-accent bg-mapit-accent/10 px-1.5 py-0.5 rounded">
                Q
              </span>
              <span className="text-sm text-mapit-text">{item.q}</span>
            </div>
            <div className="bg-mapit-surface2 border border-mapit-border rounded-lg p-3 space-y-2 ml-5">
              <div className="flex items-center gap-2">
                <span className="flex-shrink-0 text-xs font-bold text-mapit-success bg-mapit-success/10 px-1.5 py-0.5 rounded">
                  A
                </span>
                <GroundingBadge status={item.grounding_status} />
              </div>
              <p className="text-sm text-mapit-text whitespace-pre-wrap">
                {item.answer}
              </p>
              {item.referenced_node_ids.length > 0 && (
                <div className="pt-2 border-t border-mapit-border">
                  <p className="text-xs text-mapit-muted mb-1.5">Based on:</p>
                  <div className="flex flex-wrap gap-1.5">
                    {item.referenced_node_ids.map((id) => {
                      const node = state.allNodes.get(id);
                      return (
                        <button
                          key={id}
                          type="button"
                          onClick={() => navigateToNode(id)}
                          className="text-xs px-2 py-0.5 rounded bg-mapit-accent/10 text-mapit-accent border border-mapit-accent/20 hover:bg-mapit-accent/20 transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none"
                        >
                          {node?.name ?? id.slice(0, 8)}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
          </div>
        ))}

        {loading && (
          <div className="flex items-center gap-2 text-mapit-muted text-sm ml-5">
            <div className="w-4 h-4 border-2 border-mapit-accent border-t-transparent rounded-full animate-spin" />
            Thinking…
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 px-4 py-3 border-t border-mapit-border">
        <input
          type="text"
          placeholder="Ask about the codebase…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !loading && handleAsk()}
          className="flex-1 px-3 py-2 text-sm bg-mapit-bg border border-mapit-border rounded-lg
                     text-mapit-text placeholder-mapit-muted focus:outline-none focus:border-mapit-accent focus:ring-1 focus:ring-mapit-accent"
        />
        <button
          type="button"
          disabled={loading || !q.trim()}
          className="px-4 py-2 text-sm rounded-lg bg-mapit-accent text-white hover:opacity-90
                     disabled:opacity-50 transition-opacity flex items-center gap-1 focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          onClick={handleAsk}
        >
          {loading ? (
            <div className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
          ) : (
            "Send"
          )}
        </button>
      </div>
    </div>
  );
}
