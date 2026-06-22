import { useEffect, useReducer } from "react";
import { AppCtx, appReducer, initialState } from "./store";
import { connectWs, onWsEvent } from "./ws-client";
import { api } from "./api-client";
import { LoadingScreen } from "./screens/LoadingScreen";
import { MapProgressScreen } from "./screens/MapProgressScreen";
import { SystemOverview } from "./screens/SystemOverview";
import { NeighborsView } from "./screens/NeighborsView";
import { FlawsReport } from "./screens/FlawsReport";
import { SettingsPanel } from "./screens/SettingsPanel";
import { FunctionDetailPanel } from "./screens/FunctionDetailPanel";
import { TraceView } from "./screens/TraceView";
import { AskPanel } from "./screens/AskPanel";

export default function App() {
  const [state, dispatch] = useReducer(appReducer, initialState);

  /* ───── Load initial data ───── */
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

        // Load nodes, edges, features, flaws in parallel
        Promise.all([
          api.nodes(),
          api.edges(),
          api.features(),
          api.flaws(),
        ])
          .then(([nodesRes, edgesRes, featRes, flawRes]) => {
            const nodeMap = new Map(nodesRes.nodes.map((n) => [n.id, n]));
            dispatch({ type: "SET_ALL_NODES", nodes: nodeMap });
            dispatch({ type: "SET_EDGES", edges: edgesRes.edges });
            dispatch({ type: "SET_FEATURES", features: featRes.features });
            dispatch({ type: "SET_FLAWS", flaws: flawRes.flaws });
            dispatch({ type: "SET_SCREEN", screen: "system_overview" });
          })
          .catch(console.error);
      })
      .catch((err) => {
        console.warn("No project data yet, showing progress:", err.message);
        dispatch({ type: "SET_SCREEN", screen: "map_progress" });
      });

    /* ───── Websocket events ───── */
    const unsub = onWsEvent((event) => {
      switch (event.event) {
        case "map_progress":
          dispatch({
            type: "SET_MAP_PROGRESS",
            progress: {
              phase: event.phase,
              current: event.current,
              total: event.total,
              currentFile: event.current_file,
            },
          });
          break;
        case "map_phase_complete":
          if (event.phase === "structural") {
            dispatch({ type: "SET_MAP_PROGRESS", progress: null });
            dispatch({ type: "SET_SCREEN", screen: "system_overview" });
          }
          break;
        case "node_updated":
          // Fetch the updated node
          api.node(event.node_id).then((node) => {
            dispatch({ type: "UPSERT_NODE", node });
          });
          break;
        case "error":
          console.error("[ws error]", event.message);
          break;
      }
    });

    return () => unsub();
  }, []);

  /* ───── Render current screen ───── */
  const renderScreen = () => {
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
      <div className="h-screen flex flex-col bg-mapit-bg">
        {renderScreen()}

        {/* Overlays */}
        {state.overlay?.kind === "function_detail" && (
          <div className="absolute top-0 right-0 h-full z-10">
            <FunctionDetailPanel />
          </div>
        )}

        {state.overlay?.kind === "trace_view" && (
          <div className="absolute inset-0 z-20 bg-mapit-bg/95">
            <TraceView />
          </div>
        )}

        {state.overlay?.kind === "neighbors" && (
          <div className="absolute inset-0 z-20 bg-mapit-bg/95">
            <NeighborsView />
          </div>
        )}

        {state.overlay?.kind === "ask" && (
          <div className="absolute bottom-0 left-0 right-0 z-10">
            <AskPanel />
          </div>
        )}
      </div>
    </AppCtx.Provider>
  );
}
