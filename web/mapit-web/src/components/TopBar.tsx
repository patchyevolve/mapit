import { useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import { SearchBar } from "./SearchBar";

type Toast = { msg: string; ok: boolean };

export function TopBar() {
  const { state, dispatch } = useAppState();
  const [remapping, setRemapping] = useState(false);
  const [annotating, setAnnotating] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);

  const showToast = (msg: string, ok: boolean) => {
    setToast({ msg, ok });
    setTimeout(() => setToast(null), 4000);
  };

  const handleRemap = async () => {
    setRemapping(true);
    // Show progress bar immediately (indeterminate until first WS event)
    dispatch({
      type: "SET_BG_PROGRESS",
      progress: {
        phase: "structural",
        current: 0,
        total: 0,
        label: "Re-mapping codebase…",
      },
    });
    try {
      await api.remap();
      showToast("Re-map started", true);
    } catch (e) {
      dispatch({ type: "SET_BG_PROGRESS", progress: null });
      showToast(
        `Re-map failed: ${e instanceof Error ? e.message : "unknown error"}`,
        false,
      );
    } finally {
      setRemapping(false);
    }
  };

  const handleAnnotate = async () => {
    setAnnotating(true);
    // Show progress bar immediately (indeterminate until first WS event)
    dispatch({
      type: "SET_BG_PROGRESS",
      progress: {
        phase: "ai_enrichment",
        current: 0,
        total: state.project?.symbol_count ?? 0,
        label: "Annotating symbols…",
      },
    });
    try {
      await api.annotate();
      showToast("Annotation started — watch the progress bar", true);
    } catch (e) {
      dispatch({ type: "SET_BG_PROGRESS", progress: null });
      showToast(
        `Annotation failed: ${e instanceof Error ? e.message : "unknown error"}. Check Settings → API Connection.`,
        false,
      );
    } finally {
      setAnnotating(false);
    }
  };

  return (
    <header className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border relative">
      <div className="flex items-center gap-3">
        <div className="text-lg font-bold bg-gradient-to-r from-mapit-accent to-mapit-success bg-clip-text text-transparent select-none">
          mapit
        </div>
        {state.project?.project_root && (
          <div
            className="text-xs text-mapit-muted truncate max-w-xs"
            title={state.project.project_root}
          >
            {state.project.project_root.split("/").slice(-2).join("/")}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <SearchBar />

        {/* Ask AI */}
        <button
          type="button"
          className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded border transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none ${
            state.overlay?.kind === "ask"
              ? "bg-mapit-accent text-white border-mapit-accent"
              : "bg-mapit-surface2 border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50"
          }`}
          onClick={() =>
            dispatch({
              type: "SET_OVERLAY",
              overlay: state.overlay?.kind === "ask" ? null : { kind: "ask" },
            })
          }
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
          </svg>
          Ask AI
        </button>

        {/* Flaws */}
        <button
          type="button"
          className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded border transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none ${
            state.screen === "flaws_report"
              ? "bg-mapit-accent text-white border-mapit-accent"
              : "bg-mapit-surface2 border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50"
          }`}
          onClick={async () => {
            if (state.screen === "flaws_report") {
              dispatch({ type: "SET_SCREEN", screen: "system_overview" });
              return;
            }
            try {
              const res = await api.flaws();
              dispatch({ type: "SET_FLAWS", flaws: res.flaws });
              dispatch({ type: "SET_OVERLAY", overlay: null });
              dispatch({ type: "SET_SCREEN", screen: "flaws_report" });
            } catch (e) {
              showToast(
                `Could not load flaws: ${e instanceof Error ? e.message : "error"}`,
                false,
              );
            }
          }}
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
            <line x1="12" y1="9" x2="12" y2="13" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </svg>
          {state.flaws.length > 0 ? (
            <span>
              Flaws{" "}
              <span className="bg-mapit-danger text-white text-xs px-1.5 py-0.5 rounded-full ml-0.5">
                {state.flaws.length}
              </span>
            </span>
          ) : (
            "Flaws"
          )}
        </button>

        {/* Settings */}
        <button
          type="button"
          className={`flex items-center gap-1.5 px-3 py-1.5 text-sm rounded border transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none ${
            state.screen === "settings"
              ? "bg-mapit-accent text-white border-mapit-accent"
              : "bg-mapit-surface2 border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50"
          }`}
          onClick={() => {
            if (state.screen === "settings") {
              dispatch({ type: "SET_SCREEN", screen: "system_overview" });
            } else {
              dispatch({ type: "SET_OVERLAY", overlay: null });
              dispatch({ type: "SET_SCREEN", screen: "settings" });
            }
          }}
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          Settings
        </button>

        {/* Annotate */}
        <button
          type="button"
          disabled={annotating}
          className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
          onClick={handleAnnotate}
          title="Run AI enrichment (summaries + flaw detection)"
        >
          {annotating ? (
            <div className="w-3.5 h-3.5 border-2 border-mapit-accent/50 border-t-mapit-accent rounded-full animate-spin" />
          ) : (
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M12 20h9" />
              <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
            </svg>
          )}
          {annotating ? "Annotating…" : "Annotate"}
        </button>

        {/* Re-Map */}
        <button
          type="button"
          disabled={remapping}
          className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded bg-mapit-accent text-white hover:opacity-90 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
          onClick={handleRemap}
          title="Re-scan the project and rebuild the graph"
        >
          {remapping ? (
            <div className="w-3.5 h-3.5 border-2 border-white/50 border-t-white rounded-full animate-spin" />
          ) : (
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
              <path d="M21 3v5h-5" />
            </svg>
          )}
          {remapping ? "Remapping…" : "Re-Map"}
        </button>
      </div>

      {/* Toast notification */}
      {toast && (
        <div
          className={`absolute top-full left-1/2 -translate-x-1/2 mt-2 z-50 px-4 py-2 rounded-lg shadow-lg text-sm border max-w-sm text-center pointer-events-none ${
            toast.ok
              ? "bg-mapit-success/10 border-mapit-success/40 text-mapit-success"
              : "bg-mapit-danger/10 border-mapit-danger/40 text-mapit-danger"
          }`}
        >
          {toast.msg}
        </div>
      )}
    </header>
  );
}
