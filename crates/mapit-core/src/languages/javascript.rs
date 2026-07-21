//! JavaScript and TypeScript language adapters using tree-sitter-javascript
//! and tree-sitter-typescript. Both share the same extraction logic;
//! only the grammar differs.
//!
//! Extracts: function declarations, arrow functions, class definitions,
//! method definitions, import/require statements, and call expressions.

use anyhow::{Context, Result};
use tree_sitter::{Language, Node, Parser};

use super::{
    AdapterOutput, ImportKind, ImportStatement, LanguageAdapter, SymbolDefinition, SymbolKind,
    SymbolReference,
};

// ---------------------------------------------------------------------------
// Adapter structs
// ---------------------------------------------------------------------------

pub struct JavaScriptAdapter;
pub struct TypeScriptAdapter;

impl LanguageAdapter for JavaScriptAdapter {
    fn language_id(&self) -> &'static str { "javascript" }
    fn file_extensions(&self) -> &'static [&'static str] { &["js", "mjs", "cjs", "jsx"] }
    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput> {
        extract_with_grammar(
            tree_sitter_javascript::LANGUAGE.into(),
            relative_path,
            source,
        )
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn language_id(&self) -> &'static str { "typescript" }
    fn file_extensions(&self) -> &'static [&'static str] { &["ts", "mts", "cts", "tsx"] }
    fn extract(&self, relative_path: &str, source: &str) -> Result<AdapterOutput> {
        // tree-sitter-typescript exposes separate languages for .ts and .tsx
        let lang: Language = if relative_path.ends_with(".tsx") {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };
        extract_with_grammar(lang, relative_path, source)
    }
}

// ---------------------------------------------------------------------------
// Shared extraction logic
// ---------------------------------------------------------------------------

fn extract_with_grammar(lang: Language, relative_path: &str, source: &str) -> Result<AdapterOutput> {
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .context("failed to set JS/TS grammar")?;

    let tree = parser
        .parse(source, None)
        .context("tree-sitter JS/TS returned None")?;

    let mut extractor = JsExtractor {
        source,
        relative_path,
        output: AdapterOutput::default(),
        scope_stack: Vec::new(),
    };

    extractor.visit_node(tree.root_node(), None);
    Ok(extractor.output)
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

struct JsExtractor<'a> {
    source: &'a str,
    #[allow(dead_code)]
    relative_path: &'a str,
    output: AdapterOutput,
    scope_stack: Vec<String>,
}

impl<'a> JsExtractor<'a> {
    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn qualified(&self, name: &str) -> String {
        if self.scope_stack.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{}", self.scope_stack.last().unwrap(), name)
        }
    }

    fn visit_node(&mut self, node: Node, enclosing_fn: Option<&str>) {
        match node.kind() {
            "function_declaration" | "function" => {
                self.handle_function_declaration(node);
                return;
            }
            "arrow_function" => {
                // Arrow functions assigned to variables: `const foo = () => {}`
                // We need the parent lexical_declaration to get the name.
                // Handled in lexical_declaration below.
            }
            "lexical_declaration" | "variable_declaration" => {
                self.handle_variable_decl(node);
                return;
            }
            "class_declaration" | "class" => {
                self.handle_class(node);
                return;
            }
            "method_definition" => {
                self.handle_method(node);
                return;
            }
            "import_statement" => {
                self.handle_import(node);
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

    fn handle_function_declaration(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        self.emit_function(node, &name, false);
    }

    fn handle_variable_decl(&mut self, node: Node) {
        // Look for: `const/let/var name = () => {}` or `= function() {}`
        let mut matched = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| self.node_text(n))
                    .unwrap_or("")
                    .to_owned();
                if name.is_empty() {
                    continue;
                }
                if let Some(value) = child.child_by_field_name("value") {
                    if matches!(value.kind(), "arrow_function" | "function") {
                        matched.push((value, name));
                    }
                }
            }
        }
        if !matched.is_empty() {
            for (value, name) in matched {
                self.emit_function(value, &name, false);
            }
            return;
        }
        // Also recurse for other variable declarations
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            self.visit_node(child, None);
        }
    }

    fn handle_class(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let qualified = self.qualified(&name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let src = self.node_text(node);
        let sig = src.lines().next().unwrap_or("").to_owned();

        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: qualified.clone(),
            kind: SymbolKind::Type,
            start_line,
            end_line,
            source_text: sig.clone(),
            signature: sig,
            is_entry_point_candidate: false,
        });

        self.scope_stack.push(qualified);
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, None);
            }
        }
        self.scope_stack.pop();
    }

    fn handle_method(&mut self, node: Node) {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        self.emit_function(node, &name, true);
    }

    fn emit_function(&mut self, node: Node, name: &str, is_method: bool) {
        let qualified = self.qualified(name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        let sig = source_text.lines().next().unwrap_or("").to_owned();
        let is_entry = name == "main";

        self.output.definitions.push(SymbolDefinition {
            name: name.to_owned(),
            qualified_name: qualified.clone(),
            kind: if is_method { SymbolKind::Method } else { SymbolKind::Function },
            start_line,
            end_line,
            source_text,
            signature: sig,
            is_entry_point_candidate: is_entry,
        });

        // Recurse into body
        // Arrow function: body may be an expression, not a block.
        // Collect child indices first to avoid borrow conflict with cursor.
        let body_node = node.child_by_field_name("body").or_else(|| {
            let child_count = node.child_count();
            for i in 0..child_count {
                if let Some(ch) = node.child(i) {
                    let k = ch.kind();
                    if k != "=>" && k != "(" && k != ")"
                        && k != "formal_parameters"
                        && k != "identifier"
                        && k != "type_annotation"
                    {
                        return Some(ch);
                    }
                }
            }
            None
        });

        if let Some(body) = body_node {
            self.scope_stack.push(qualified.clone());
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, Some(&qualified));
            }
            self.scope_stack.pop();
        }
    }

    fn handle_import(&mut self, node: Node) {
        // `import ... from 'module'`
        if let Some(source_node) = node.child_by_field_name("source") {
            let raw = self.node_text(source_node);
            let target = raw.trim_matches(|c| c == '\'' || c == '"').to_owned();
            if !target.is_empty() {
                self.output.imports.push(ImportStatement {
                    kind: ImportKind::Import,
                    target,
                    line: node.start_position().row as u32 + 1,
                });
            }
        }
    }

    fn handle_call(&mut self, node: Node, caller: &str) {
        let func_node = match node.child_by_field_name("function") {
            Some(n) => n,
            None => return,
        };

        let called_name = match func_node.kind() {
            "identifier" => self.node_text(func_node).to_owned(),
            "member_expression" => {
                // obj.method — grab method property name
                func_node
                    .child_by_field_name("property")
                    .map(|p| self.node_text(p).to_owned())
                    .unwrap_or_else(|| self.node_text(func_node).to_owned())
            }
            _ => self.node_text(func_node).to_owned(),
        };

        if called_name.is_empty() {
            return;
        }

        // Skip `require()` — handle it as an import instead
        if called_name == "require" {
            self.handle_require(node);
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

    fn handle_require(&mut self, node: Node) {
        // `require('module')` — treat as an import
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for child in args.children(&mut cursor) {
                if child.kind() == "string" {
                    let raw = self.node_text(child);
                    let target = raw.trim_matches(|c| c == '\'' || c == '"').to_owned();
                    if !target.is_empty() {
                        self.output.imports.push(ImportStatement {
                            kind: ImportKind::Require,
                            target,
                            line: node.start_position().row as u32 + 1,
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_js(src: &str) -> AdapterOutput {
        JavaScriptAdapter.extract("test.js", src).unwrap()
    }

    fn extract_ts(src: &str) -> AdapterOutput {
        TypeScriptAdapter.extract("test.ts", src).unwrap()
    }

    #[test]
    fn js_finds_function_declaration() {
        let out = extract_js("function greet(name) { return name; }");
        assert!(out.definitions.iter().any(|d| d.name == "greet"));
    }

    #[test]
    fn js_finds_arrow_function() {
        let out = extract_js("const add = (a, b) => a + b;");
        assert!(out.definitions.iter().any(|d| d.name == "add"));
    }

    #[test]
    fn js_finds_class() {
        let out = extract_js("class Animal { speak() {} }");
        assert!(out.definitions.iter().any(|d| d.name == "Animal"));
    }

    #[test]
    fn js_finds_method_with_class_scope() {
        let out = extract_js("class Dog { bark() {} }");
        let bark = out.definitions.iter().find(|d| d.name == "bark").unwrap();
        assert!(bark.qualified_name.contains("Dog"));
    }

    #[test]
    fn js_finds_import() {
        let out = extract_js("import path from 'path';\nfunction f() {}");
        assert!(out.imports.iter().any(|i| i.target == "path"));
    }

    #[test]
    fn js_finds_call() {
        let out = extract_js("function a() { b(); } function b() {}");
        assert!(out.references.iter().any(|r| r.called_name == "b"));
    }

    #[test]
    fn ts_finds_function() {
        let out = extract_ts("function greet(name: string): string { return name; }");
        assert!(out.definitions.iter().any(|d| d.name == "greet"));
    }

    #[test]
    fn ts_finds_class_with_method() {
        let src = "class Greeter { greet(name: string): void {} }";
        let out = extract_ts(src);
        assert!(out.definitions.iter().any(|d| d.name == "Greeter"));
        let m = out.definitions.iter().find(|d| d.name == "greet").unwrap();
        assert!(m.qualified_name.contains("Greeter"));
    }
}
