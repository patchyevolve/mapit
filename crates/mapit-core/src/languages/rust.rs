//! Rust language adapter using tree-sitter-rust.
//! Extracts: function_item, impl methods, struct/enum/type definitions,
//! use statements, and call-expression references.

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use super::{
    AdapterOutput, ImportKind, ImportStatement, LanguageAdapter, SymbolDefinition, SymbolKind,
    SymbolReference,
};

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn supports_cfg(&self) -> bool { true }

    fn cfg_language(&self) -> Option<crate::control_flow::CfgLanguage> {
        Some(crate::control_flow::CfgLanguage::Rust)
    }

    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .context("failed to load tree-sitter-rust grammar")?;

        let tree = parser
            .parse(source, None)
            .context("tree-sitter-rust returned None (source too large or timeout)")?;

        let mut extractor = RustExtractor {
            source,
            relative_path,
            output: AdapterOutput::default(),
            scope_stack: Vec::new(),
        };

        extractor.visit_node(tree.root_node());
        Ok(extractor.output)
    }
}

// ---------------------------------------------------------------------------
// Internal extractor — walks the syntax tree
// ---------------------------------------------------------------------------

struct RustExtractor<'a> {
    source: &'a str,
    #[allow(dead_code)]
    relative_path: &'a str,
    output: AdapterOutput,
    /// Stack of enclosing named scopes, e.g. ["MyStruct", "impl MyStruct"].
    scope_stack: Vec<String>,
}

impl<'a> RustExtractor<'a> {
    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn visit_node(&mut self, node: Node) {
        match node.kind() {
            "function_item" => self.handle_function(node, false),
            "impl_item" => self.handle_impl(node),
            "struct_item" => self.handle_type_def(node, SymbolKind::Type),
            "enum_item" => self.handle_type_def(node, SymbolKind::Type),
            "type_item" => self.handle_type_def(node, SymbolKind::Type),
            "union_item" => self.handle_type_def(node, SymbolKind::Type),
            "macro_definition" => self.handle_macro(node),
            "use_declaration" => self.handle_use(node),
            "static_item" | "const_item" => self.handle_global(node),
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit_node(child);
                }
            }
        }
    }

    fn qualified_name(&self, name: &str) -> String {
        if self.scope_stack.is_empty() {
            name.to_owned()
        } else {
            format!("{}::{}", self.scope_stack.last().unwrap(), name)
        }
    }

    // ------------------------------------------------------------------
    // Handler: free function or method
    // ------------------------------------------------------------------
    fn handle_function(&mut self, node: Node, is_method: bool) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let qualified = self.qualified_name(&name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        let signature = self.extract_signature(node);
        let is_pub = self.is_pub(node);
        let is_entry = is_entry_point(&name, is_pub);

        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: qualified.clone(),
            kind: if is_method { SymbolKind::Method } else { SymbolKind::Function },
            start_line,
            end_line,
            source_text,
            signature,
            is_entry_point_candidate: is_entry,
        });

        // Extract call references inside this function body
        if let Some(body) = node.child_by_field_name("body") {
            let mut call_visitor = CallVisitor {
                source: self.source,
                caller_qualified: &qualified,
                calls: Vec::new(),
                order: 0,
            };
            call_visitor.visit(body);
            self.output.references.extend(call_visitor.calls);
        }
    }

    // ------------------------------------------------------------------
    // Handler: impl block — push scope, recurse into methods
    // ------------------------------------------------------------------
    fn handle_impl(&mut self, node: Node) {
        // Get the type name being impl'd
        let type_name = node
            .child_by_field_name("type")
            .map(|n| self.node_text(n).to_owned())
            .unwrap_or_else(|| "<unknown>".to_owned());

        self.scope_stack.push(type_name);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "declaration_list" {
                let mut c2 = child.walk();
                for item in child.children(&mut c2) {
                    if item.kind() == "function_item" {
                        self.handle_function(item, true);
                    } else {
                        self.visit_node(item);
                    }
                }
            }
        }

        self.scope_stack.pop();
    }

    // ------------------------------------------------------------------
    // Handler: struct / enum / type alias / union
    // ------------------------------------------------------------------
    fn handle_type_def(&mut self, node: Node, kind: SymbolKind) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let qualified = self.qualified_name(&name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();

        self.output.definitions.push(SymbolDefinition {
            name,
            qualified_name: qualified,
            kind,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    // ------------------------------------------------------------------
    // Handler: macro_definition
    // ------------------------------------------------------------------
    fn handle_macro(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();

        self.output.definitions.push(SymbolDefinition {
            qualified_name: self.qualified_name(&name),
            name,
            kind: SymbolKind::Macro,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    // ------------------------------------------------------------------
    // Handler: use declaration
    // ------------------------------------------------------------------
    fn handle_use(&mut self, node: Node) {
        let text = self.node_text(node);
        // Strip "use " prefix and trailing ";"
        let target = text
            .trim_start_matches("use ")
            .trim_end_matches(';')
            .trim()
            .to_owned();

        self.output.imports.push(ImportStatement {
            kind: ImportKind::Use,
            target,
            line: node.start_position().row as u32 + 1,
        });
    }

    // ------------------------------------------------------------------
    // Handler: static / const
    // ------------------------------------------------------------------
    fn handle_global(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();

        self.output.definitions.push(SymbolDefinition {
            qualified_name: self.qualified_name(&name),
            name,
            kind: SymbolKind::Global,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn is_pub(&self, node: Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "visibility_modifier" {
                return true;
            }
        }
        false
    }

    fn extract_signature(&self, node: Node) -> String {
        // Grab everything up to (but not including) the body block
        let body_start = node
            .child_by_field_name("body")
            .map(|b| b.start_position().column)
            .unwrap_or(0);

        let full = self.node_text(node);
        // Take only the first line if it contains the signature
        let first_line = full.lines().next().unwrap_or("");
        // Strip any trailing opening brace on the same line
        let sig = first_line.trim_end_matches('{').trim();
        let _ = body_start; // suppressed; column info available if needed
        sig.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Call-expression visitor
// ---------------------------------------------------------------------------

struct CallVisitor<'a> {
    source: &'a str,
    caller_qualified: &'a str,
    calls: Vec<SymbolReference>,
    order: i32,
}

impl<'a> CallVisitor<'a> {
    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn visit(&mut self, node: Node) {
        match node.kind() {
            "call_expression" => {
                self.handle_call(node);
                // Still recurse into the arguments — calls can be nested
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut cursor = args.walk();
                    for child in args.children(&mut cursor) {
                        self.visit(child);
                    }
                }
            }
            "macro_invocation" => {
                self.handle_macro_call(node);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit(child);
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit(child);
                }
            }
        }
    }

    fn handle_call(&mut self, node: Node) {
        let func_node = match node.child_by_field_name("function") {
            Some(n) => n,
            None => return,
        };

        let called_name = self.extract_called_name(func_node);
        if called_name.is_empty() {
            return;
        }

        self.calls.push(SymbolReference {
            from_qualified_name: self.caller_qualified.to_owned(),
            called_name,
            call_line: node.start_position().row as u32 + 1,
            order_hint: self.order,
            condition: None, // TODO Phase 3: extract branch condition context
        });
        self.order += 1;
    }

    fn handle_macro_call(&mut self, node: Node) {
        let macro_node = match node.child_by_field_name("macro") {
            Some(n) => n,
            None => return,
        };
        let name = self.node_text(macro_node).to_owned();
        if name.is_empty() {
            return;
        }
        self.calls.push(SymbolReference {
            from_qualified_name: self.caller_qualified.to_owned(),
            called_name: format!("{name}!"),
            call_line: node.start_position().row as u32 + 1,
            order_hint: self.order,
            condition: None,
        });
        self.order += 1;
    }

    /// Extract a human-readable callee name from the `function` sub-node.
    fn extract_called_name(&self, node: Node) -> String {
        match node.kind() {
            // Simple identifier: `foo()`
            "identifier" => self.node_text(node).to_owned(),
            // Path like `std::mem::drop` or `Self::new`
            "scoped_identifier" => self.node_text(node).to_owned(),
            // Method call: obj.method() — grab only the method name portion
            "field_expression" => {
                if let Some(field) = node.child_by_field_name("field") {
                    self.node_text(field).to_owned()
                } else {
                    self.node_text(node).to_owned()
                }
            }
            _ => self.node_text(node).to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry-point heuristic
// ---------------------------------------------------------------------------

fn is_entry_point(name: &str, is_pub: bool) -> bool {
    // main is always an entry point candidate
    if name == "main" {
        return true;
    }
    // Common embedded/OS entry points
    if matches!(
        name,
        "kmain"
            | "kernel_main"
            | "start"
            | "_start"
            | "init"
            | "setup"
            | "panic_handler"
            | "interrupt_handler"
    ) {
        return true;
    }
    // Any pub function could be called from outside the crate
    is_pub
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> AdapterOutput {
        RustAdapter.extract("test.rs", src).unwrap()
    }

    #[test]
    fn finds_free_function() {
        let out = extract("pub fn hello() -> u32 { 42 }");
        assert!(out.definitions.iter().any(|d| d.name == "hello"));
    }

    #[test]
    fn finds_struct() {
        let out = extract("struct Foo { x: u32 }");
        assert!(out.definitions.iter().any(|d| d.name == "Foo"));
    }

    #[test]
    fn finds_impl_method() {
        let src = r#"
            struct Bar;
            impl Bar {
                fn greet(&self) {}
            }
        "#;
        let out = extract(src);
        assert!(out.definitions.iter().any(|d| d.name == "greet"));
        // Qualified name should include impl scope
        assert!(out
            .definitions
            .iter()
            .any(|d| d.qualified_name.contains("Bar") && d.name == "greet"));
    }

    #[test]
    fn finds_call_reference() {
        let src = r#"
            fn caller() { callee(); }
            fn callee() {}
        "#;
        let out = extract(src);
        assert!(out.references.iter().any(|r| r.called_name == "callee"));
    }

    #[test]
    fn finds_use_import() {
        let out = extract("use std::collections::HashMap;");
        assert!(out.imports.iter().any(|i| i.target.contains("HashMap")));
    }

    #[test]
    fn main_is_entry_point() {
        let out = extract("fn main() {}");
        let main = out.definitions.iter().find(|d| d.name == "main").unwrap();
        assert!(main.is_entry_point_candidate);
    }

    #[test]
    fn private_fn_not_entry_point() {
        let out = extract("fn helper() {}");
        let helper = out.definitions.iter().find(|d| d.name == "helper").unwrap();
        assert!(!helper.is_entry_point_candidate);
    }

    #[test]
    fn pub_fn_is_entry_point_candidate() {
        let out = extract("pub fn exported() {}");
        let f = out.definitions.iter().find(|d| d.name == "exported").unwrap();
        assert!(f.is_entry_point_candidate);
    }
}
