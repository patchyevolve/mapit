
import { useState } from "react";
import { useAppState } from "../store";
import { api } from "../api-client";
import { SearchBar } from "./SearchBar";

export function TopBar() {
  const { state, dispatch } = useAppState();
  const [remapping, setRemapping] = useState(false);
  const [annotating, setAnnotating] = useState(false);

  return (
    <header className="flex items-center justify-between px-4 py-2 bg-mapit-surface border-b border-mapit-border">
      <div className="flex items-center gap-3">
        <div className="text-lg font-bold bg-gradient-to-r from-mapit-accent to-mapit-success bg-clip-text text-transparent">
          mapit
        </div>
        <div className="text-xs text-mapit-muted">
          {state.project?.project_root && (
            <span>Workspace: {state.project.project_root}</span>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2">
        <SearchBar />

        <button
          className="flex items-center gap-2 px-3 py-1.5 text-sm rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          onClick={() => dispatch({ type: "SET_OVERLAY", overlay: { kind: "ask" } })}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="M21 21l-4.35-4.35" />
          </svg>
          Ask AI
        </button>

        <button
          className="flex items-center gap-2 px-3 py-1.5 text-sm rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          onClick={async () => {
            try {
              const res = await api.flaws();
              dispatch({ type: "SET_FLAWS", flaws: res.flaws });
              dispatch({ type: "SET_SCREEN", screen: "flaws_report" });
            } catch (e) {
              console.error(e);
            }
          }}
        >
          <svg
            width="16"
            height="16"
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
          {state.flaws.length > 0 ? `Flaws (${state.flaws.length})` : "Flaws"}
        </button>

        <button
          className="flex items-center gap-2 px-3 py-1.5 text-sm rounded bg-mapit-surface2 border border-mapit-border text-mapit-text hover:bg-mapit-surface hover:border-mapit-accent/50 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none"
          onClick={() => dispatch({ type: "SET_SCREEN", screen: "settings" })}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v6m0 6v6M4.22 4.22l4.24 4.24m7.08 7.08l4.24 4.24M1 12h6m6 0h6M4.22 19.78l4.24-4.24m7.08-7.08l4.24-4.24" />
          </svg>
          Settings
        </button>

        <button
          disabled={annotating}
          className="flex items-center gap-2 px-3 py-1.5 text-sm rounded bg-mapit-accent text-white hover:opacity-90 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
          onClick={async () => {
            setAnnotating(true);
            try {
              await api.annotate();
              const project = await api.project();
              dispatch({ type: "SET_PROJECT", project });
              const flawsRes = await api.flaws();
              dispatch({ type: "SET_FLAWS", flaws: flawsRes.flaws });
            } catch (e) {
              console.error(e);
            } finally {
              setAnnotating(false);
            }
          }}
        >
          {annotating ? (
            <div className="w-4 h-4 border-2 border-white/50 border-t-white rounded-full animate-spin" />
          ) : (
            <svg
              width="16"
              height="16"
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
          {annotating ? "Annotating..." : "Annotate"}
        </button>

        <button
          disabled={remapping}
          className="flex items-center gap-2 px-3 py-1.5 text-sm rounded bg-mapit-success text-white hover:opacity-90 transition-all focus:ring-2 focus:ring-mapit-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
          onClick={async () => {
            setRemapping(true);
            try {
              await api.remap();
              const [project, nodesRes, edgesRes, featRes, flawRes] = await Promise.all([
                api.project(),
                api.nodes(),
                api.edges(),
                api.features(),
                api.flaws(),
              ]);
              const nodeMap = new Map(nodesRes.nodes.map((n) => [n.id, n]));
              dispatch({ type: "SET_PROJECT", project });
              dispatch({ type: "SET_ALL_NODES", nodes: nodeMap });
              dispatch({ type: "SET_EDGES", edges: edgesRes.edges });
              dispatch({ type: "SET_FEATURES", features: featRes.features });
              dispatch({ type: "SET_FLAWS", flaws: flawRes.flaws });
            } catch (e) {
              console.error(e);
            } finally {
              setRemapping(false);
            }
          }}
        >
          {remapping ? (
            <div className="w-4 h-4 border-2 border-white/50 border-t-white rounded-full animate-spin" />
          ) : (
            <svg
              width="16"
              height="16"
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
          {remapping ? "Remapping..." : "Re-Map"}
        </button>
      </div>
    </header>
  );
}
