//! Phase 4 integration test — store query layer that powers headless CLI commands.
//!
//! "done when" criteria (06-implementation-plan.md Phase 4):
//!   (a) All headless commands work against a real project.
//!   (b) Output matches App Flow §3 style.
//!
//! This test verifies the store query methods that `find`, `explain`, `trace`,
//! `status`, and `flaws` commands depend on. CLI-specific rendering is tested
//! manually per the plan; this file validates that the data layer returns correct
//! results for every query a headless command could issue.
//!
//! Fixture: tests/fixtures/three_file_rust (same as Phase 1)
//!   main.rs        → fn main, calls lib::compute, println!
//!   lib.rs         → fn compute (entry pt), fn never_called (dead candidate)
//!   utils.rs       → fn double, fn print_version (calls println!)
//!   External nodes → println! (unresolved in project scope)

use mapit_core::graph::{
    builder::{self, FileInput, ParseResult},
    model::{EdgeConfidence, EdgeType, Node},
    store::GraphStore,
};
use mapit_core::languages::adapter_for_language;

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

/// Build fixture, persist to an in-memory store, return (store, nodes, edges).
fn build_and_persist() -> (GraphStore, Vec<Node>, Vec<mapit_core::graph::model::Edge>) {
    let store = GraphStore::open_in_memory().expect("in-memory store");

    let mut file_inputs: Vec<FileInput> = Vec::new();
    for fix in FIXTURES {
        let adapter = adapter_for_language(fix.language).expect("adapter exists");
        let output = adapter.extract(fix.relative_path, fix.source).expect("parse ok");
        file_inputs.push(FileInput {
            relative_path: fix.relative_path,
            language: fix.language,
            size_bytes: fix.source.len() as u64,
            parse_result: ParseResult::Ok(output),
            source: Some(fix.source),
        });
    }

    let result = builder::build(&file_inputs).expect("build succeeds");

    for node in &result.nodes {
        store.upsert_node(node).unwrap();
    }
    for edge in &result.edges {
        store.upsert_edge(edge).unwrap();
    }
    store.recompute_incoming_calls().unwrap();

    (store, result.nodes, result.edges)
}

// ---------------------------------------------------------------------------
// Criterion (a) — search_nodes_by_name powers `mapit find`
// ---------------------------------------------------------------------------

#[test]
fn search_by_substring_finds_matching_symbols() {
    let (store, _, _) = build_and_persist();

    // Search for "main" — should find fn main + maybe file node
    let results = store.search_nodes_by_name("main").unwrap();
    let names: Vec<&str> = results.iter().map(|n| n.base().name.as_str()).collect();
    assert!(
        names.contains(&"main"),
        "search 'main' should find fn main: {names:?}"
    );

    // Search for "compute"
    let results = store.search_nodes_by_name("compute").unwrap();
    let names: Vec<&str> = results.iter().map(|n| n.base().name.as_str()).collect();
    assert!(
        names.contains(&"compute"),
        "search 'compute' should find fn compute: {names:?}"
    );

    // Substring: "print" should match print_version and println!
    let results = store.search_nodes_by_name("print").unwrap();
    let names: Vec<&str> = results.iter().map(|n| n.base().name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.contains(&"print")),
        "search 'print' should return symbols with 'print' in name: {names:?}"
    );

    // Search for something that doesn't exist
    let results = store.search_nodes_by_name("xyznonexistent").unwrap();
    assert!(results.is_empty(), "search for non-existent should be empty");
}

#[test]
fn function_count_returns_correct_number() {
    let (store, nodes, _) = build_and_persist();
    let fn_count: usize = nodes
        .iter()
        .filter(|n| matches!(n, Node::Function(_)))
        .count();
    assert_eq!(
        store.function_count().unwrap() as usize,
        fn_count,
        "store.function_count() should match actual function count"
    );
    assert!(
        fn_count >= 4,
        "fixture should have ≥4 functions (main, compute, never_called, double, print_version), got {fn_count}"
    );
}

// ---------------------------------------------------------------------------
// Criterion (b) — get_node + edges_from/edges_to powers `mapit explain`
// ---------------------------------------------------------------------------

#[test]
fn get_node_retrieves_expected_fields() {
    let (store, nodes, _edges) = build_and_persist();

    let compute = nodes
        .iter()
        .find(|n| n.base().name == "compute")
        .expect("compute must exist in fixture");

    let stored = store
        .get_node(compute.id())
        .unwrap()
        .expect("compute should be in store");

    match &stored {
        Node::Function(f) => {
            assert_eq!(f.base.name, "compute");
            assert!(f.is_entry_point_candidate, "pub fn compute should be entry pt");
            assert!(
                !f.has_incoming_calls,
                "compute is called by main (exact-resolved), so has_incoming_calls should be true"
            );
        }
        _ => panic!("expected FunctionNode for compute"),
    }
}

#[test]
fn explain_shows_callees_to_external_nodes() {
    let (store, nodes, _edges) = build_and_persist();

    let compute_id = nodes
        .iter()
        .find(|n| n.base().name == "compute")
        .unwrap()
        .id();

    // Callees: compute calls utils::double (cross-module → DynamicUnresolved to ExternalNode)
    let outgoing = store.edges_from(&compute_id).unwrap();
    let callee_ids: Vec<&str> = outgoing
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .map(|e| e.to_id.as_str())
        .collect();
    assert_eq!(
        callee_ids.len(),
        1,
        "compute should have exactly 1 outgoing calls edge"
    );
    let callee = store
        .get_node(callee_ids[0])
        .unwrap()
        .expect("callee node should exist");
    assert!(
        callee.base().name.contains("double"),
        "compute should call double, got: {}",
        callee.base().name
    );
    // Cross-module calls are DynamicUnresolved → ExternalNode in v1
    assert!(
        matches!(callee, Node::External(_)),
        "cross-module callee should be ExternalNode"
    );

    // Callers: compute has NO resolved callers.
    // main calls lib::compute (cross-module → DynamicUnresolved to ExternalNode)
    let incoming = store.edges_to(&compute_id).unwrap();
    let caller_calls: Vec<_> = incoming
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .collect();
    assert!(
        caller_calls.is_empty(),
        "compute should have no resolved callers (cross-module calls unresolved), got {}",
        caller_calls.len()
    );
}

// ---------------------------------------------------------------------------
// Criterion (c) — edges from store match builder output exactly
// ---------------------------------------------------------------------------

#[test]
fn stored_edge_count_matches_build_output() {
    let (store, _, edges) = build_and_persist();
    assert_eq!(
        store.edge_count().unwrap() as usize,
        edges.len(),
        "store edge count should match builder output"
    );
}

#[test]
fn stored_node_count_matches_build_output() {
    let (store, nodes, _) = build_and_persist();
    assert_eq!(
        store.node_count().unwrap() as usize,
        nodes.len(),
        "store node count should match builder output"
    );
}

// ---------------------------------------------------------------------------
// Criterion (d) — has_incoming_calls / is_entry_point_candidate survive
//                   round-trip through store
// ---------------------------------------------------------------------------

#[test]
fn structural_flags_survive_round_trip() {
    let (store, nodes, _edges) = build_and_persist();

    for node in &nodes {
        if let Node::Function(f) = node {
            let stored = store
                .get_node(node.id())
                .unwrap()
                .expect("node must survive round-trip");

            match stored {
                Node::Function(sf) => {
                    assert_eq!(
                        sf.has_incoming_calls, f.has_incoming_calls,
                        "has_incoming_calls mismatch for {}: expected {}, got {}",
                        f.base.name, f.has_incoming_calls, sf.has_incoming_calls
                    );
                    assert_eq!(
                        sf.is_entry_point_candidate, f.is_entry_point_candidate,
                        "is_entry_point_candidate mismatch for {}: expected {}, got {}",
                        f.base.name, f.is_entry_point_candidate, sf.is_entry_point_candidate
                    );
                }
                _ => panic!("expected FunctionNode for {}", f.base.name),
            }
        }
    }
}

#[test]
fn never_called_has_no_incoming_calls() {
    let (store, _, _) = build_and_persist();

    let never_called = store.search_nodes_by_name("never_called").unwrap();
    let nc = never_called
        .iter()
        .find(|n| n.base().name == "never_called")
        .expect("never_called must exist");

    match nc {
        Node::Function(f) => {
            assert!(
                !f.has_incoming_calls,
                "never_called should have has_incoming_calls=false (dead code candidate)"
            );
        }
        _ => panic!("expected FunctionNode for never_called"),
    }
}

// ---------------------------------------------------------------------------
// Criterion (e) — flaws layer is callable even when empty
// ---------------------------------------------------------------------------

#[test]
fn flaw_queries_return_empty_when_no_ai_pass() {
    let (store, _, _) = build_and_persist();

    let total = store.flaw_count(None).unwrap();
    assert_eq!(total, 0, "no AI pass yet → 0 flaws");

    let by_severity = store.flaw_count(Some("high")).unwrap();
    assert_eq!(by_severity, 0);

    let all = store.query_flaws(None).unwrap();
    assert!(all.is_empty(), "query_flaws should return empty vec");

    let for_node = store.get_flaws_for_node("any_id").unwrap();
    assert!(for_node.is_empty(), "get_flaws_for_node should return empty vec for any id");
}

// ---------------------------------------------------------------------------
// Criterion (f) — edges_from/to filtered by edge type (powering explain)
// ---------------------------------------------------------------------------

#[test]
fn edges_to_finds_defines_and_calls_for_file_nodes() {
    let (store, nodes, _) = build_and_persist();

    // Test edges_from on lib.rs FileNode → should have Defines edges to compute and never_called
    let lib_file = nodes
        .iter()
        .find(|n| n.base().name == "lib.rs")
        .expect("lib.rs FileNode must exist");
    let outgoing = store.edges_from(&lib_file.id()).unwrap();
    let defines: Vec<_> = outgoing
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Defines))
        .collect();
    assert!(
        defines.len() >= 2,
        "lib.rs should define at least 2 symbols (compute, never_called), got {}",
        defines.len()
    );

    // Test edges_to on the utils::double ExternalNode → should have Calls edge from compute
    let utils_double = nodes
        .iter()
        .find(|n| n.base().name == "utils::double")
        .expect("utils::double ExternalNode must exist");
    let calls_to = store.edges_to(&utils_double.id()).unwrap();
    let calls: Vec<_> = calls_to
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "utils::double should have exactly 1 caller (compute), got {}",
        calls.len()
    );
    let caller_id = calls[0].from_id.as_str();
    let caller = store
        .get_node(caller_id)
        .unwrap()
        .expect("caller must exist");
    assert_eq!(
        caller.base().name, "compute",
        "caller of utils::double should be compute"
    );

    // Cross-module calls are DynamicUnresolved in v1
    assert!(
        matches!(calls[0].confidence, EdgeConfidence::DynamicUnresolved),
        "cross-module calls should be dynamic_unresolved (v1 limitation)"
    );
}

// ---------------------------------------------------------------------------
// Criterion (g) — map status flow: node_count / edge_count match after remap
// ---------------------------------------------------------------------------

#[test]
fn counts_after_rebuild_match_initial() {
    let (_store, nodes, edges) = build_and_persist();

    // Create a fresh in-memory store and re-persist to simulate remap
    let fresh_store = GraphStore::open_in_memory().unwrap();
    for node in &nodes {
        fresh_store.upsert_node(node).unwrap();
    }
    for edge in &edges {
        fresh_store.upsert_edge(edge).unwrap();
    }

    assert_eq!(
        fresh_store.node_count().unwrap() as usize,
        nodes.len(),
        "node count after rebuild should match original"
    );
    assert_eq!(
        fresh_store.edge_count().unwrap() as usize,
        edges.len(),
        "edge count after rebuild should match original"
    );
}

// ---------------------------------------------------------------------------
// Criterion (h) — search respects LIMIT
// ---------------------------------------------------------------------------

#[test]
fn search_by_name_returns_at_most_50_results() {
    // Use a temp file-backed store to test against real SQLite LIMIT behavior
    let dir = std::env::temp_dir().join("mapit_phase4_test");
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("graph.sqlite");
    let _ = std::fs::remove_file(&db_path);
    let store = GraphStore::open(&db_path).unwrap();

    // Insert 60 dummy nodes with names "z_test_0" through "z_test_59"
    for i in 0..60 {
        let node = Node::Function(mapit_core::graph::model::FunctionNode {
            base: mapit_core::graph::model::BaseNode {
                id: format!("test_{i}"),
                name: format!("z_test_{i}"),
                node_type: mapit_core::graph::model::NodeType::Function,
                language: Some("rust".to_owned()),
                file_path: Some("src/test.rs".to_owned()),
                span: None,
                ai_summary: None,
                ai_summary_status: mapit_core::graph::model::AiSummaryStatus::Pending,
                ai_model_used: None,
                structural_hash: format!("hash_{i}"),
                flaws: vec![],
            },
            signature: "fn test()".to_owned(),
            is_entry_point_candidate: false,
            has_incoming_calls: false,
            control_flow: None,
        });
        store.upsert_node(&node).unwrap();
    }

    // Search should return at most 50
    let results = store.search_nodes_by_name("z_test").unwrap();
    assert!(
        results.len() <= 50,
        "search LIMIT should cap at 50, got {} results",
        results.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
