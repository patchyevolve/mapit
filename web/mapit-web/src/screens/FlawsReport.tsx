import { useState, useMemo } from "react";
import { useAppState } from "../store";
import type { FlawEntry, FlawSeverity, FlawKind } from "../types";

const SEVERITY_ORDER: Record<FlawSeverity, number> = {
  high: 0,
  warning: 1,
  info: 2,
};

export function FlawsReport() {
  const { state, dispatch } = useAppState();
  const [filterSeverity, setFilterSeverity] = useState<FlawSeverity | "all">(
    "all",
  );
  const [filterKind, setFilterKind] = useState<FlawKind | "all">("all");
  const [sortBy, setSortBy] = useState<"severity" | "confidence">("severity");

  const allKinds = useMemo(() => {
    const kinds = new Set(state.flaws.map((f) => f.kind));
    return Array.from(kinds) as FlawKind[];
  }, [state.flaws]);

  const displayed = useMemo(() => {
    let list = [...state.flaws];
    if (filterSeverity !== "all")
      list = list.filter((f) => f.severity === filterSeverity);
    if (filterKind !== "all") list = list.filter((f) => f.kind === filterKind);
    if (sortBy === "severity")
      list.sort(
        (a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity],
      );
    if (sortBy === "confidence")
      list.sort((a, b) => b.confidence - a.confidence);
    return list;
  }, [state.flaws, filterSeverity, filterKind, sortBy]);

  const severityColor = (s: FlawSeverity) => {
    switch (s) {
      case "high":
        return "text-mapit-danger";
      case "warning":
        return "text-mapit-warning";
      default:
        return "text-mapit-muted";
    }
  };

  const bgColor = (s: FlawSeverity) => {
    switch (s) {
      case "high":
        return "bg-mapit-danger/10 border-mapit-danger/30";
      case "warning":
        return "bg-mapit-warning/10 border-mapit-warning/30";
      default:
        return "bg-mapit-surface2 border-mapit-border";
    }
  };

  const navigateToFlaw = (f: FlawEntry) => {
    dispatch({ type: "SET_OVERLAY", overlay: null });
    const node = state.allNodes.get(f.primary_node_id);
    if (
      node?.type === "function" ||
      node?.type === "type" ||
      node?.type === "macro" ||
      node?.type === "global"
    ) {
      dispatch({
        type: "SET_OVERLAY",
        overlay: { kind: "function_detail", node_id: f.primary_node_id },
      });
      dispatch({ type: "SET_SCREEN", screen: "system_overview" });
    } else if (node?.type === "file" || node?.type === "feature") {
      dispatch({
        type: "SET_BREADCRUMB",
        breadcrumb: [{ label: node.name, node_id: node.id }],
      });
      dispatch({
        type: "SET_SCREEN",
        screen: node.type === "feature" ? "expanded_feature" : "expanded_file",
      });
    } else {
      dispatch({ type: "SET_SCREEN", screen: "system_overview" });
    }
  };

  const filterBtnClass = (active: boolean) =>
    `px-2 py-0.5 text-xs rounded border transition-colors focus:ring-1 focus:ring-mapit-accent focus:outline-none ${
      active
        ? "bg-mapit-accent text-white border-mapit-accent"
        : "bg-mapit-surface2 text-mapit-muted border-mapit-border hover:border-mapit-accent/50"
    }`;

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <h2 className="text-sm font-semibold text-mapit-text">
          Flaws &amp; Issues
          <span className="text-mapit-muted font-normal ml-1">
            ({state.flaws.length})
          </span>
          {displayed.length !== state.flaws.length && (
            <span className="text-mapit-muted font-normal ml-1">
              / showing {displayed.length}
            </span>
          )}
        </h2>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              const text = state.flaws
                .map(
                  (f) =>
                    `[${f.severity.toUpperCase()}] ${f.kind.replace(/_/g, " ")} — ${f.description}\n    File: ${f.file_path} — ${f.primary_node_name} (${(f.confidence * 100).toFixed(0)}% confidence, ${f.basis})`,
                )
                .join("\n\n");
              navigator.clipboard.writeText(text).catch(console.error);
            }}
            className="flex items-center gap-1 text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded px-2 py-1 text-xs transition-colors"
            title="Copy all flaws as text"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
            Copy
          </button>
          <button
            type="button"
            className="text-mapit-muted hover:text-mapit-text focus:ring-2 focus:ring-mapit-accent focus:outline-none rounded px-2 py-1 text-xs"
            onClick={() =>
              dispatch({ type: "SET_SCREEN", screen: "system_overview" })
            }
          >
            ← Back to graph
          </button>
        </div>
      </div>

      {/* Filters */}
      <div className="px-4 py-2 bg-mapit-surface border-b border-mapit-border flex flex-wrap items-center gap-2 text-xs">
        <span className="text-mapit-muted">Severity:</span>
        {(["all", "high", "warning", "info"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={filterBtnClass(filterSeverity === s)}
            onClick={() => setFilterSeverity(s)}
          >
            {s === "all" ? "All" : s}
          </button>
        ))}

        {allKinds.length > 0 && (
          <>
            <span className="text-mapit-muted ml-2">Kind:</span>
            <button
              type="button"
              className={filterBtnClass(filterKind === "all")}
              onClick={() => setFilterKind("all")}
            >
              All
            </button>
            {allKinds.map((k) => (
              <button
                key={k}
                type="button"
                className={filterBtnClass(filterKind === k)}
                onClick={() => setFilterKind(k)}
              >
                {k.replace(/_/g, " ")}
              </button>
            ))}
          </>
        )}

        <span className="text-mapit-muted ml-2">Sort:</span>
        {(["severity", "confidence"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={filterBtnClass(sortBy === s)}
            onClick={() => setSortBy(s)}
          >
            {s}
          </button>
        ))}
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto p-4 min-h-0">
        <div className="bg-mapit-warning/10 border border-mapit-warning/30 rounded px-3 py-2 mb-4 text-xs text-mapit-text">
          ⚠ Flaws are AI-assisted heuristics, not guaranteed facts. Always
          verify before acting on them.
        </div>

        <div className="space-y-2">
          {displayed.length === 0 && (
            <p className="text-mapit-muted text-sm">
              {state.flaws.length === 0
                ? "No flaws detected."
                : "No flaws match the current filters."}
            </p>
          )}
          {displayed.map((f: FlawEntry) => (
            <button
              key={f.id}
              type="button"
              onClick={() => navigateToFlaw(f)}
              className={`w-full text-left border rounded-lg px-3 py-2 transition-colors hover:border-mapit-accent/50 focus:ring-2 focus:ring-mapit-accent focus:outline-none ${bgColor(f.severity)}`}
            >
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`text-xs font-bold uppercase ${severityColor(f.severity)}`}
                >
                  {f.severity}
                </span>
                <span className="text-xs text-mapit-muted">
                  {f.kind.replace(/_/g, " ")}
                </span>
                <span className="text-xs text-mapit-muted ml-auto">
                  {(f.confidence * 100).toFixed(0)}% · {f.basis}
                </span>
              </div>
              <p className="text-sm text-mapit-text mt-1">{f.description}</p>
              <p className="text-xs text-mapit-muted mt-1 font-mono truncate">
                {f.file_path} — {f.primary_node_name}
              </p>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
