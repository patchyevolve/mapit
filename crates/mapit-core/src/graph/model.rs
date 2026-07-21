//! Rust structs that are the authoritative mirror of docs/03-graph-data-model.md §1–§5.
//! If this file and that document ever disagree, the document wins — fix this file.
//! All field names are snake_case to match the spec's JSON-over-the-wire contract
//! (serialized with serde rename_all = "snake_case" throughout).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    #[default]
    Feature,
    Module,
    File,
    Function,
    Type,
    Macro,
    Global,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiSummaryStatus {
    Pending,
    Ready,
    Unavailable,
}

impl Default for AiSummaryStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Source span (line numbers, 1-indexed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
}

/// Fields common to every node type. (doc §1 BaseNode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseNode {
    pub id: String,
    /// The node type — NOT serialized here because the outer `Node` enum's
    /// `#[serde(tag = "type")]` already writes the discriminant as "type".
    /// Stored in-memory for convenience; reconstructed from the enum variant
    /// when deserializing.
    #[serde(skip)]
    pub node_type: NodeType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_summary: Option<String>,
    pub ai_summary_status: AiSummaryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_model_used: Option<String>,
    /// SHA-256 hex of the source span; drives incremental re-annotation.
    pub structural_hash: String,
    pub flaws: Vec<FlawFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureNode {
    #[serde(flatten)]
    pub base: BaseNode,
    pub member_node_ids: Vec<String>,
    pub classification_confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Ok,
    ParseFailed,
    UnsupportedLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    #[serde(flatten)]
    pub base: BaseNode,
    pub size_bytes: u64,
    pub parse_status: ParseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionNode {
    #[serde(flatten)]
    pub base: BaseNode,
    pub signature: String,
    pub is_entry_point_candidate: bool,
    pub has_incoming_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_flow: Option<ControlFlowGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalReason {
    NoSourcePresent,
    DynamicDispatch,
    UnrecognizedBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalNode {
    #[serde(flatten)]
    pub base: BaseNode,
    pub reason: ExternalReason,
}

/// Discriminated union over all node variants, for easy storage/retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Feature(FeatureNode),
    File(FileNode),
    Function(FunctionNode),
    External(ExternalNode),
    /// Module / Type / Macro / Global share the base fields only in v1.
    Module(BaseNode),
    #[serde(rename = "type")]
    TypeNode(BaseNode),
    Macro(BaseNode),
    Global(BaseNode),
}

impl Node {
    pub fn base(&self) -> &BaseNode {
        match self {
            Node::Feature(n) => &n.base,
            Node::File(n) => &n.base,
            Node::Function(n) => &n.base,
            Node::External(n) => &n.base,
            Node::Module(b) | Node::TypeNode(b) | Node::Macro(b) | Node::Global(b) => b,
        }
    }

    pub fn base_mut(&mut self) -> &mut BaseNode {
        match self {
            Node::Feature(n) => &mut n.base,
            Node::File(n) => &mut n.base,
            Node::Function(n) => &mut n.base,
            Node::External(n) => &mut n.base,
            Node::Module(b) | Node::TypeNode(b) | Node::Macro(b) | Node::Global(b) => b,
        }
    }

    /// Correct the `node_type` field on BaseNode after deserialization.
    /// Because `node_type` is `#[serde(skip)]` (to avoid the duplicate "type"
    /// field clash with the enum tag), it defaults to `NodeType::Feature` after
    /// deserialization and must be fixed up here.
    pub fn fix_node_type(&mut self) {
        let t = match self {
            Node::Feature(_) => NodeType::Feature,
            Node::File(_) => NodeType::File,
            Node::Function(_) => NodeType::Function,
            Node::External(_) => NodeType::External,
            Node::Module(_) => NodeType::Module,
            Node::TypeNode(_) => NodeType::Type,
            Node::Macro(_) => NodeType::Macro,
            Node::Global(_) => NodeType::Global,
        };
        self.base_mut().node_type = t;
    }

    pub fn id(&self) -> &str {
        &self.base().id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Calls,
    Includes,
    Defines,
    References,
    LinksInto,
    MemberOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeConfidence {
    Exact,
    Probable,
    DynamicUnresolved,
}

/// A single directed edge in the graph. (doc §2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    pub confidence: EdgeConfidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_hint: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlawKind {
    DeadCode,
    CircularDependency,
    StructuralSmell,
    SuspectedBug,
    MissingErrorHandling,
    ResourceLeakPattern,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlawSeverity {
    Info,
    Warning,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlawBasis {
    Structural,
    Ai,
    StructuralPlusAi,
}

/// An AI-assisted flaw annotation on a node. (doc §3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlawFlag {
    pub id: String,
    pub kind: FlawKind,
    pub severity: FlawSeverity,
    pub description: String,
    pub confidence: f64,
    pub basis: FlawBasis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related_node_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Sequential,
    Branch,
    Loop,
    Return,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInBlock {
    pub edge_id: String,
    pub order_hint: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTransition {
    pub block_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// A basic block in the control-flow graph. (doc §5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlFlowBlock {
    pub id: String,
    pub kind: BlockKind,
    pub calls_in_block: Vec<CallInBlock>,
    pub next_blocks: Vec<BlockTransition>,
}

/// The full control-flow graph for one function. (doc §5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlFlowGraph {
    pub entry_block_id: String,
    pub blocks: Vec<ControlFlowBlock>,
}

use sha2::{Digest, Sha256};

/// Compute a stable node ID: sha256(file_path + ":" + node_type + ":" + qualified_name)
/// truncated to 16 hex bytes (32 chars). Line numbers are NOT included so that
/// adding a blank line above a function does not change its ID.
pub fn compute_node_id(file_path: &str, node_type: &NodeType, qualified_name: &str) -> String {
    let type_str = match node_type {
        NodeType::Feature => "feature",
        NodeType::Module => "module",
        NodeType::File => "file",
        NodeType::Function => "function",
        NodeType::Type => "type",
        NodeType::Macro => "macro",
        NodeType::Global => "global",
        NodeType::External => "external",
    };
    let input = format!("{file_path}:{type_str}:{qualified_name}");
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(&hash[..16])
}

/// Compute a structural hash for a source span (used for incremental re-annotation).
pub fn compute_structural_hash(source: &str) -> String {
    let hash = Sha256::digest(source.as_bytes());
    hex::encode(&hash[..16])
}

/// Compute a stable edge ID: sha256(from_id + to_id + edge_type + order_hint)
pub fn compute_edge_id(
    from_id: &str,
    to_id: &str,
    edge_type: &EdgeType,
    order_hint: Option<i32>,
) -> String {
    let type_str = match edge_type {
        EdgeType::Calls => "calls",
        EdgeType::Includes => "includes",
        EdgeType::Defines => "defines",
        EdgeType::References => "references",
        EdgeType::LinksInto => "links_into",
        EdgeType::MemberOf => "member_of",
    };
    let hint = order_hint.map(|h| h.to_string()).unwrap_or_default();
    let input = format!("{from_id}:{to_id}:{type_str}:{hint}");
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(&hash[..16])
}

/// Dead-code gating rule (03-graph-data-model.md §3):
/// `dead_code` flaws must never be generated from AI judgment alone.
/// They must be gated on `has_incoming_calls == false` AND
/// `is_entry_point_candidate == false` first (structural facts),
/// with AI only assessing plausibility on top.
pub fn is_dead_code_candidate(node: &Node) -> bool {
    match node {
        Node::Function(f) => !f.has_incoming_calls && !f.is_entry_point_candidate,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_is_stable() {
        let a = compute_node_id("src/main.rs", &NodeType::Function, "main");
        let b = compute_node_id("src/main.rs", &NodeType::Function, "main");
        assert_eq!(a, b);
    }

    #[test]
    fn node_id_differs_by_scope() {
        let a = compute_node_id("src/foo.rs", &NodeType::Function, "Foo::new");
        let b = compute_node_id("src/foo.rs", &NodeType::Function, "Bar::new");
        assert_ne!(a, b);
    }

    #[test]
    fn structural_hash_length() {
        let h = compute_structural_hash("fn main() {}");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn dead_code_candidate_true_when_no_callers_and_not_entry() {
        let node = Node::Function(FunctionNode {
            base: BaseNode {
                id: "t".into(), name: "unused_helper".into(), node_type: NodeType::Function,
                language: None, file_path: None, span: None,
                ai_summary: None, ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None, structural_hash: "h".into(), flaws: vec![],
            },
            signature: "fn unused_helper()".into(),
            is_entry_point_candidate: false,
            has_incoming_calls: false,
            control_flow: None,
        });
        assert!(is_dead_code_candidate(&node));
    }

    #[test]
    fn dead_code_candidate_false_when_has_callers() {
        let node = Node::Function(FunctionNode {
            base: BaseNode {
                id: "t".into(), name: "used_fn".into(), node_type: NodeType::Function,
                language: None, file_path: None, span: None,
                ai_summary: None, ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None, structural_hash: "h".into(), flaws: vec![],
            },
            signature: "fn used_fn()".into(),
            is_entry_point_candidate: false,
            has_incoming_calls: true,
            control_flow: None,
        });
        assert!(!is_dead_code_candidate(&node));
    }

    #[test]
    fn dead_code_candidate_false_when_entry_point() {
        let node = Node::Function(FunctionNode {
            base: BaseNode {
                id: "t".into(), name: "main".into(), node_type: NodeType::Function,
                language: None, file_path: None, span: None,
                ai_summary: None, ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None, structural_hash: "h".into(), flaws: vec![],
            },
            signature: "fn main()".into(),
            is_entry_point_candidate: true,
            has_incoming_calls: false,
            control_flow: None,
        });
        assert!(!is_dead_code_candidate(&node));
    }

    #[test]
    fn dead_code_candidate_false_for_non_function() {
        let node = Node::File(FileNode {
            base: BaseNode {
                id: "f".into(), name: "file.rs".into(), node_type: NodeType::File,
                language: Some("rust".into()), file_path: Some("src/file.rs".into()), span: None,
                ai_summary: None, ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None, structural_hash: "h".into(), flaws: vec![],
            },
            size_bytes: 100, parse_status: ParseStatus::Ok, parse_error: None,
        });
        assert!(!is_dead_code_candidate(&node));
    }
}
