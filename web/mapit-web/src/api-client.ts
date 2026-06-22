import type {
  ProjectInfo,
  Node,
  NeighborsResponse,
  TraceResponse,
  FeaturesResponse,
  FlawsResponse,
  SearchResponse,
  AskRequest,
  AskResponse,
  AppConfig,
  ConfigUpdate,
  RemapResponse,
  AnnotateResponse,
} from "./types";

const BASE = `${window.location.protocol}//${window.location.host}/api`;

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error || `HTTP ${res.status}`);
  }
  return res.json();
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const b = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(b.error || `HTTP ${res.status}`);
  }
  return res.json();
}

async function put<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const b = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(b.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export const api = {
  project: () => get<ProjectInfo>("/project"),

  node: (id: string) => get<Node>(`/graph/node/${encodeURIComponent(id)}`),

  neighbors: (
    id: string,
    direction: "callers" | "callees" | "both" = "both",
    depth = 1,
  ) =>
    get<NeighborsResponse>(
      `/graph/neighbors/${encodeURIComponent(id)}?direction=${direction}&depth=${depth}`,
    ),

  trace: (id: string, maxDepth = 6) =>
    get<TraceResponse>(
      `/graph/trace/${encodeURIComponent(id)}?max_depth=${maxDepth}`,
    ),

  nodes: (type?: string) => {
    const qs = type ? `?type=${encodeURIComponent(type)}` : "";
    return get<{ nodes: Node[] }>(`/graph/nodes${qs}`);
  },

  edges: () => get<{ edges: import("./types").Edge[] }>("/graph/edges"),

  features: () => get<FeaturesResponse>("/graph/features"),

  flaws: (severity?: "high" | "warning" | "info") => {
    const qs = severity ? `?severity=${severity}` : "";
    return get<FlawsResponse>(`/graph/flaws${qs}`);
  },

  search: (q: string, limit = 20) =>
    get<SearchResponse>(
      `/graph/search?q=${encodeURIComponent(q)}&limit=${limit}`,
    ),

  ask: (req: AskRequest) => post<AskResponse>("/ask", req),

  config: () => get<AppConfig>("/config"),

  updateConfig: (cfg: ConfigUpdate) => put<AppConfig>("/config", cfg),

  remap: (force = false) => post<RemapResponse>("/remap", { force }),

  annotate: (all = false, force = false) =>
    post<AnnotateResponse>("/annotate", { all, force }),

  source: (file: string, start?: number, end?: number) => {
    const params = new URLSearchParams({ file });
    if (start != null) params.set("start", String(start));
    if (end != null) params.set("end", String(end));
    return get<{
      content: string;
      language: string;
      start_line: number;
      end_line: number;
    }>(`/source?${params.toString()}`);
  },

  testConnection: () =>
    post<{
      ok: boolean;
      message: string;
      latency_ms?: number;
      models: string[];
    }>("/config/test-connection", {}),

  testChat: (message: string) =>
    post<{
      ok: boolean;
      response?: string;
      error?: string;
      latency_ms?: number;
    }>("/config/test-chat", { message }),
};
