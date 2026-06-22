//! Phase 2 integration tests — all language adapters + cross-language C/asm fixture.
//!
//! "done when" criteria (06-implementation-plan.md Phase 2):
//!   ✓ C, C++, asm, Python, JS/TS adapters parse without panicking
//!   ✓ Each adapter extracts at least one definition from its fixture
//!   ✓ Cross-language C→asm edge: C's main() calls asm_boot_routine (dynamic_unresolved
//!     since asm is separate file — or probable if name-matched across the project)
//!   ✓ Cross-language asm→C edge: asm_boot_routine calls c_helper (probable/dynamic_unresolved)
//!   ✓ dynamic_unresolved edges emitted for calls with no matching source

use mapit_core::{
    graph::{
        builder::{self, FileInput, ParseResult},
        model::{EdgeConfidence, EdgeType, Node},
    },
    languages::adapter_for_language,
};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn parse_file(language: &str, relative_path: &str, source: &str) -> FileInput<'static> {
    // We need 'static for FileInput in the vec; use Box::leak for test convenience.
    let source: &'static str = Box::leak(source.to_owned().into_boxed_str());
    let relative_path: &'static str = Box::leak(relative_path.to_owned().into_boxed_str());
    let language: &'static str = Box::leak(language.to_owned().into_boxed_str());

    let adapter = adapter_for_language(language).expect("adapter exists");
    let parse_result = match adapter.extract(relative_path, source) {
        Ok(output) => ParseResult::Ok(output),
        Err(e) => ParseResult::Failed { error: e.to_string() },
    };
    FileInput {
        relative_path,
        language,
        size_bytes: source.len() as u64,
        parse_result,
    }
}

// ---------------------------------------------------------------------------
// Per-adapter smoke tests
// ---------------------------------------------------------------------------

#[test]
fn c_adapter_extracts_definitions() {
    let src = r#"
#include <stdio.h>
struct Point { int x; int y; };
int add(int a, int b) { return a + b; }
int main(void) { add(1, 2); return 0; }
"#;
    let fi = parse_file("c", "src/main.c", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    let fn_names: Vec<&str> = result.nodes.iter()
        .filter_map(|n| match n { Node::Function(f) => Some(f.base.name.as_str()), _ => None })
        .collect();
    assert!(fn_names.contains(&"add"), "missing add: {fn_names:?}");
    assert!(fn_names.contains(&"main"), "missing main: {fn_names:?}");
    // main should be entry point
    let main = result.nodes.iter().find(|n| n.base().name == "main").unwrap();
    if let Node::Function(f) = main {
        assert!(f.is_entry_point_candidate);
    }
}

#[test]
fn c_adapter_emits_calls_edge() {
    let src = "void callee(void) {} void caller(void) { callee(); }";
    let fi = parse_file("c", "src/a.c", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    assert!(result.edges.iter().any(|e| matches!(e.edge_type, EdgeType::Calls)));
}

#[test]
fn cpp_adapter_extracts_class_and_method() {
    let src = "class Foo { public: void bar() { } };";
    let fi = parse_file("cpp", "src/foo.cpp", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    let names: Vec<&str> = result.nodes.iter().map(|n| n.base().name.as_str()).collect();
    assert!(names.contains(&"Foo"), "missing Foo: {names:?}");
    assert!(names.contains(&"bar"), "missing bar: {names:?}");
}

#[test]
fn asm_adapter_extracts_labels() {
    let src = ".globl my_func\nmy_func:\n  xor eax, eax\n  ret\n";
    let fi = parse_file("asm", "src/boot.s", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    assert!(result.nodes.iter().any(|n| n.base().name == "my_func"));
}

#[test]
fn python_adapter_extracts_class_and_function() {
    let src = "class Parser:\n    def parse(self, text):\n        return text\n";
    let fi = parse_file("python", "src/parser.py", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    let names: Vec<&str> = result.nodes.iter().map(|n| n.base().name.as_str()).collect();
    assert!(names.contains(&"Parser"), "{names:?}");
    assert!(names.contains(&"parse"), "{names:?}");
}

#[test]
fn js_adapter_extracts_function_and_class() {
    let src = "class Animal { speak() {} }\nfunction main() { new Animal(); }";
    let fi = parse_file("javascript", "src/app.js", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    let names: Vec<&str> = result.nodes.iter().map(|n| n.base().name.as_str()).collect();
    assert!(names.contains(&"Animal"), "{names:?}");
    assert!(names.contains(&"main"), "{names:?}");
}

#[test]
fn ts_adapter_extracts_typed_function() {
    let src = "function greet(name: string): string { return name; }";
    let fi = parse_file("typescript", "src/greet.ts", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    assert!(result.nodes.iter().any(|n| n.base().name == "greet"));
}

// ---------------------------------------------------------------------------
// Cross-language C/asm fixture test (TRD §4.3 requirement)
// ---------------------------------------------------------------------------

#[test]
fn c_asm_cross_language_edges() {
    let c_src = include_str!("fixtures/c_asm_crosslang/main.c");
    let asm_src = include_str!("fixtures/c_asm_crosslang/boot.s");
    let h_src = include_str!("fixtures/c_asm_crosslang/boot.h");

    let files = vec![
        parse_file("c", "main.c", c_src),
        parse_file("asm", "boot.s", asm_src),
        parse_file("c", "boot.h", h_src),
    ];

    let result = builder::build(&files).unwrap();

    // Verify all key symbols exist
    let names: Vec<&str> = result.nodes.iter().map(|n| n.base().name.as_str()).collect();
    assert!(names.contains(&"main"), "main not found: {names:?}");
    assert!(names.contains(&"c_helper"), "c_helper not found: {names:?}");
    assert!(
        names.contains(&"asm_boot_routine"),
        "asm_boot_routine not found: {names:?}"
    );

    // Verify C→asm call edge exists (main calls asm_boot_routine).
    // Resolution: asm_boot_routine is defined in boot.s, called from main.c.
    // With name-based resolution across files it should be at least probable.
    let c_to_asm_edge = result.edges.iter().find(|e| {
        if !matches!(e.edge_type, EdgeType::Calls) {
            return false;
        }
        let caller = result.nodes.iter().find(|n| n.id() == e.from_id);
        let callee = result.nodes.iter().find(|n| n.id() == e.to_id);
        matches!(
            (caller, callee),
            (Some(c), Some(a)) if c.base().name == "main" && a.base().name == "asm_boot_routine"
        )
    });
    assert!(
        c_to_asm_edge.is_some(),
        "expected C→asm calls edge (main→asm_boot_routine), edges: {:#?}",
        result.edges.iter()
            .filter(|e| matches!(e.edge_type, EdgeType::Calls))
            .map(|e| {
                let from = result.nodes.iter().find(|n| n.id() == e.from_id).map(|n| n.base().name.as_str()).unwrap_or("?");
                let to = result.nodes.iter().find(|n| n.id() == e.to_id).map(|n| n.base().name.as_str()).unwrap_or("?");
                format!("{from}→{to} ({:?})", e.confidence)
            })
            .collect::<Vec<_>>()
    );

    // Verify asm→C call edge exists (asm_boot_routine calls c_helper).
    let asm_to_c_edge = result.edges.iter().find(|e| {
        if !matches!(e.edge_type, EdgeType::Calls) {
            return false;
        }
        let caller = result.nodes.iter().find(|n| n.id() == e.from_id);
        let callee = result.nodes.iter().find(|n| n.id() == e.to_id);
        matches!(
            (caller, callee),
            (Some(a), Some(c)) if a.base().name == "asm_boot_routine" && c.base().name == "c_helper"
        )
    });
    assert!(
        asm_to_c_edge.is_some(),
        "expected asm→C calls edge (asm_boot_routine→c_helper)"
    );

    // Confidence may be probable (name-matched across files) or dynamic_unresolved
    // (if not found at all — but they ARE in the file set so should be at least probable)
    if let Some(edge) = c_to_asm_edge {
        assert!(
            matches!(edge.confidence, EdgeConfidence::Probable | EdgeConfidence::Exact),
            "C→asm edge should be probable or exact, got {:?}", edge.confidence
        );
    }
}

// ---------------------------------------------------------------------------
// Unresolved call → ExternalNode for each adapter
// ---------------------------------------------------------------------------

#[test]
fn c_unresolved_call_produces_external_node() {
    let src = "void f(void) { some_extern_func(); }";
    let fi = parse_file("c", "src/f.c", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    assert!(result.nodes.iter().any(|n| matches!(n, Node::External(_))));
}

#[test]
fn python_unresolved_call_produces_external_node() {
    let src = "def f():\n    os.path.join('a', 'b')\n";
    let fi = parse_file("python", "src/f.py", src);
    let result = builder::build(std::slice::from_ref(&fi)).unwrap();
    // 'join' or the method call should produce an external node
    assert!(
        result.nodes.iter().any(|n| matches!(n, Node::External(_))),
        "expected external node for unresolved call"
    );
}
