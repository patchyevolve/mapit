import { useEffect, useReducer, useState } from "react";
import { AppCtx, appReducer, initialState } from "./store";
import { connectWs, onWsEvent } from "./ws-client";
import { api } from "./api-client";
import { TopBar } from "./components/TopBar";
import { Breadcrumb } from "./components/Breadcrumb";
import { BgProgressBar } from "./components/BgProgressBar";
import { LoadingScreen } from "./screens/LoadingScreen";
import { MapProgressScreen } from "./screens/MapProgressScreen";
import { SystemOverview } from "./screens/SystemOverview";
import { NeighborsView } from "./screens/NeighborsView";
import { FlawsReport } from "./screens/FlawsReport";
import { SettingsPanel } from "./screens/SettingsPanel";
import { FunctionDetailPanel } from "./screens/FunctionDetailPanel";
import { ExternalDetailPanel } from "./screens/ExternalDetailPanel";
import { TraceView } from "./screens/TraceView";
import { AskPanel } from "./screens/AskPanel";
import { SimulationView } from "./screens/SimulationView";

export default function App() {
  const [state, dispatch] = useReducer(appReducer, initialState);
  const [errorToast, setErrorToast] = useState<string | null>(null);

  /* ─── Load initial data ─── */
  useEffect(() => {
    connectWs();

    api
      .project()
      .then((project) => {
        dispatch({ type: "SET_PROJECT", project });

        if (project.symbol_count === 0) {
          dispatch({ type: "SET_SCREEN", screen: "map_progress" });
          return;
        }

        // Phase 1 — file nodes + features + flaws → show UI immediately
        Promise.all([api.nodes("file"), api.features(), api.flaws()])
          .then(([filesRes, featRes, flawRes]) => {
            const fileMap = new Map(filesRes.nodes.map((n) => [n.id, n]));
            dispatch({ type: "SET_ALL_NODES", nodes: fileMap });
            dispatch({ type: "SET_FEATURES", features: featRes.features });
            dispatch({ type: "SET_FLAWS", flaws: flawRes.flaws });
            dispatch({ type: "SET_SCREEN", screen: "system_overview" });

            // Phase 2 — full node set + edges in background
            Promise.all([api.nodes(), api.edges()])
              .then(([nodesRes, edgesRes]) => {
                const fullMap = new Map(nodesRes.nodes.map((n) => [n.id, n]));
                dispatch({ type: "SET_ALL_NODES", nodes: fullMap });
                dispatch({ type: "SET_EDGES", edges: edgesRes.edges });
              })
              .catch((err) => {
                console.error(err);
                setErrorToast("Failed to load graph data — check the server connection.");
              });
          })
          .catch((err) => {
            console.error(err);
            setErrorToast("Failed to load project data — check the server connection.");
          });
      })
      .catch((err) => {
        console.warn("No project data yet:", err.message);
        dispatch({ type: "SET_SCREEN", screen: "map_progress" });
      });

    // ─── WebSocket event handler ───────────────────────────────────────────────
    const unsub = onWsEvent((event) => {
      // We read state inline via a ref trick — but since this closure captures
      // the initial state, we use the dispatch pattern only. The screen check
      // uses a local variable updated each render via the state ref pattern below.
      switch (event.event) {
        case "map_progress": {
          const prog = {
            phase: event.phase,
            current: event.current,
            total: event.total,
            currentFile: event.current_file,
            currentSymbol: event.current_symbol,
          };

          // We dispatch both — the reducer decides which matters based on current screen.
          // We use two separate action types to route correctly in the reducer.
          dispatch({ type: "SET_MAP_PROGRESS", progress: prog });

          dispatch({
            type: "TICK_BG_PROGRESS",
            current: event.current,
            total: event.total,
            currentFile: event.current_file,
            currentSymbol: event.current_symbol,
          });
          break;
        }

        case "map_phase_complete": {
          const done = event.total ?? 0;

          if (event.phase === "structural") {
            // Clear full-screen progress (only relevant during initial load)
            dispatch({ type: "SET_MAP_PROGRESS", progress: null });
            dispatch({ type: "SET_SCREEN", screen: "system_overview" });

            // Mark bg progress as complete (will auto-dismiss or let user close)
            dispatch({
              type: "TICK_BG_PROGRESS",
              current: done,
              total: done,
              currentFile: undefined,
            });
          } else if (event.phase === "ai_enrichment") {
            dispatch({ type: "SET_MAP_PROGRESS", progress: null });

            // Mark complete, then refresh project stats + flaws after a brief delay
            dispatch({
              type: "TICK_BG_PROGRESS",
              current: done,
              total: done,
              currentFile: undefined,
            });
            setTimeout(() => {
              Promise.all([api.project(), api.flaws()])
                .then(([project, flawsRes]) => {
                  dispatch({ type: "SET_PROJECT", project });
                  dispatch({ type: "SET_FLAWS", flaws: flawsRes.flaws });
                })
                .catch((err) => {
                  console.error(err);
                  setErrorToast("Failed to refresh after annotation.");
                });
            }, 800);
          }
          break;
        }

        case "node_updated":
          api.node(event.node_id).then((node) => {
            dispatch({ type: "UPSERT_NODE", node });
          }).catch((err) => {
            console.error(err);
            setErrorToast("Failed to update node from server.");
          });
          break;

        case "error": {
          console.error("[ws error]", event.message);
          const errMsg = `⚠ ${event.message}`;
          setErrorToast(errMsg);
          setTimeout(() => setErrorToast(null), 8000);
          // Clear background progress so user sees the error
          dispatch({ type: "SET_BG_PROGRESS", progress: null });
          break;
        }
      }
    });

    return () => unsub();
  }, []);

  // ─── Show/hide the shell ────────────────────────────────────────────────────
  const showShell =
    state.screen !== "connecting" && state.screen !== "map_progress";

  // ─── Whether bgProgress should be visible ──────────────────────────────────
  const showBgProgress = showShell && state.bgProgress !== null;

  const renderMain = () => {
    switch (state.screen) {
      case "connecting":
        return <LoadingScreen />;
      case "map_progress":
        return <MapProgressScreen />;
      case "system_overview":
      case "expanded_feature":
      case "expanded_file":
        return <SystemOverview />;
      case "flaws_report":
        return <FlawsReport />;
      case "settings":
        return <SettingsPanel />;
    }
  };

  return (
    <AppCtx.Provider value={{ state, dispatch }}>
      <div className="h-screen flex flex-col bg-mapit-bg relative overflow-hidden">
        {showShell && <TopBar />}
        {showShell && state.breadcrumb.length > 0 && <Breadcrumb />}

        <main className="flex-1 min-h-0 relative">{renderMain()}</main>

        {state.overlay?.kind === "function_detail" && (
          <div className="absolute top-0 right-0 h-full z-30 pointer-events-auto">
            <FunctionDetailPanel />
          </div>
        )}

        {state.overlay?.kind === "external_detail" && (
          <div className="absolute top-0 right-0 h-full z-30 pointer-events-auto">
            <ExternalDetailPanel />
          </div>
        )}

        {state.overlay?.kind === "trace_view" && (
          <div className="absolute inset-0 z-40 bg-mapit-bg/95 pointer-events-auto">
            <TraceView />
          </div>
        )}

        {state.overlay?.kind === "neighbors" && (
          <div className="absolute inset-0 z-40 bg-mapit-bg/95 pointer-events-auto">
            <NeighborsView />
          </div>
        )}

        {state.overlay?.kind === "ask" && (
          <div className="absolute bottom-0 left-0 right-0 z-30 pointer-events-auto">
            <AskPanel />
          </div>
        )}

        {(state.overlay?.kind === "simulation"
          || state.overlay?.kind === "file_simulation"
          || state.overlay?.kind === "module_simulation"
          || state.overlay?.kind === "feature_simulation"
          || state.overlay?.kind === "project_simulation"
        ) && (
          <div className="absolute inset-0 z-40 bg-mapit-bg pointer-events-auto">
            <SimulationView />
          </div>
        )}

        {showBgProgress && <BgProgressBar />}

        {errorToast && (
          <div className="fixed bottom-20 right-4 z-50 max-w-md bg-mapit-danger/10 border border-mapit-danger/40 text-mapit-danger px-4 py-3 rounded-xl shadow-2xl text-sm">
            <div className="flex items-start gap-2">
              <span className="mt-0.5 shrink-0">⚠️</span>
              <span className="break-words">{errorToast}</span>
              <button
                type="button"
                onClick={() => setErrorToast(null)}
                className="ml-2 shrink-0 text-mapit-danger/60 hover:text-mapit-danger transition-colors"
              >
                ✕
              </button>
            </div>
          </div>
        )}
      </div>
    </AppCtx.Provider>
  );
}
