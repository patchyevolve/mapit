//! Phase 3 integration test — incremental remap + trace-walk verification.
//!
//! "done when" criteria (06-implementation-plan.md Phase 3):
//!   (a) Re-running the pipeline after modifying exactly one file
//!       only re-processes that file (verified via a counter).
//!   (b) A trace on a function with an if-branch produces two
//!       correctly labeled paths.

use mapit_core::control_flow::{CfgLanguage, extract_cfg, walk_trace};
use mapit_core::graph::{
    builder::{self, FileInput, ParseResult},
    incremental::{diff_manifest, changed_count, changed_paths, ManifestFile, ManifestEntry},
    model::Node,
    store::GraphStore,
};
use mapit_core::languages::adapter_for_language;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Fixture — three_file_rust
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

/// Compute a SHA-256 content hash for incremental diffing.
fn content_hash(source: &str) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(source.as_bytes());
    format!("sha256:{}", hex::encode(&hash[..16]))
}

// ---------------------------------------------------------------------------
// Criterion (a): Incremental remap — only changed file is re-processed
// ---------------------------------------------------------------------------

#[test]
fn incremental_diff_detects_single_changed_file() {
    // Build initial manifest from the 3 fixture files
    let mut manifest = ManifestFile::new();
    for fix in FIXTURES {
        manifest.files.insert(
            fix.relative_path.to_owned(),
            ManifestEntry {
                content_hash: content_hash(fix.source),
                language: Some(fix.language.to_owned()),
                last_parsed_at: "2026-06-20T10:00:00Z".to_owned(),
                parse_status: "ok".to_owned(),
            },
        );
    }

    // Compute current hashes (initially unchanged)
    let current: HashMap<String, String> = FIXTURES
        .iter()
        .map(|fix| (fix.relative_path.to_owned(), content_hash(fix.source)))
        .collect();

    let diff1 = diff_manifest(&current, &manifest);
    assert_eq!(
        changed_count(&diff1),
        0,
        "should detect 0 changes when nothing changed"
    );

    // "Modify" lib.rs by appending a comment
    let modified_source = format!("{}\n// trailing comment\n", FIXTURES[1].source);
    let mut current_modified = current.clone();
    current_modified.insert("src/lib.rs".to_owned(), content_hash(&modified_source));

    let diff2 = diff_manifest(&current_modified, &manifest);
    assert_eq!(
        changed_count(&diff2),
        1,
        "should detect exactly 1 change after modifying lib.rs"
    );

    let paths = changed_paths(&diff2);
    assert_eq!(paths, vec!["src/lib.rs"], "only lib.rs should be in changed_paths");
}

#[test]
fn incremental_build_only_reprocesses_changed_file() {
    // Full build of all 3 fixture files
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

    let first_result = builder::build(&file_inputs).expect("first build succeeds");
    let _first_fn_count = first_result
        .nodes
        .iter()
        .filter(|n| matches!(n, Node::Function(_)))
        .count();

    // Store nodes/edges in an in-memory DB (simulating the full pipeline)
    let store = GraphStore::open_in_memory().expect("in-memory store");
    for node in &first_result.nodes {
        store.upsert_node(node).unwrap();
    }
    for edge in &first_result.edges {
        store.upsert_edge(edge).unwrap();
    }

    // Build a second time — but this time mark main.rs and utils.rs as Unchanged,
    // and re-process lib.rs with modified content.
    let modified_source = format!("{}\n// added function\nfn added() -> u32 {{ 42 }}\n", FIXTURES[1].source);
    let mut second_inputs: Vec<FileInput> = Vec::new();

    for fix in FIXTURES {
        if fix.relative_path == "src/lib.rs" {
            let adapter = adapter_for_language(fix.language).unwrap();
            let output = adapter.extract(fix.relative_path, &modified_source).unwrap();
            second_inputs.push(FileInput {
                relative_path: fix.relative_path,
                language: fix.language,
                size_bytes: modified_source.len() as u64,
                parse_result: ParseResult::Ok(output),
                source: Some(&modified_source),
            });
        } else {
            // Unchanged — signal builder to skip re-processing
            second_inputs.push(FileInput {
                relative_path: fix.relative_path,
                language: fix.language,
                size_bytes: fix.source.len() as u64,
                parse_result: ParseResult::Unchanged,
                source: None,
            });
        }
    }

    let second_result = builder::build(&second_inputs).expect("second build succeeds");

    // Count new function nodes — only the "added" function from lib.rs should appear
    // (the builder still emits FileNodes for unchanged files, but not their definitions)
    let second_fn_nodes: Vec<&Node> = second_result
        .nodes
        .iter()
        .filter(|n| matches!(n, Node::Function(_)))
        .collect();

    // Build 1 had 4 functions (main, compute, double, never_called).
    // Build 2 should have 1 new function (added) — but the builder doesn't
    // know about the old ones; it produces whatever it parsed this run.
    // So second_fn_nodes should be the functions in lib.rs only.
    let fn_names: Vec<&str> = second_fn_nodes
        .iter()
        .filter_map(|n| match n {
            Node::Function(f) => Some(f.base.name.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        fn_names.contains(&"compute"),
        "lib.rs's compute should still be in the output, got {fn_names:?}"
    );
    assert!(
        fn_names.contains(&"never_called"),
        "lib.rs's never_called should still be in the output, got {fn_names:?}"
    );
    assert!(
        fn_names.contains(&"added"),
        "the newly added function should be in the output, got {fn_names:?}"
    );
    assert!(
        !fn_names.contains(&"main"),
        "main (from unchanged main.rs) should NOT be re-processed, got {fn_names:?}"
    );
    assert!(
        !fn_names.contains(&"double"),
        "double (from unchanged utils.rs) should NOT be re-processed, got {fn_names:?}"
    );
}

// ---------------------------------------------------------------------------
// Criterion (b): Trace on an if-branch produces two labeled paths
// ---------------------------------------------------------------------------

#[test]
fn trace_if_branch_produces_two_labeled_paths() {
    let cfg = extract_cfg(
        CfgLanguage::Rust,
        "{ if condition { do_thing(); } else { do_other(); } }",
        "test_fn_id",
    )
    .expect("CFG extraction succeeds");

    let paths = walk_trace(&cfg);

    assert_eq!(
        paths.len(),
        2,
        "if-else should produce exactly 2 paths, got {}",
        paths.len()
    );

    let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
    assert!(
        labels.iter().any(|l| l.contains("condition") && !l.contains("not")),
        "expected a path with the condition text 'condition', got {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.contains("not (condition)") || l.contains("else")),
        "expected a path with 'not (condition)' or 'else', got {labels:?}"
    );
}

#[test]
fn trace_c_if_branch_produces_two_labeled_paths() {
    let cfg = extract_cfg(
        CfgLanguage::C,
        "{ if (ptr != NULL) { use(ptr); } else { fallback(); } }",
        "test_fn_id",
    )
    .expect("C CFG extraction succeeds");

    let paths = walk_trace(&cfg);

    assert_eq!(paths.len(), 2, "C if-else should produce 2 paths, got {}", paths.len());

    let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
    assert!(
        labels.iter().any(|l| l.contains("ptr != NULL") && !l.contains("not")),
        "expected a path with 'ptr != NULL', got {labels:?}"
    );
}

#[test]
fn trace_sequential_has_no_condition_labels() {
    let cfg = extract_cfg(
        CfgLanguage::Rust,
        "{ a(); b(); c(); }",
        "seq_fn",
    )
    .expect("CFG extraction succeeds");

    let paths = walk_trace(&cfg);
    assert_eq!(paths.len(), 1, "sequential body should produce 1 path");
    // Sequential path has no conditions → label is empty string
    assert_eq!(paths[0].label, "", "sequential path should have empty label");
    assert!(paths[0].blocks.len() >= 1, "should have at least the entry block");
}
