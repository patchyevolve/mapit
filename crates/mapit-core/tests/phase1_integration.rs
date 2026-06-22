//! Phase 1 integration test — hand-built fixture with KNOWN CORRECT answers.
//!
//! Fixture: tests/fixtures/three_file_rust/src/
//!   main.rs    → calls lib::compute
//!   lib.rs     → calls utils::double; defines never_called (no callers → dead code candidate)
//!   utils.rs   → calls println! (external/unresolved in project scope)
//!
//! "done when" criteria (06-implementation-plan.md Phase 1):
//!   ✓ at least 3 files → nodes
//!   ✓ at least one cross-file call edge
//!   ✓ at least one unresolved/external call
//!   ✓ never_called has has_incoming_calls == false

use mapit_core::{
    graph::{
        builder::{self, FileInput, ParseResult},
        model::{EdgeType, Node},
        store::GraphStore,
    },
    languages::adapter_for_language,
};

struct FixtureFile {
    relative_path: &'static str,
    language: &'static str,
    source: &'static str,
}

const FIXTURES: &[FixtureFile] = &[
    FixtureFile {
        relative_path: "src/main.rs",
        language: "rust",
        source: include_str!("fixtures/three_file_rust/src/main.rs"),
    },
    FixtureFile {
        relative_path: "src/lib.rs",
        language: "rust",
        source: include_str!("fixtures/three_file_rust/src/lib.rs"),
    },
    FixtureFile {
        relative_path: "src/utils.rs",
        language: "rust",
        source: include_str!("fixtures/three_file_rust/src/utils.rs"),
    },
];

fn build_fixture() -> (Vec<Node>, Vec<mapit_core::graph::model::Edge>) {
    let mut file_inputs: Vec<FileInput> = Vec::new();

    for fix in FIXTURES {
        let adapter = adapter_for_language(fix.language).expect("rust adapter exists");
        let output = adapter.extract(fix.relative_path, fix.source).expect("parse ok");
        file_inputs.push(FileInput {
            relative_path: fix.relative_path,
            language: fix.language,
            size_bytes: fix.source.len() as u64,
            parse_result: ParseResult::Ok(output),
            source: Some(fix.source),
        });
    }

    let result = builder::build(&file_inputs).expect("builder succeeds");
    (result.nodes, result.edges)
}

#[test]
fn fixture_produces_nodes_for_all_files() {
    let (nodes, _) = build_fixture();
    // Should have at least a FileNode for each of the 3 source files
    let file_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| matches!(n, Node::File(_)))
        .collect();
    assert!(
        file_nodes.len() >= 3,
        "expected ≥3 file nodes, got {}",
        file_nodes.len()
    );
}

#[test]
fn fixture_finds_all_functions() {
    let (nodes, _) = build_fixture();
    let fn_names: Vec<&str> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Function(f) => Some(f.base.name.as_str()),
            _ => None,
        })
        .collect();

    assert!(fn_names.contains(&"main"), "missing main; got {fn_names:?}");
    assert!(fn_names.contains(&"compute"), "missing compute; got {fn_names:?}");
    assert!(fn_names.contains(&"double"), "missing double; got {fn_names:?}");
    assert!(fn_names.contains(&"never_called"), "missing never_called; got {fn_names:?}");
}

#[test]
fn fixture_has_calls_edges() {
    let (_, edges) = build_fixture();
    let calls: Vec<_> = edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .collect();
    assert!(
        !calls.is_empty(),
        "expected at least one calls edge, found none"
    );
}

#[test]
fn fixture_has_external_node_for_unresolved_call() {
    let (nodes, edges) = build_fixture();
    let has_external = nodes.iter().any(|n| matches!(n, Node::External(_)));
    assert!(
        has_external,
        "expected at least one ExternalNode for unresolvable calls (e.g. println!)"
    );
    let has_unresolved_edge = edges.iter().any(|e| {
        matches!(e.edge_type, EdgeType::Calls)
            && matches!(
                e.confidence,
                mapit_core::graph::model::EdgeConfidence::DynamicUnresolved
            )
    });
    assert!(
        has_unresolved_edge,
        "expected at least one dynamic_unresolved calls edge"
    );
}

#[test]
fn never_called_has_no_incoming_calls_in_store() {
    // Build + persist to an in-memory store, then recompute_incoming_calls
    let store = GraphStore::open_in_memory().expect("in-memory store");

    let mut file_inputs: Vec<FileInput> = Vec::new();
    for fix in FIXTURES {
        let adapter = adapter_for_language(fix.language).unwrap();
        let output = adapter.extract(fix.relative_path, fix.source).unwrap();
        file_inputs.push(FileInput {
            relative_path: fix.relative_path,
            language: fix.language,
            size_bytes: fix.source.len() as u64,
            parse_result: ParseResult::Ok(output),
            source: Some(fix.source),
        });
    }

    let result = builder::build(&file_inputs).unwrap();
    for node in &result.nodes {
        store.upsert_node(node).unwrap();
    }
    for edge in &result.edges {
        store.upsert_edge(edge).unwrap();
    }
    store.recompute_incoming_calls().unwrap();

    // Find never_called and assert has_incoming_calls == false
    let never_called_node = result
        .nodes
        .iter()
        .find(|n| n.base().name == "never_called")
        .expect("never_called must exist in fixture");

    let stored = store
        .get_node(never_called_node.id())
        .unwrap()
        .expect("node must be in store");

    match stored {
        Node::Function(f) => {
            assert!(
                !f.has_incoming_calls,
                "never_called should have has_incoming_calls=false (dead code candidate)"
            );
        }
        _ => panic!("expected FunctionNode for never_called"),
    }
}

#[test]
fn main_is_entry_point_candidate() {
    let (nodes, _) = build_fixture();
    let main_node = nodes
        .iter()
        .find(|n| n.base().name == "main")
        .expect("main must exist");

    match main_node {
        Node::Function(f) => {
            assert!(
                f.is_entry_point_candidate,
                "main() must be flagged as is_entry_point_candidate"
            );
        }
        _ => panic!("main should be a FunctionNode"),
    }
}

#[test]
fn defines_edges_link_files_to_functions() {
    let (_, edges) = build_fixture();
    let defines_count = edges
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Defines))
        .count();
    assert!(
        defines_count >= 4,
        "expected ≥4 defines edges (one per function), got {defines_count}"
    );
}
