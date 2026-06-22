import { useAppState } from "../store";
import type { FlawEntry } from "../types";

export function FlawsReport() {
  const { state, dispatch } = useAppState();
  const flaws = state.flaws;

  const severityColor = (s: string) => {
    switch (s) {
      case "high": return "text-mapit-danger";
      case "warning": return "text-mapit-warning";
      default: return "text-mapit-muted";
    }
  };

  const bgColor = (s: string) => {
    switch (s) {
      case "high": return "bg-red-900/20 border-red-800";
      case "warning": return "bg-yellow-900/20 border-yellow-800";
      default: return "bg-gray-800/20 border-gray-700";
    }
  };

  return (
    <div className="flex flex-col h-full bg-mapit-bg">
      <div className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
        <h2 className="text-sm font-semibold text-mapit-text">Flaws & Issues</h2>
        <button
          className="text-mapit-muted hover:text-mapit-text"
          onClick={() => dispatch({ type: "SET_SCREEN", screen: "system_overview" })}
        >
          Back
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        <div className="bg-yellow-900/20 border border-yellow-800 rounded px-3 py-2 mb-4 text-xs text-yellow-200">
          ⚠ Flaws are AI-assisted heuristics, not guaranteed facts.
        </div>

        <div className="space-y-2">
          {flaws.length === 0 && (
            <p className="text-mapit-muted text-sm">No flaws detected.</p>
          )}
          {flaws.map((f: FlawEntry) => (
            <div
              key={f.id}
              className={`border rounded-lg px-3 py-2 ${bgColor(f.severity)}`}
            >
              <div className="flex items-center gap-2">
                <span className={`text-xs font-bold uppercase ${severityColor(f.severity)}`}>
                  {f.severity}
                </span>
                <span className="text-xs text-mapit-muted">{f.kind}</span>
                <span className="text-xs text-mapit-muted ml-auto">
                  {(f.confidence * 100).toFixed(0)}% · {f.basis}
                </span>
              </div>
              <p className="text-sm text-mapit-text mt-1">{f.description}</p>
              <p className="text-xs text-mapit-muted mt-1">
                {f.file_path} — {f.primary_node_name}
              </p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
