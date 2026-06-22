//! C++ language adapter using tree-sitter-cpp.
//! Extends the C adapter with class/namespace scoping, method definitions,
//! and constructor/destructor extraction.

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use super::{
    AdapterOutput, ImportKind, ImportStatement, LanguageAdapter, SymbolDefinition, SymbolKind,
    SymbolReference,
};

pub struct CppAdapter;

impl LanguageAdapter for CppAdapter {
    fn language_id(&self) -> &'static str {
        "cpp"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["cpp", "cc", "cxx", "hpp", "hh", "hxx"]
    }

    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .context("failed to load tree-sitter-cpp grammar")?;

        let tree = parser
            .parse(source, None)
            .context("tree-sitter-cpp returned None")?;

        let mut extractor = CppExtractor {
            source,
            relative_path,
            output: AdapterOutput::default(),
            scope_stack: Vec::new(),
        };

        extractor.visit_node(tree.root_node(), None);
        Ok(extractor.output)
    }
}

// ---------------------------------------------------------------------------
// C++ extractor
// ---------------------------------------------------------------------------

struct CppExtractor<'a> {
    source: &'a str,
    #[allow(dead_code)]
    relative_path: &'a str,
    output: AdapterOutput,
    /// Enclosing class/namespace names for qualified name building.
    scope_stack: Vec<String>,
}

impl<'a> CppExtractor<'a> {
    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn qualified(&self, name: &str) -> String {
        if self.scope_stack.is_empty() {
            name.to_owned()
        } else {
            format!("{}::{}", self.scope_stack.last().unwrap(), name)
        }
    }

    fn visit_node(&mut self, node: Node, enclosing_fn: Option<&str>) {
        match node.kind() {
            "function_definition" => {
                self.handle_function(node);
                return; // body visited inside handle_function
            }
            "declaration" => {
                self.handle_declaration(node);
            }
            "class_specifier" | "struct_specifier" => {
                self.handle_class(node);
                return;
            }
            "namespace_definition" => {
                self.handle_namespace(node);
                return;
            }
            "preproc_include" => {
                self.handle_include(node);
            }
            "call_expression" => {
                if let Some(caller) = enclosing_fn {
                    self.handle_call(node, caller);
                }
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut c = args.walk();
                    for child in args.children(&mut c) {
                        self.visit_node(child, enclosing_fn);
                    }
                }
                return;
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, enclosing_fn);
        }
    }

    fn handle_function(&mut self, node: Node) {
        let name = self.extract_function_name(node);
        if name.is_empty() {
            return;
        }
        let qualified = self.qualified(&name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        let signature = self.extract_signature(node);
        let is_entry = is_cpp_entry_point(&name);

        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: qualified.clone(),
            kind: SymbolKind::Function,
            start_line,
            end_line,
            source_text,
            signature,
            is_entry_point_candidate: is_entry,
        });

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, Some(&qualified));
            }
        }
    }

    fn handle_class(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_owned())
            .unwrap_or_default();

        if !name.is_empty() {
            let start_line = node.start_position().row as u32 + 1;
            let end_line = node.end_position().row as u32 + 1;
            let qualified = self.qualified(&name);
            self.output.definitions.push(SymbolDefinition {
                name: name.clone(),
                qualified_name: qualified.clone(),
                kind: SymbolKind::Type,
                start_line,
                end_line,
                source_text: self.node_text(node).lines().next().unwrap_or("").to_owned(),
                signature: self.node_text(node).lines().next().unwrap_or("").to_owned(),
                is_entry_point_candidate: false,
            });
            self.scope_stack.push(qualified);
        }

        // Recurse into class body — handle access specifiers (public:/private:/protected:)
        // which wrap member declarations in the tree-sitter-cpp grammar.
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_class_body(body);
        }

        if !name.is_empty() {
            self.scope_stack.pop();
        }
    }

    fn visit_class_body(&mut self, body: Node) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "function_definition" => self.handle_function(child),
                "class_specifier" | "struct_specifier" => self.handle_class(child),
                "access_specifier" => {
                    // Pure label (e.g. "public:") — no children to recurse into
                }
                _ => {
                    // e.g. field_declaration, template_declaration — skip for now
                }
            }
        }
    }

    fn handle_namespace(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_owned())
            .unwrap_or_default();

        if !name.is_empty() {
            self.scope_stack.push(name.clone());
        }

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, None);
            }
        }

        if !name.is_empty() {
            self.scope_stack.pop();
        }
    }

    fn handle_declaration(&mut self, node: Node) {
        // Only top-level (translation_unit child) declarations
        if node.parent().map(|p| p.kind()) != Some("translation_unit") {
            return;
        }
        // Skip forward declarations of functions (no body)
        let name = self.extract_declarator_name(node);
        if name.is_empty() {
            return;
        }
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        self.output.definitions.push(SymbolDefinition {
            qualified_name: self.qualified(&name),
            name,
            kind: SymbolKind::Global,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    fn handle_include(&mut self, node: Node) {
        let raw = node
            .children(&mut node.walk())
            .find(|c| matches!(c.kind(), "string_literal" | "system_lib_string"))
            .map(|c| self.node_text(c).to_owned())
            .unwrap_or_default();
        let target = raw.trim_matches(|c| c == '"' || c == '<' || c == '>').to_owned();
        if target.is_empty() {
            return;
        }
        self.output.imports.push(ImportStatement {
            kind: ImportKind::Include,
            target,
            line: node.start_position().row as u32 + 1,
        });
    }

    fn handle_call(&mut self, node: Node, caller: &str) {
        let func_node = match node.child_by_field_name("function") {
            Some(n) => n,
            None => return,
        };
        let called_name = match func_node.kind() {
            "identifier" => self.node_text(func_node).to_owned(),
            "qualified_identifier" => self.node_text(func_node).to_owned(),
            "field_expression" => func_node
                .child_by_field_name("field")
                .map(|f| self.node_text(f).to_owned())
                .unwrap_or_else(|| self.node_text(func_node).to_owned()),
            _ => self.node_text(func_node).to_owned(),
        };
        if called_name.is_empty() {
            return;
        }
        let order = self.output.references.len() as i32;
        self.output.references.push(SymbolReference {
            from_qualified_name: caller.to_owned(),
            called_name,
            call_line: node.start_position().row as u32 + 1,
            order_hint: order,
            condition: None,
        });
    }

    fn extract_function_name(&self, node: Node) -> String {
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return self.extract_declarator_name_from(declarator);
        }
        String::new()
    }

    fn extract_declarator_name(&self, node: Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => return self.node_text(child).to_owned(),
                "init_declarator" | "function_declarator" | "pointer_declarator" => {
                    return self.extract_declarator_name_from(child);
                }
                _ => {}
            }
        }
        String::new()
    }

    fn extract_declarator_name_from(&self, node: Node) -> String {
        match node.kind() {
            "identifier" | "field_identifier" | "destructor_name" | "operator_name" => {
                self.node_text(node).to_owned()
            }
            "qualified_identifier" => {
                // e.g. MyClass::method — take the last segment
                node.child_by_field_name("name")
                    .map(|n| self.node_text(n).to_owned())
                    .unwrap_or_else(|| self.node_text(node).to_owned())
            }
            "function_declarator" => {
                if let Some(d) = node.child_by_field_name("declarator") {
                    return self.extract_declarator_name_from(d);
                }
                String::new()
            }
            "pointer_declarator" | "reference_declarator" => {
                if let Some(d) = node.child_by_field_name("declarator") {
                    return self.extract_declarator_name_from(d);
                }
                String::new()
            }
            _ => {
                // Try identifier child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return self.node_text(child).to_owned();
                    }
                }
                String::new()
            }
        }
    }

    fn extract_signature(&self, node: Node) -> String {
        let body_start = node
            .child_by_field_name("body")
            .map(|b| b.start_byte())
            .unwrap_or(node.end_byte());
        let sig = &self.source[node.start_byte()..body_start];
        sig.lines()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_owned()
    }
}

fn is_cpp_entry_point(name: &str) -> bool {
    matches!(name, "main" | "WinMain" | "wmain" | "wWinMain")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> AdapterOutput {
        CppAdapter.extract("test.cpp", src).unwrap()
    }

    #[test]
    fn finds_free_function() {
        let out = extract("int add(int a, int b) { return a + b; }");
        assert!(out.definitions.iter().any(|d| d.name == "add"));
    }

    #[test]
    fn finds_class() {
        let out = extract("class Foo { public: void bar() {} };");
        assert!(out.definitions.iter().any(|d| d.name == "Foo"));
    }

    #[test]
    fn finds_method_with_scope() {
        let out = extract("class Foo { public: void bar() {} };");
        // bar should exist and have Foo in its qualified name
        let bar = out.definitions.iter().find(|d| d.name == "bar");
        assert!(bar.is_some());
        assert!(bar.unwrap().qualified_name.contains("Foo"));
    }

    #[test]
    fn finds_include() {
        let out = extract("#include <vector>\nvoid f() {}");
        assert!(out.imports.iter().any(|i| i.target == "vector"));
    }

    #[test]
    fn finds_call() {
        let out = extract("void callee(){} void caller(){ callee(); }");
        assert!(out.references.iter().any(|r| r.called_name == "callee"));
    }
}
