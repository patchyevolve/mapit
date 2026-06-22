import { createContext, useContext } from "react";
import type { AppState } from "./types";

export const initialState: AppState = {
  screen: "connecting",
  overlay: null,
  breadcrumb: [],
  project: null,
  allNodes: new Map(),
  allEdges: [],
  features: [],
  flaws: [],
  searchResults: [],
  wsConnected: false,
  mapProgress: null,
};

export type AppAction =
  | { type: "SET_SCREEN"; screen: AppState["screen"] }
  | { type: "SET_OVERLAY"; overlay: AppState["overlay"] }
  | { type: "SET_BREADCRUMB"; breadcrumb: AppState["breadcrumb"] }
  | { type: "SET_PROJECT"; project: AppState["project"] }
  | { type: "SET_ALL_NODES"; nodes: Map<string, import("./types").Node> }
  | { type: "UPSERT_NODE"; node: import("./types").Node }
  | { type: "SET_EDGES"; edges: import("./types").Edge[] }
  | { type: "SET_FEATURES"; features: import("./types").FeatureNode[] }
  | { type: "SET_FLAWS"; flaws: import("./types").FlawEntry[] }
  | { type: "SET_SEARCH"; results: import("./types").SearchResult[] }
  | { type: "SET_WS_CONNECTED"; connected: boolean }
  | { type: "SET_MAP_PROGRESS"; progress: AppState["mapProgress"] };

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "SET_SCREEN":
      return { ...state, screen: action.screen };
    case "SET_OVERLAY":
      return { ...state, overlay: action.overlay };
    case "SET_BREADCRUMB":
      return { ...state, breadcrumb: action.breadcrumb };
    case "SET_PROJECT":
      return { ...state, project: action.project };
    case "SET_ALL_NODES":
      return { ...state, allNodes: action.nodes };
    case "UPSERT_NODE": {
      const next = new Map(state.allNodes);
      next.set(action.node.id, action.node);
      return { ...state, allNodes: next };
    }
    case "SET_EDGES":
      return { ...state, allEdges: action.edges };
    case "SET_FEATURES":
      return { ...state, features: action.features };
    case "SET_FLAWS":
      return { ...state, flaws: action.flaws };
    case "SET_SEARCH":
      return { ...state, searchResults: action.results };
    case "SET_WS_CONNECTED":
      return { ...state, wsConnected: action.connected };
    case "SET_MAP_PROGRESS":
      return { ...state, mapProgress: action.progress };
    default:
      return state;
  }
}

export const AppCtx = createContext<{
  state: AppState;
  dispatch: React.Dispatch<AppAction>;
} | null>(null);

export function useAppState() {
  const ctx = useContext(AppCtx);
  if (!ctx) throw new Error("useAppState must be used inside AppProvider");
  return ctx;
}
