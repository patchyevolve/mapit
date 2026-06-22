/* ──────────────────────────────────────────────
   types.ts — exact mirror of 03-graph-data-model.md
   ────────────────────────────────────────────── */

export type NodeType =
  | "feature"
  | "module"
  | "file"
  | "function"
  | "type"
  | "macro"
  | "global"
  | "external";

export interface Span {
  start_line: number;
  end_line: number;
}

export type AiSummaryStatus = "pending" | "ready" | "unavailable";

export interface BaseNode {
  id: string;
  name: string;
  language?: string;
  file_path?: string;
  span?: Span;
  ai_summary?: string | null;
  ai_summary_status: AiSummaryStatus;
  ai_model_used?: string | null;
  structural_hash: string;
  flaws: FlawFlag[];
}

export interface FeatureNode extends BaseNode {
  type: "feature";
  member_node_ids: string[];
  classification_confidence: number;
}

export interface FileNode extends BaseNode {
  type: "file";
  size_bytes: number;
  parse_status: "ok" | "parse_failed" | "unsupported_language";
  parse_error?: string;
}

export interface FunctionNode extends BaseNode {
  type: "function";
  signature: string;
  is_entry_point_candidate: boolean;
  has_incoming_calls: boolean;
  control_flow?: ControlFlowGraph;
}

export interface ExternalNode extends BaseNode {
  type: "external";
  reason: "no_source_present" | "dynamic_dispatch" | "unrecognized_binding";
}

export interface ModuleNode extends BaseNode {
  type: "module";
}

export interface TypeNode extends BaseNode {
  type: "type";
}

export interface MacroNode extends BaseNode {
  type: "macro";
}

export interface GlobalNode extends BaseNode {
  type: "global";
}

export type Node =
  | FeatureNode
  | FileNode
  | FunctionNode
  | ExternalNode
  | ModuleNode
  | TypeNode
  | MacroNode
  | GlobalNode;

export type EdgeType =
  | "calls"
  | "includes"
  | "defines"
  | "references"
  | "links_into"
  | "member_of";

export type EdgeConfidence = "exact" | "probable" | "dynamic_unresolved";

export interface Edge {
  id: string;
  from_id: string;
  to_id: string;
  type: EdgeType;
  confidence: EdgeConfidence;
  order_hint?: number;
  condition?: string | null;
}

export type FlawKind =
  | "dead_code"
  | "circular_dependency"
  | "structural_smell"
  | "suspected_bug"
  | "missing_error_handling"
  | "resource_leak_pattern";

export type FlawSeverity = "info" | "warning" | "high";

export type FlawBasis = "structural" | "ai" | "structural+ai";

export interface FlawFlag {
  id: string;
  kind: FlawKind;
  severity: FlawSeverity;
  description: string;
  confidence: number;
  basis: FlawBasis;
  related_node_ids?: string[];
}

export interface ControlFlowBlock {
  id: string;
  kind: "sequential" | "branch" | "loop" | "return";
  calls_in_block: { edge_id: string; order_hint: number }[];
  next_blocks: { block_id: string; condition?: string | null }[];
}

export interface ControlFlowGraph {
  entry_block_id: string;
  blocks: ControlFlowBlock[];
}

/* ──────────────────────────────────────────────
   API-specific types (05-backend-schema.md §6)
   ────────────────────────────────────────────── */

export interface ProjectInfo {
  project_root: string;
  last_full_map_at: string;
  last_incremental_map_at: string;
  file_count: number;
  symbol_count: number;
  edge_count: number;
  languages: string[];
  provider: string;
  model: string;
  ai_annotation_coverage_pct: number;
}

export interface NeighborsResponse {
  center_id: string;
  nodes: Node[];
  edges: Edge[];
}

export interface TraceStep {
  block_id: string;
  label?: string;
  calls: { node: FunctionNode; order_hint: number }[];
  branches: { condition?: string | null; next_block_id: string }[];
}

export interface TraceResponse {
  entry_node_id: string;
  steps: TraceStep[];
  truncated_at_depth: boolean;
}

export interface FeaturesResponse {
  features: FeatureNode[];
}

export interface FlawEntry {
  id: string;
  kind: FlawKind;
  severity: FlawSeverity;
  description: string;
  confidence: number;
  basis: FlawBasis;
  related_node_ids?: string[];
  primary_node_id: string;
  primary_node_name: string;
  file_path: string;
}

export interface FlawsResponse {
  flaws: FlawEntry[];
}

export interface SearchResult {
  node: Node;
  match_reason: "name" | "file_path" | "ai_summary";
}

export interface SearchResponse {
  results: SearchResult[];
}

export interface AskRequest {
  question: string;
}

export type GroundingStatus = "ok" | "partial" | "no_relevant_context_found";

export interface AskResponse {
  answer: string;
  referenced_node_ids: string[];
  grounding_status: GroundingStatus;
}

export interface AppConfig {
  provider: string;
  model: string;
  base_url: string;
  api_key_set: boolean;
  ignore_patterns: string[];
}

export interface ConfigUpdate {
  provider?: string;
  model?: string;
  base_url?: string;
  api_key?: string;
  extra_ignore_patterns?: string[];
}

export interface RemapResponse {
  status: "started";
  mode: "incremental" | "full";
}

export interface AnnotateResponse {
  status: "started";
}

/* ──────────────────────────────────────────────
   WebSocket event types (05-backend-schema.md §6 WS)
   ────────────────────────────────────────────── */

export interface WsMapProgress {
  event: "map_progress";
  phase: "structural" | "ai_enrichment";
  current: number;
  total: number;
  current_file?: string;
}

export interface WsMapPhaseComplete {
  event: "map_phase_complete";
  phase: "structural" | "ai_enrichment";
}

export interface WsNodeUpdated {
  event: "node_updated";
  node_id: string;
  fields_changed: string[];
}

export interface WsError {
  event: "error";
  scope: "file_parse" | "ai_call" | "remap";
  message: string;
  detail?: string;
}

export type WsEvent = WsMapProgress | WsMapPhaseComplete | WsNodeUpdated | WsError;

/* ──────────────────────────────────────────────
   App state machine (App Flow §5)
   ────────────────────────────────────────────── */

export type AppScreen =
  | "connecting"
  | "map_progress"
  | "system_overview"
  | "expanded_feature"
  | "expanded_file"
  | "flaws_report"
  | "settings";

export type Overlay =
  | { kind: "function_detail"; node_id: string }
  | { kind: "trace_view"; node_id: string }
  | { kind: "neighbors"; node_id: string }
  | { kind: "external_detail"; node_id: string }
  | { kind: "ask" }
  | null;

export interface AppState {
  screen: AppScreen;
  overlay: Overlay;
  breadcrumb: { label: string; node_id?: string }[];
  project: ProjectInfo | null;
  allNodes: Map<string, Node>;
  allEdges: Edge[];
  features: FeatureNode[];
  flaws: FlawEntry[];
  searchResults: SearchResult[];
  wsConnected: boolean;
  mapProgress: { phase: string; current: number; total: number; currentFile?: string } | null;
}
