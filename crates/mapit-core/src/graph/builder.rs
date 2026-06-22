//! Graph builder: takes adapter output for a set of files and produces
//! Node/Edge values, resolves calls with confidence tiers per TRD §3.2.

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, warn};

use crate::languages::{AdapterOutput, ImportKind, SymbolKind};

use super::model::{
    AiSummaryStatus, BaseNode, Edge, EdgeConfidence, EdgeType, ExternalNode, ExternalReason,
    FileNode, FunctionNode, Node, NodeType, ParseStatus, compute_edge_id, compute_node_id,
    compute_structural_hash,
};

// ---------------------------------------------------------------------------
// Per-file input for the builder
// ---------------------------------------------------------------------------

pub struct FileInput<'a> {
    pub relative_path: &'a str,
    pub language: &'a str,
    pub size_bytes: u64,
    pub parse_result: ParseResult<'a>,
}

pub enum ParseResult<'a> {
    Ok(AdapterOutput),
    Failed { error: String },
    Unsupported,
    /// File unchanged since last run — skip re-building its nodes/edges.
    Unchanged,
    /// Source text needed for structural_hash computation (used in Ok path).
    #[allow(dead_code)]
    WithSource { output: AdapterOutput, source: &'a str },
}

// ---------------------------------------------------------------------------
// Build output
// ---------------------------------------------------------------------------

pub struct BuildOutput {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Build nodes and edges for a batch of files.
///
/// Resolution strategy (TRD §3.2):
/// 1. Exact — callee found in same file.
/// 2. Probable — callee found in an explicitly imported/included file.
/// 3. Probable — callee found anywhere else in the project (name match only).
/// 4. dynamic_unresolved — callee not found in project source; emits an
///    ExternalNode so the call is visible rather than silently dropped.
pub fn build(files: &[FileInput]) -> Result<BuildOutput> {
    // -----------------------------------------------------------------------
    // Pass 1: collect all definitions across all files keyed by qualified name
    // and by simple name (for fallback resolution).
    // -----------------------------------------------------------------------

    // qualified_name -> node_id
    let mut qualified_map: HashMap<String, String> = HashMap::new();
    // simple name -> list of (node_id, file_path) — for probable resolution
    let mut name_map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    // file_path -> set of qualified_names defined there
    let mut file_defs: HashMap<String, Vec<String>> = HashMap::new();

    let mut nodes: Vec<Node> = Vec::new();

    for file in files {
        // Always emit a FileNode for every walked file.
        let file_node = build_file_node(file);
        nodes.push(file_node);

        let definitions = match &file.parse_result {
            ParseResult::Ok(output) => &output.definitions,
            ParseResult::WithSource { output, .. } => &output.definitions,
            _ => continue,
        };

        for def in definitions {
            let node_type = match def.kind {
                SymbolKind::Function | SymbolKind::Method => NodeType::Function,
                SymbolKind::Type => NodeType::Type,
                SymbolKind::Macro => NodeType::Macro,
                SymbolKind::Global => NodeType::Global,
                SymbolKind::Module => NodeType::Module,
            };

            let node_id = compute_node_id(file.relative_path, &node_type, &def.qualified_name);
            let structural_hash = compute_structural_hash(&def.source_text);

            let base = BaseNode {
                id: node_id.clone(),
                node_type: node_type.clone(),
                name: def.name.clone(),
                language: Some(file.language.to_owned()),
                file_path: Some(file.relative_path.to_owned()),
                span: Some(super::model::Span {
                    start_line: def.start_line,
                    end_line: def.end_line,
                }),
                ai_summary: None,
                ai_summary_status: AiSummaryStatus::Pending,
                ai_model_used: None,
                structural_hash,
                flaws: vec![],
            };

            let node = match def.kind {
                SymbolKind::Function | SymbolKind::Method => Node::Function(FunctionNode {
                    base,
                    signature: def.signature.clone(),
                    is_entry_point_candidate: def.is_entry_point_candidate,
                    has_incoming_calls: false, // recomputed by store after full build
                    control_flow: None,
                }),
                SymbolKind::Type => Node::TypeNode(base),
                SymbolKind::Macro => Node::Macro(base),
                SymbolKind::Global => Node::Global(base),
                SymbolKind::Module => Node::Module(base),
            };

            // Register for resolution
            qualified_map.insert(def.qualified_name.clone(), node_id.clone());
            name_map
                .entry(def.name.clone())
                .or_default()
                .push((node_id.clone(), file.relative_path.to_owned()));
            file_defs
                .entry(file.relative_path.to_owned())
                .or_default()
                .push(def.qualified_name.clone());

            nodes.push(node);
        }
    }

    // -----------------------------------------------------------------------
    // Pass 2: emit "defines" edges (FileNode -> child nodes)
    // -----------------------------------------------------------------------

    let mut edges: Vec<Edge> = Vec::new();

    for file in files {
        let file_node_id =
            compute_node_id(file.relative_path, &NodeType::File, file.relative_path);

        let defs = file_defs.get(file.relative_path).cloned().unwrap_or_default();
        for qname in &defs {
            if let Some(child_id) = qualified_map.get(qname) {
                let edge_id =
                    compute_edge_id(&file_node_id, child_id, &EdgeType::Defines, None);
                edges.push(Edge {
                    id: edge_id,
                    from_id: file_node_id.clone(),
                    to_id: child_id.clone(),
                    edge_type: EdgeType::Defines,
                    confidence: EdgeConfidence::Exact,
                    order_hint: None,
                    condition: None,
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pass 3: emit "includes" edges from import statements
    // -----------------------------------------------------------------------

    for file in files {
        let file_node_id =
            compute_node_id(file.relative_path, &NodeType::File, file.relative_path);

        let imports = match &file.parse_result {
            ParseResult::Ok(output) => &output.imports,
            ParseResult::WithSource { output, .. } => &output.imports,
            _ => continue,
        };

        for import in imports {
            if import.kind == ImportKind::Use
                || import.kind == ImportKind::Include
                || import.kind == ImportKind::Import
            {
                // Try to find the target file by matching the import path
                // to a relative_path in the file set (best-effort, not exhaustive).
                let target_id =
                    resolve_import_to_file(&import.target, files, &qualified_map);

                if let Some(tid) = target_id {
                    let edge_id =
                        compute_edge_id(&file_node_id, &tid, &EdgeType::Includes, None);
                    edges.push(Edge {
                        id: edge_id,
                        from_id: file_node_id.clone(),
                        to_id: tid,
                        edge_type: EdgeType::Includes,
                        confidence: EdgeConfidence::Probable,
                        order_hint: None,
                        condition: None,
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pass 4: emit "calls" edges from symbol references
    // -----------------------------------------------------------------------

    // Build a mapping: file_path -> set of file_paths it imports (for
    // confidence tiering in resolution).
    let mut import_map: HashMap<String, Vec<String>> = HashMap::new();
    for file in files {
        let imports = match &file.parse_result {
            ParseResult::Ok(output) => &output.imports,
            ParseResult::WithSource { output, .. } => &output.imports,
            _ => continue,
        };
        for import in imports {
            // Record any resolved import targets
            if let Some(tid) = resolve_import_to_file(&import.target, files, &qualified_map) {
                import_map
                    .entry(file.relative_path.to_owned())
                    .or_default()
                    .push(tid);
            }
        }
    }

    let mut external_nodes: HashMap<String, Node> = HashMap::new();

    for file in files {
        let references = match &file.parse_result {
            ParseResult::Ok(output) => &output.references,
            ParseResult::WithSource { output, .. } => &output.references,
            _ => continue,
        };

        for reference in references {
            // Find the caller node id
            let caller_id = match qualified_map.get(&reference.from_qualified_name) {
                Some(id) => id.clone(),
                None => {
                    debug!(
                        "Caller {} not found in definition map — skipping reference to {}",
                        reference.from_qualified_name, reference.called_name
                    );
                    continue;
                }
            };

            // Resolve the callee
            let (callee_id, confidence) = resolve_call(
                &reference.called_name,
                file.relative_path,
                &qualified_map,
                &name_map,
                &import_map,
                &file_defs,
            );

            let callee_id = match callee_id {
                Some(id) => id,
                None => {
                    // Unresolvable — emit an ExternalNode + dynamic_unresolved edge
                    let ext_id = compute_node_id(
                        "<external>",
                        &NodeType::External,
                        &reference.called_name,
                    );
                    external_nodes.entry(ext_id.clone()).or_insert_with(|| {
                        Node::External(ExternalNode {
                            base: BaseNode {
                                id: ext_id.clone(),
                                node_type: NodeType::External,
                                name: reference.called_name.clone(),
                                language: None,
                                file_path: None,
                                span: None,
                                ai_summary: None,
                                ai_summary_status: AiSummaryStatus::Unavailable,
                                ai_model_used: None,
                                structural_hash: compute_structural_hash(&reference.called_name),
                                flaws: vec![],
                            },
                            reason: ExternalReason::NoSourcePresent,
                        })
                    });
                    ext_id
                }
            };

            let edge_id = compute_edge_id(
                &caller_id,
                &callee_id,
                &EdgeType::Calls,
                Some(reference.order_hint),
            );
            edges.push(Edge {
                id: edge_id,
                from_id: caller_id,
                to_id: callee_id,
                edge_type: EdgeType::Calls,
                confidence,
                order_hint: Some(reference.order_hint),
                condition: reference.condition.clone(),
            });
        }
    }

    // Add external nodes to the output
    nodes.extend(external_nodes.into_values());

    debug!(
        "Builder produced {} nodes, {} edges",
        nodes.len(),
        edges.len()
    );
    Ok(BuildOutput { nodes, edges })
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

/// Resolve a call to (node_id, confidence), or None if truly unresolvable.
fn resolve_call(
    called_name: &str,
    caller_file: &str,
    qualified_map: &HashMap<String, String>,
    name_map: &HashMap<String, Vec<(String, String)>>,
    import_map: &HashMap<String, Vec<String>>,
    file_defs: &HashMap<String, Vec<String>>,
) -> (Option<String>, EdgeConfidence) {
    // Strip trailing `!` from macro calls for resolution purposes
    let lookup_name = called_name.trim_end_matches('!');

    // 1. Exact: qualified name matches directly (e.g. "MyStruct::method")
    if let Some(id) = qualified_map.get(lookup_name) {
        return (Some(id.clone()), EdgeConfidence::Exact);
    }

    // 2. Exact: simple name defined in the same file
    if let Some(same_file_defs) = file_defs.get(caller_file) {
        for qname in same_file_defs {
            let simple = qname.rsplit("::").next().unwrap_or(qname);
            if simple == lookup_name {
                if let Some(id) = qualified_map.get(qname) {
                    return (Some(id.clone()), EdgeConfidence::Exact);
                }
            }
        }
    }

    // 3. Probable: name found in an explicitly imported file
    if let Some(imported_files) = import_map.get(caller_file) {
        for imp_file in imported_files {
            if let Some(imp_defs) = file_defs.get(imp_file) {
                for qname in imp_defs {
                    let simple = qname.rsplit("::").next().unwrap_or(qname);
                    if simple == lookup_name {
                        if let Some(id) = qualified_map.get(qname) {
                            return (Some(id.clone()), EdgeConfidence::Probable);
                        }
                    }
                }
            }
        }
    }

    // 4. Probable: name found anywhere in the project (single match)
    if let Some(candidates) = name_map.get(lookup_name) {
        if candidates.len() == 1 {
            return (Some(candidates[0].0.clone()), EdgeConfidence::Probable);
        } else if candidates.len() > 1 {
            // Ambiguous — pick the first but mark as probable
            warn!(
                "Ambiguous call to '{}' from '{}': {} candidates, using first",
                called_name,
                caller_file,
                candidates.len()
            );
            return (Some(candidates[0].0.clone()), EdgeConfidence::Probable);
        }
    }

    // 5. Unresolved — will become ExternalNode + dynamic_unresolved edge
    (None, EdgeConfidence::DynamicUnresolved)
}

/// Best-effort: given an import target string (e.g. "std::collections::HashMap"
/// or "crate::foo::bar"), find a matching file node id.
fn resolve_import_to_file(
    target: &str,
    files: &[FileInput],
    _qualified_map: &HashMap<String, String>,
) -> Option<String> {
    // Convert module path separators to filesystem path separators
    let as_path = target
        .replace("::", "/")
        .replace('.', "/")
        .to_lowercase();

    for file in files {
        let fp = file.relative_path.to_lowercase();
        let fp_no_ext = fp.rsplit('.').skip(1).collect::<Vec<_>>().join(".");
        if fp_no_ext.ends_with(&as_path) || fp_no_ext.replace('/', "::") == target {
            return Some(compute_node_id(
                file.relative_path,
                &NodeType::File,
                file.relative_path,
            ));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// FileNode constructor
// ---------------------------------------------------------------------------

fn build_file_node(file: &FileInput) -> Node {
    let (parse_status, parse_error) = match &file.parse_result {
        ParseResult::Ok(_) | ParseResult::WithSource { .. } | ParseResult::Unchanged => {
            (ParseStatus::Ok, None)
        }
        ParseResult::Failed { error } => (ParseStatus::ParseFailed, Some(error.clone())),
        ParseResult::Unsupported => (ParseStatus::UnsupportedLanguage, None),
    };

    let node_id = compute_node_id(file.relative_path, &NodeType::File, file.relative_path);

    Node::File(FileNode {
        base: BaseNode {
            id: node_id,
            node_type: NodeType::File,
            name: file
                .relative_path
                .rsplit('/')
                .next()
                .unwrap_or(file.relative_path)
                .to_owned(),
            language: Some(file.language.to_owned()),
            file_path: Some(file.relative_path.to_owned()),
            span: None,
            ai_summary: None,
            ai_summary_status: AiSummaryStatus::Pending,
            ai_model_used: None,
            structural_hash: String::new(),
            flaws: vec![],
        },
        size_bytes: file.size_bytes,
        parse_status,
        parse_error,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::{
        AdapterOutput, SymbolDefinition, SymbolKind, SymbolReference,
    };

    fn make_file<'a>(
        path: &'a str,
        lang: &'a str,
        output: AdapterOutput,
    ) -> FileInput<'a> {
        FileInput {
            relative_path: path,
            language: lang,
            size_bytes: 0,
            parse_result: ParseResult::Ok(output),
        }
    }

    #[test]
    fn builds_function_node() {
        let mut output = AdapterOutput::default();
        output.definitions.push(SymbolDefinition {
            name: "main".to_owned(),
            qualified_name: "main".to_owned(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 3,
            source_text: "fn main() {}".to_owned(),
            signature: "fn main()".to_owned(),
            is_entry_point_candidate: true,
        });

        let files = vec![make_file("src/main.rs", "rust", output)];
        let result = build(&files).unwrap();

        // Should have a FileNode + FunctionNode
        assert_eq!(result.nodes.len(), 2);
        assert!(result.nodes.iter().any(|n| n.base().name == "main"));
    }

    #[test]
    fn resolves_same_file_call() {
        let mut output = AdapterOutput::default();
        output.definitions.push(SymbolDefinition {
            name: "caller".to_owned(),
            qualified_name: "caller".to_owned(),
            kind: SymbolKind::Function,
            start_line: 1, end_line: 3,
            source_text: "fn caller() { callee(); }".to_owned(),
            signature: "fn caller()".to_owned(),
            is_entry_point_candidate: false,
        });
        output.definitions.push(SymbolDefinition {
            name: "callee".to_owned(),
            qualified_name: "callee".to_owned(),
            kind: SymbolKind::Function,
            start_line: 5, end_line: 7,
            source_text: "fn callee() {}".to_owned(),
            signature: "fn callee()".to_owned(),
            is_entry_point_candidate: false,
        });
        output.references.push(SymbolReference {
            from_qualified_name: "caller".to_owned(),
            called_name: "callee".to_owned(),
            call_line: 2,
            order_hint: 0,
            condition: None,
        });

        let files = vec![make_file("src/lib.rs", "rust", output)];
        let result = build(&files).unwrap();

        let calls: Vec<_> = result
            .edges
            .iter()
            .filter(|e| matches!(e.edge_type, EdgeType::Calls))
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].confidence, EdgeConfidence::Exact);
    }

    #[test]
    fn unresolved_call_produces_external_node() {
        let mut output = AdapterOutput::default();
        output.definitions.push(SymbolDefinition {
            name: "caller".to_owned(),
            qualified_name: "caller".to_owned(),
            kind: SymbolKind::Function,
            start_line: 1, end_line: 3,
            source_text: "fn caller() { extern_func(); }".to_owned(),
            signature: "fn caller()".to_owned(),
            is_entry_point_candidate: false,
        });
        output.references.push(SymbolReference {
            from_qualified_name: "caller".to_owned(),
            called_name: "extern_func".to_owned(),
            call_line: 2,
            order_hint: 0,
            condition: None,
        });

        let files = vec![make_file("src/lib.rs", "rust", output)];
        let result = build(&files).unwrap();

        assert!(result.nodes.iter().any(|n| matches!(n, Node::External(_))));
        let ext_call = result
            .edges
            .iter()
            .find(|e| matches!(e.edge_type, EdgeType::Calls));
        assert!(ext_call.is_some());
        assert_eq!(ext_call.unwrap().confidence, EdgeConfidence::DynamicUnresolved);
    }

    #[test]
    fn defines_edges_emitted() {
        let mut output = AdapterOutput::default();
        output.definitions.push(SymbolDefinition {
            name: "foo".to_owned(),
            qualified_name: "foo".to_owned(),
            kind: SymbolKind::Function,
            start_line: 1, end_line: 2,
            source_text: "fn foo() {}".to_owned(),
            signature: "fn foo()".to_owned(),
            is_entry_point_candidate: false,
        });

        let files = vec![make_file("src/foo.rs", "rust", output)];
        let result = build(&files).unwrap();

        assert!(result
            .edges
            .iter()
            .any(|e| matches!(e.edge_type, EdgeType::Defines)));
    }
}
