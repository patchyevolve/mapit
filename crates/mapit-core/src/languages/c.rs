//! C language adapter using tree-sitter-c.
//! Extracts: function definitions, struct/union/enum type definitions,
//! global variables/constants, #include directives, and call expressions.
//! Also handles extern "C" markers for cross-language edge linking (TRD §4.3).

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use super::{
    AdapterOutput, ImportKind, ImportStatement, LanguageAdapter, SymbolDefinition, SymbolKind,
    SymbolReference,
};

pub struct CAdapter;

impl LanguageAdapter for CAdapter {
    fn language_id(&self) -> &'static str {
        "c"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }

    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .context("failed to load tree-sitter-c grammar")?;

        let tree = parser
            .parse(source, None)
            .context("tree-sitter-c returned None")?;

        let mut extractor = CExtractor {
            source,
            relative_path,
            output: AdapterOutput::default(),
        };

        extractor.visit_node(tree.root_node(), None);
        Ok(extractor.output)
    }
}

// ---------------------------------------------------------------------------
// Shared helper — used by both C and C++ adapters
// ---------------------------------------------------------------------------

pub(crate) struct CExtractor<'a> {
    pub(crate) source: &'a str,
    pub(crate) relative_path: &'a str,
    pub(crate) output: AdapterOutput,
}

impl<'a> CExtractor<'a> {
    pub(crate) fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    pub(crate) fn visit_node(&mut self, node: Node, enclosing_fn: Option<&str>) {
        match node.kind() {
            "function_definition" => self.handle_function(node),
            "declaration" => self.handle_declaration(node),
            "struct_specifier" | "union_specifier" | "enum_specifier" => {
                self.handle_type_def(node)
            }
            "preproc_include" => self.handle_include(node),
            "call_expression" => {
                if let Some(caller) = enclosing_fn {
                    self.handle_call(node, caller);
                }
                // recurse into call arguments
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut c = args.walk();
                    for child in args.children(&mut c) {
                        self.visit_node(child, enclosing_fn);
                    }
                }
                return; // avoid double-recursing below
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

        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        let signature = self.extract_signature(node);
        let is_entry = is_c_entry_point(&name);

        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: name.clone(),
            kind: SymbolKind::Function,
            start_line,
            end_line,
            source_text,
            signature,
            is_entry_point_candidate: is_entry,
        });

        // Extract calls from the function body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, Some(&name));
            }
        }
    }

    fn extract_function_name(&self, node: Node) -> String {
        // C function name is inside the declarator
        if let Some(declarator) = node.child_by_field_name("declarator") {
            return self.declarator_name(declarator);
        }
        String::new()
    }

    pub(crate) fn declarator_name(&self, node: Node) -> String {
        match node.kind() {
            "identifier" => self.node_text(node).to_owned(),
            "function_declarator" => {
                if let Some(inner) = node.child_by_field_name("declarator") {
                    return self.declarator_name(inner);
                }
                String::new()
            }
            "pointer_declarator" => {
                if let Some(inner) = node.child_by_field_name("declarator") {
                    return self.declarator_name(inner);
                }
                String::new()
            }
            _ => {
                // Try to find an identifier child
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
        // Take the text up to (but not including) the body
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

    fn handle_declaration(&mut self, node: Node) {
        // Only interested in top-level variable/constant declarations
        // (inside translation_unit, not inside a function)
        if node.parent().map(|p| p.kind()) != Some("translation_unit") {
            return;
        }
        let name = self.extract_declaration_name(node);
        if name.is_empty() {
            return;
        }
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: name,
            kind: SymbolKind::Global,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    fn extract_declaration_name(&self, node: Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "init_declarator" => {
                    if let Some(d) = child.child_by_field_name("declarator") {
                        return self.declarator_name(d);
                    }
                }
                "function_declarator" | "pointer_declarator" | "identifier" => {
                    return self.declarator_name(child);
                }
                _ => {}
            }
        }
        String::new()
    }

    fn handle_type_def(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_owned())
            .unwrap_or_default();
        if name.is_empty() {
            return;
        }
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        self.output.definitions.push(SymbolDefinition {
            qualified_name: name.clone(),
            name,
            kind: SymbolKind::Type,
            start_line,
            end_line,
            source_text: source_text.clone(),
            signature: source_text.lines().next().unwrap_or("").to_owned(),
            is_entry_point_candidate: false,
        });
    }

    fn handle_include(&mut self, node: Node) {
        // #include "foo.h" or #include <foo.h>
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
            "field_expression" | "pointer_expression" => {
                // obj->method or obj.field — grab the field name
                func_node
                    .child_by_field_name("field")
                    .map(|f| self.node_text(f).to_owned())
                    .unwrap_or_else(|| self.node_text(func_node).to_owned())
            }
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
}

fn is_c_entry_point(name: &str) -> bool {
    matches!(
        name,
        "main"
            | "kmain"
            | "kernel_main"
            | "start"
            | "_start"
            | "init"
            | "setup"
            | "module_init"
            | "module_exit"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> AdapterOutput {
        CAdapter.extract("test.c", src).unwrap()
    }

    #[test]
    fn finds_function() {
        let out = extract("int add(int a, int b) { return a + b; }");
        assert!(out.definitions.iter().any(|d| d.name == "add"));
    }

    #[test]
    fn finds_struct() {
        let out = extract("struct Point { int x; int y; };");
        assert!(out.definitions.iter().any(|d| d.name == "Point"));
    }

    #[test]
    fn finds_include() {
        let out = extract("#include <stdio.h>\nint main() { return 0; }");
        assert!(out.imports.iter().any(|i| i.target == "stdio.h"));
    }

    #[test]
    fn finds_call() {
        let out = extract("void caller() { callee(); } void callee() {}");
        assert!(out.references.iter().any(|r| r.called_name == "callee"));
    }

    #[test]
    fn main_is_entry_point() {
        let out = extract("int main(int argc, char **argv) { return 0; }");
        let m = out.definitions.iter().find(|d| d.name == "main").unwrap();
        assert!(m.is_entry_point_candidate);
    }

    #[test]
    fn finds_pointer_function() {
        let out = extract("static int *get_ptr(void) { return 0; }");
        assert!(out.definitions.iter().any(|d| d.name == "get_ptr"));
    }
}
