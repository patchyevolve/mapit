import { useEffect, useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { FunctionNode, Node } from "../types";

export function FunctionDetailPanel() {
  const { state, dispatch } = useAppState();
  const [fetchedNode, setFetchedNode] = useState<Node | null>(null);
  const [sourceCode, setSourceCode] = useState<{
    content: string;
    start_line: number;
  } | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
  const [sourceOpen, setSourceOpen] = useState(false);

  const nodeId =
    state.overlay?.kind === "function_detail" ? state.overlay.node_id : null;

  useEffect(() => {
    if (!nodeId) {
      setFetchedNode(null);
      setSourceCode(null);
      setSourceOpen(false);
      return;
    }
    const fromStore = state.allNodes.get(nodeId);
    if (fromStore) {
      setFetchedNode(fromStore);
    } else {
      api.node(nodeId).then(setFetchedNode).catch(console.error);
    }
  }, [nodeId]);

  // Reset source panel when node changes
  useEffect(() => {
    setSourceCode(null);
    setSourceOpen(false);
  }, [nodeId]);

  const fetchSource = (node: FunctionNode) => {
    if (!node.file_path || !node.span) return;
    setSourceLoading(true);
    const context = 2; // extra lines of context around the function
    api
      .source(
        node.file_path,
        Math.max(1, node.span.start_line - context),
        node.span.end_line + context,
      )
      .then((res) => {
        setSourceCode({ content: res.content, start_line: res.start_line });
        setSourceOpen(true);
      })
      .catch(console.error)
      .finally(() => setSourceLoading(false));
  };

  const openNode = (target: Node) => {
    if (
      target.type === "function" ||
      target.type === "type" ||
      target.type === "macro" ||
      target.type === "global" ||
      target.type === "module"
    ) {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: target.id },
      });
    } else if (target.type === "external") {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "external_detail", node_id: target.id },
      });
    } else if (target.type === "file") {
      dispatch({ type: "SET_OVERLAY", overlay: null });
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [{ label: target.name, node_id: target.id }],
      });
      dispatch({ type: "SET_SCREEN", screen: "expanded_file" });
    }
  };

  if (!nodeId) return null;

  const node = (state.allNodes.get(nodeId) ?? fetchedNode) as
    | FunctionNode
    | undefined;
  if (!node)
    return <div className="p-4 text-mapit-muted text-sm">Loading…</div>;

  const callers = state.allEdges
    .filter((e) => e.to_id === node.id && e.type === "calls")
    .map((e) => state.allNodes.get(e.from_id))
    .filter(Boolean) as Node[];

  const callees = state.allEdges
    .filter((e) => e.from_id === node.id && e.type === "calls")
    .map((e) => state.allNodes.get(e.to_id))
    .filter(Boolean) as Node[];

  return (
    <div className="w-96 bg-mapit-surface border-l border-mapit-border h-full overflow-y-auto shadow-xl flex flex-col">
      {/* Sticky header */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-mapit-border bg-mapit-surface sticky top-0 z-10">
        <h2
          className="text-sm font-semibold text-mapit-text truncate"
          title={node.name}
        >
          {node.name}
        </h2>
        <button
          type="button"
          className="text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded ml-2 flex-shrink-0"
          onClick={() => dispatch({ type: "SET_OVERLAY", overlay: null })}
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4 text-sm">
        {/* Signature & location */}
        <div className="space-y-1">
          {node.signature && (
            <p className="font-mono text-xs text-mapit-accent bg-mapit-bg rounded px-2 py-1 break-all">
              {node.signature}
            </p>
          )}
          {node.file_path && (
            <p className="text-mapit-muted text-xs font-mono">
              {node.file_path}
              {node.span
                ? `:${node.span.start_line}–${node.span.end_line}`
                : ""}
            </p>
          )}
          {node.language && (
            <span className="inline-block text-xs text-mapit-muted bg-mapit-surface2 border border-mapit-border rounded px-1.5 py-0.5">
              {node.language}
            </span>
          )}
        </div>

        {/* AI Summary */}
        <div>
          <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
            Summary
          </h3>
          {node.ai_summary_status === "ready" && node.ai_summary ? (
            <p className="text-mapit-text text-sm leading-relaxed">
              {node.ai_summary}
            </p>
          ) : node.ai_summary_status === "pending" ? (
            <div className="flex items-center gap-2 text-mapit-muted text-xs italic">
              <span className="w-2 h-2 rounded-full bg-mapit-accent animate-pulse" />
              Summary pending…
            </div>
          ) : (
            <p className="text-mapit-muted text-xs italic">
              No AI summary — configure a provider in{" "}
              <button
                type="button"
                className="text-mapit-accent underline"
                onClick={() =>
                  dispatch({ type: "SET_SCREEN", screen: "settings" })
                }
              >
                Settings
              </button>
            </p>
          )}
        </div>

        {/* Callers */}
        <div>
          <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
            Callers ({callers.length})
          </h3>
          <div className="space-y-1">
            {callers.length === 0 ? (
              <p className="text-mapit-muted text-xs italic">
                No incoming calls
              </p>
            ) : (
              callers.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className="block w-full text-left px-2 py-1 rounded bg-mapit-bg hover:bg-mapit-surface2 text-mapit-text text-xs transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none font-mono"
                  onClick={() => openNode(c)}
                >
                  <span
                    className={`text-xs font-mono mr-1 ${
                      c.type === "function"
                        ? "text-mapit-node-function"
                        : c.type === "external"
                          ? "text-mapit-node-external"
                          : c.type === "file"
                            ? "text-mapit-node-file"
                            : "text-mapit-muted"
                    }`}
                  >
                    {c.type === "function"
                      ? "fn "
                      : c.type === "file"
                        ? "📄 "
                        : c.type === "external"
                          ? "ext "
                          : ""}
                  </span>
                  {c.name}
                </button>
              ))
            )}
          </div>
        </div>

        {/* Callees */}
        <div>
          <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
            Callees ({callees.length})
          </h3>
          <div className="space-y-1">
            {callees.length === 0 ? (
              <p className="text-mapit-muted text-xs italic">
                No outgoing calls
              </p>
            ) : (
              callees.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className="block w-full text-left px-2 py-1 rounded bg-mapit-bg hover:bg-mapit-surface2 text-mapit-text text-xs transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none font-mono"
                  onClick={() => openNode(c)}
                >
                  <span
                    className={`text-xs font-mono mr-1 ${
                      c.type === "function"
                        ? "text-mapit-node-function"
                        : c.type === "external"
                          ? "text-mapit-node-external"
                          : c.type === "file"
                            ? "text-mapit-node-file"
                            : "text-mapit-muted"
                    }`}
                  >
                    {c.type === "function"
                      ? "fn "
                      : c.type === "file"
                        ? "📄 "
                        : c.type === "external"
                          ? "ext "
                          : ""}
                  </span>
                  {c.name}
                </button>
              ))
            )}
          </div>
        </div>

        {/* Flaws */}
        {node.flaws && node.flaws.length > 0 && (
          <div>
            <h3 className="text-mapit-muted text-xs uppercase tracking-wider mb-1">
              Flaws ({node.flaws.length})
            </h3>
            <div className="space-y-1">
              {node.flaws.map((f) => (
                <div
                  key={f.id}
                  className={`px-2 py-1.5 rounded text-xs border ${
                    f.severity === "high"
                      ? "bg-mapit-danger/10 text-mapit-danger border-mapit-danger/30"
                      : f.severity === "warning"
                        ? "bg-mapit-warning/10 text-mapit-warning border-mapit-warning/30"
                        : "bg-mapit-surface2 text-mapit-muted border-mapit-border"
                  }`}
                >
                  <span className="font-semibold uppercase">{f.severity}</span>
                  {" · "}
                  <span>{f.kind.replace(/_/g, " ")}</span>
                  <p className="mt-0.5 font-normal">{f.description}</p>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Source Code */}
        {node.file_path && node.span && (
          <div>
            <div className="flex items-center justify-between mb-1">
              <h3 className="text-mapit-muted text-xs uppercase tracking-wider">
                Source Code
              </h3>
              {!sourceOpen && (
                <button
                  type="button"
                  disabled={sourceLoading}
                  onClick={() => fetchSource(node)}
                  className="text-xs text-mapit-accent hover:text-mapit-text transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none disabled:opacity-50"
                >
                  {sourceLoading ? "Loading…" : "View source"}
                </button>
              )}
              {sourceOpen && (
                <button
                  type="button"
                  onClick={() => setSourceOpen(false)}
                  className="text-xs text-mapit-muted hover:text-mapit-text transition-colors"
                >
                  Hide
                </button>
              )}
            </div>
            {sourceOpen && sourceCode && (
              <div className="bg-mapit-bg border border-mapit-border rounded overflow-auto max-h-80">
                <pre className="text-xs font-mono text-mapit-text p-3 leading-relaxed">
                  {sourceCode.content.split("\n").map((line, i) => (
                    <div key={i} className="flex">
                      <span className="select-none text-mapit-muted w-10 text-right pr-3 flex-shrink-0">
                        {sourceCode.start_line + i}
                      </span>
                      <span>{line}</span>
                    </div>
                  ))}
                </pre>
              </div>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex flex-col gap-1.5 pt-1">
          <div className="flex gap-2">
            <button
              type="button"
              className="flex-1 px-3 py-1.5 text-xs rounded bg-mapit-accent text-white hover:opacity-90 transition-opacity focus:ring-2 focus:ring-mapit-accent focus:outline-none"
              onClick={() =>
                dispatch({
                  type: "SET_OVERLAY",
                  overlay: { kind: "trace_view", node_id: node.id },
                })
              }
            >
              ▶ Trace
            </button>
            <button
              type="button"
              className="flex-1 px-3 py-1.5 text-xs rounded bg-mapit-bg border border-mapit-border text-mapit-text hover:border-mapit-accent transition-colors focus:ring-2 focus:ring-mapit-accent focus:outline-none"
              onClick={() =>
                dispatch({
                  type: "SET_OVERLAY",
                  overlay: { kind: "neighbors", node_id: node.id },
                })
              }
            >
              Call tree
            </button>
          </div>
          <button
            type="button"
            className="w-full px-3 py-2 text-xs rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:border-mapit-accent/60 hover:bg-mapit-surface transition-colors focus:ring-2 focus:ring-mapit-accent focus:outline-none flex items-center justify-center gap-1.5 font-medium"
            onClick={() =>
              dispatch({
                type: "SET_OVERLAY",
                overlay: { kind: "simulation", node_id: node.id },
              })
            }
          >
            <span>🎬</span>
            <span>Simulate execution from here</span>
          </button>
        </div>
      </div>
    </div>
  );
}
