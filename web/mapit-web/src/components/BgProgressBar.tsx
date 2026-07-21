import { useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import type { ProgressState } from "../types";

// ─── Phase metadata ────────────────────────────────────────────────────────────

const PHASE_META: Record<
  ProgressState["phase"],
  { icon: string; color: string; trackColor: string }
> = {
  structural: {
    icon: "🔍",
    color:      "bg-mapit-accent",
    trackColor: "bg-mapit-accent/20",
  },
  ai_enrichment: {
    icon: "✨",
    color:      "bg-mapit-success",
    trackColor: "bg-mapit-success/20",
  },
};

// ─── Component ─────────────────────────────────────────────────────────────────

export function BgProgressBar() {
  const { state, dispatch } = useAppState();
  const [minimised, setMinimised] = useState(false);
  const bg = state.bgProgress;

  if (!bg) return null;

  const pct = bg.total > 0 ? Math.min(100, Math.round((bg.current / bg.total) * 100)) : 0;
  const meta = PHASE_META[bg.phase] ?? PHASE_META.structural;
  const isDone = bg.current >= bg.total && bg.total > 0;
  const isIndeterminate = bg.total === 0;

  // Minimal pill — shown when minimised
  if (minimised) {
    return (
      <button
        type="button"
        onClick={() => setMinimised(false)}
        className="fixed bottom-4 right-4 z-50 flex items-center gap-2 px-3 py-1.5 rounded-full bg-mapit-surface border border-mapit-border shadow-xl hover:border-mapit-accent/50 transition-all"
        title="Show progress"
      >
        <span className="text-sm">{meta.icon}</span>
        {!isIndeterminate && (
          <span className="text-xs font-mono text-mapit-text">{pct}%</span>
        )}
        <span
          className={`w-2 h-2 rounded-full ${
            isDone
              ? "bg-mapit-success"
              : `${meta.color} animate-pulse`
          }`}
        />
      </button>
    );
  }

  return (
    <div className="fixed bottom-4 right-4 z-50 w-80 bg-mapit-surface border border-mapit-border rounded-xl shadow-2xl overflow-hidden">
      {/* Header bar */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-mapit-border">
        <div className="flex items-center gap-2">
          <span className="text-base leading-none">{meta.icon}</span>
          <span className="text-sm font-semibold text-mapit-text truncate">
            {bg.label}
          </span>
          {isDone && (
            <span className="text-xs text-mapit-success font-medium">✓ Done</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => setMinimised(true)}
            className="text-mapit-muted hover:text-mapit-text transition-colors p-1 rounded focus:outline-none focus:ring-1 focus:ring-mapit-accent"
            title="Minimise"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
          </button>
          {isDone && (
            <button
              type="button"
              onClick={() => dispatch({ type: "SET_BG_PROGRESS", progress: null })}
              className="text-mapit-muted hover:text-mapit-text transition-colors p-1 rounded focus:outline-none focus:ring-1 focus:ring-mapit-accent"
              title="Dismiss"
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          )}
        </div>
      </div>

      {/* Progress bar */}
      <div className="px-4 pt-3 pb-1">
        <div className={`w-full h-2 rounded-full overflow-hidden ${meta.trackColor}`}>
          {isIndeterminate ? (
            /* Indeterminate — animated shimmer */
            <div className={`h-full w-1/3 ${meta.color} rounded-full animate-indeterminate`} />
          ) : (
            <div
              className={`h-full ${meta.color} rounded-full transition-all duration-500 ease-out`}
              style={{ width: `${pct}%` }}
            />
          )}
        </div>
      </div>

      {/* Stats row */}
      <div className="flex items-center justify-between px-4 py-2">
        <span className="text-xs text-mapit-muted truncate max-w-[200px]" title={bg.currentFile}>
          {bg.currentSymbol || (bg.currentFile
            ? bg.currentFile.split("/").slice(-1)[0]
            : isDone
              ? "Complete"
              : bg.phase === "structural"
                ? "Parsing source files…"
                : "Summarising symbols…")}
        </span>
        {!isIndeterminate && (
          <div className="flex items-center gap-2 flex-shrink-0 ml-2">
            <span className="text-xs text-mapit-muted font-mono">
              {bg.current.toLocaleString()}&thinsp;/&thinsp;{bg.total.toLocaleString()}
            </span>
            <span className="text-xs font-mono font-semibold text-mapit-text">
              {pct}%
            </span>
          </div>
        )}
      </div>
      {/* File name when showing symbol */}
      {bg.currentSymbol && bg.currentFile && (
        <div className="px-4 pb-1 -mt-1">
          <span className="text-[11px] text-mapit-muted/60 truncate block" title={bg.currentFile}>
            {bg.currentFile.split("/").slice(-1)[0]}
          </span>
        </div>
      )}

      {/* Phase sub-label + cancel */}
      <div className="flex items-center justify-between px-4 pb-3">
        <span className={`text-xs font-medium ${
          bg.phase === "ai_enrichment" ? "text-mapit-success" : "text-mapit-accent"
        }`}>
          {bg.phase === "structural" ? "Structural mapping" : "AI enrichment"}
        </span>
        {!isDone && bg.phase === "ai_enrichment" && (
          <button
            type="button"
            onClick={async () => {
              try {
                await api.cancelAnnotate();
              } catch (e) {
                console.error("Cancel annotation failed:", e);
              }
            }}
            className="text-xs font-medium text-mapit-danger hover:text-mapit-danger/80 transition-colors focus:outline-none"
          >
            Stop
          </button>
        )}
      </div>
    </div>
  );
}
