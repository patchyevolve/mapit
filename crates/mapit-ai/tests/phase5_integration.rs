//! Phase 5 integration test — AI tasks + store round-trip.
//!
//! "done when" criteria (06-implementation-plan.md Phase 5):
//!   (a) Summarize/classify/flaw/answer tasks parse valid responses.
//!   (b) Dead-code gating function works correctly.
//!   (c) Tasks work with mocked AiProvider.
//!   (d) Annotations can be persisted to GraphStore and read back.

use mapit_ai::{
    provider::{AiProvider, AiRequest, AiResponse, ModelInfo},
    tasks::{self},
};
use mapit_core::graph::{
    builder::{self, FileInput, ParseResult},
    model::{AiSummaryStatus, Node},
    store::GraphStore,
};
use mapit_core::languages::adapter_for_language;

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

struct MockProvider {
    response: String,
}

impl AiProvider for MockProvider {
    fn id(&self) -> &str { "mock" }
    fn list_models(&self) -> Result<Vec<ModelInfo>, anyhow::Error> { Ok(vec![]) }
    fn complete(&self, _request: AiRequest) -> Result<AiResponse, anyhow::Error> {
        Ok(AiResponse {
            content: self.response.clone(),
            model_used: "mock-model".into(),
            finish_reason: Some("stop".into()),
        })
    }
    fn supports_streaming(&self) -> bool { false }
}

// ---------------------------------------------------------------------------
// Fixture (mirrors the three_file_rust fixture from mapit-core tests)
// ---------------------------------------------------------------------------

struct FixtureFile {
    relative_path: &'static str,
    language: &'static str,
    source: &'static str,
}

const FIXTURES: &[FixtureFile] = &[
    FixtureFile {
        relative_path: "src/main.rs",
        language: "rust",
        source: include_str!("../../mapit-core/tests/fixtures/three_file_rust/src/main.rs"),
    },
    FixtureFile {
        relative_path: "src/lib.rs",
        language: "rust",
        source: include_str!("../../mapit-core/tests/fixtures/three_file_rust/src/lib.rs"),
    },
    FixtureFile {
        relative_path: "src/utils.rs",
        language: "rust",
        source: include_str!("../../mapit-core/tests/fixtures/three_file_rust/src/utils.rs"),
    },
];

fn build_fixture() -> (GraphStore, Vec<Node>) {
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
    (store, result.nodes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn summarize_and_persist_round_trip() {
    let (store, nodes) = build_fixture();

    let provider = MockProvider {
        response: r#"{"summary": "This function handles initialization."}"#.into(),
    };

    // Annotate every function node
    for node in &nodes {
        if let Node::Function(f) = node {
            let result = tasks::summarize(
                &provider,
                "test-model",
                &f.base.name,
                "Function",
                f.base.file_path.as_deref().unwrap_or(""),
                f.base.span.as_ref().map(|s| s.start_line).unwrap_or(0),
                f.base.span.as_ref().map(|s| s.end_line).unwrap_or(0),
                f.base.language.as_deref().unwrap_or(""),
                "", // no source text available
                &f.signature,
                &[], // callers
                &[], // callees
            );
            assert!(result.is_ok(), "summarize failed for {}: {:?}", f.base.name, result.err());

            // Persist the annotation
            let mut updated = node.clone();
            updated.base_mut().ai_summary = Some(result.unwrap().summary);
            updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
            store.upsert_node(&updated).unwrap();
        }
    }

    // Read back and verify
    for node in &nodes {
        let stored = store.get_node(node.id()).unwrap().expect("node must exist");
        match &stored {
            Node::Function(f) => {
                assert_eq!(f.base.ai_summary_status, AiSummaryStatus::Ready);
                assert!(
                    f.base.ai_summary.is_some(),
                    "{} should have an AI summary",
                    f.base.name
                );
                assert_eq!(
                    f.base.ai_summary.as_deref(),
                    Some("This function handles initialization.")
                );
            }
            _ => {}
        }
    }
}

#[test]
fn dead_code_gating_works_on_fixture() {
    let (_store, nodes) = build_fixture();

    // never_called should be a dead code candidate
    let never_called = nodes
        .iter()
        .find(|n| n.base().name == "never_called")
        .expect("never_called must exist in fixture");
    assert!(
        mapit_core::graph::model::is_dead_code_candidate(never_called),
        "never_called should be dead code candidate"
    );

    // main should NOT be a dead code candidate (entry point)
    let main = nodes
        .iter()
        .find(|n| n.base().name == "main")
        .expect("main must exist");
    assert!(
        !mapit_core::graph::model::is_dead_code_candidate(main),
        "main should NOT be dead code candidate"
    );

    // Non-function nodes (files) should NOT be candidates
    for node in &nodes {
        if !matches!(node, Node::Function(_)) {
            assert!(
                !mapit_core::graph::model::is_dead_code_candidate(node),
                "non-function {:?} should not be dead code candidate",
                node.base().name
            );
        }
    }
}

#[test]
fn annotate_updates_store_read_back_same() {
    let (store, nodes) = build_fixture();

    let provider = MockProvider {
        response: r#"{"summary": "Mocked summary."}"#.into(),
    };

    // Simulate the annotate flow: for each function, call AI and store result
    let fn_nodes: Vec<&Node> = nodes.iter().filter(|n| matches!(n, Node::Function(_))).collect();
    assert!(fn_nodes.len() >= 4, "expected ≥4 functions");

    for node in &fn_nodes {
        let node_ref: &Node = node;
        if let Node::Function(f) = node_ref {
            let result = tasks::summarize(
                &provider, "m",
                &f.base.name, "Function",
                f.base.file_path.as_deref().unwrap_or(""),
                0, 0, "", "", &f.signature, &[], &[],
            ).expect("summarize ok");

            let mut updated = node_ref.clone();
            updated.base_mut().ai_summary = Some(result.summary);
            updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
            updated.base_mut().ai_model_used = Some("mock/m".into());
            store.upsert_node(&updated).unwrap();
        }
    }

    // Read back and verify all function nodes have summaries
    for node in &fn_nodes {
        let id = node.id();
        let stored = store.get_node(&id).unwrap().expect("must exist");
        match stored {
            Node::Function(f) => {
                assert_eq!(
                    f.base.ai_summary_status,
                    AiSummaryStatus::Ready,
                    "{} should be annotated",
                    f.base.name
                );
                assert!(
                    f.base.ai_summary.is_some(),
                    "{} should have summary",
                    f.base.name
                );
            }
            _ => panic!("expected function node"),
        }
    }

    // Verify AiSummaryStatus survives store round-trip
    let all_nodes = store.search_nodes_by_name("").unwrap();
    let re_read_fn: Vec<_> = all_nodes.iter().filter(|n| matches!(n, Node::Function(_))).collect();
    assert_eq!(re_read_fn.len(), fn_nodes.len(), "same number of function nodes");
}
