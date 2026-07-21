//! Python language adapter using tree-sitter-python.
//! Extracts: function/method definitions, class definitions,
//! import statements, and call expressions.

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use super::{
    AdapterOutput, ImportKind, ImportStatement, LanguageAdapter, SymbolDefinition, SymbolKind,
    SymbolReference,
};

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language_id(&self) -> &'static str {
        "python"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["py"]
    }

    fn extract(&self, _relative_path: &str, source: &str) -> Result<AdapterOutput> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .context("failed to load tree-sitter-python grammar")?;

        let tree = parser
            .parse(source, None)
            .context("tree-sitter-python returned None")?;

        let mut extractor = PythonExtractor {
            source,
            output: AdapterOutput::default(),
            scope_stack: Vec::new(),
        };

        extractor.visit_node(tree.root_node(), None);
        Ok(extractor.output)
    }
}

struct PythonExtractor<'a> {
    source: &'a str,
    output: AdapterOutput,
    scope_stack: Vec<String>,
}

impl<'a> PythonExtractor<'a> {
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
            "function_definition" => {
                self.handle_function(node);
                return;
            }
            "class_definition" => {
                self.handle_class(node);
                return;
            }
            "import_statement" => self.handle_import(node),
            "import_from_statement" => self.handle_import_from(node),
            "call" => {
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
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n))
            .unwrap_or("<anonymous>")
            .to_owned();

        let qualified = self.qualified(&name);
        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;
        let source_text = self.node_text(node).to_owned();
        let sig_line = source_text.lines().next().unwrap_or("").to_owned();
        let is_entry = name == "main" || name == "__main__";

        self.output.definitions.push(SymbolDefinition {
            name: name.clone(),
            qualified_name: qualified.clone(),
            kind: if self.scope_stack.is_empty() {
                SymbolKind::Function
            } else {
                SymbolKind::Method
            },
            start_line,
            end_line,
            source_text,
            signature: sig_line,
            is_entry_point_candidate: is_entry,
        });

        if let Some(body) = node.child_by_field_name("body") {
            self.scope_stack.push(qualified.clone());
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                self.visit_node(child, Some(&qualified));
            }
            self.scope_stack.pop();
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

    fn handle_import(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "dotted_name" || child.kind() == "aliased_import" {
                let target = if child.kind() == "aliased_import" {
                    child
                        .child_by_field_name("name")
                        .map(|n| self.node_text(n))
                        .unwrap_or("")
                        .to_owned()
                } else {
                    self.node_text(child).to_owned()
                };
                if !target.is_empty() {
                    self.output.imports.push(ImportStatement {
                        kind: ImportKind::Import,
                        target,
                        line: node.start_position().row as u32 + 1,
                    });
                }
            }
        }
    }

    fn handle_import_from(&mut self, node: Node) {
        let module = node
            .child_by_field_name("module_name")
            .map(|n| self.node_text(n))
            .unwrap_or("")
            .to_owned();
        if !module.is_empty() {
            self.output.imports.push(ImportStatement {
                kind: ImportKind::Import,
                target: module,
                line: node.start_position().row as u32 + 1,
            });
        }
    }

    fn handle_call(&mut self, node: Node, caller: &str) {
        let func_node = match node.child_by_field_name("function") {
            Some(n) => n,
            None => return,
        };
        let called_name = match func_node.kind() {
            "identifier" => self.node_text(func_node).to_owned(),
            "attribute" => {
                // obj.method — grab only the method attribute name
                func_node
                    .child_by_field_name("attribute")
                    .map(|a| self.node_text(a).to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(src: &str) -> AdapterOutput {
        PythonAdapter.extract("test.py", src).unwrap()
    }

    #[test]
    fn finds_function() {
        let out = extract("def add(a, b):\n    return a + b\n");
        assert!(out.definitions.iter().any(|d| d.name == "add"));
    }

    #[test]
    fn finds_class() {
        let out = extract("class Foo:\n    pass\n");
        assert!(out.definitions.iter().any(|d| d.name == "Foo"));
    }

    #[test]
    fn finds_method_with_scope() {
        let src = "class Foo:\n    def bar(self):\n        pass\n";
        let out = extract(src);
        let bar = out.definitions.iter().find(|d| d.name == "bar").unwrap();
        assert!(bar.qualified_name.contains("Foo"));
    }

    #[test]
    fn finds_import() {
        let out = extract("import os\ndef f(): pass\n");
        assert!(out.imports.iter().any(|i| i.target == "os"));
    }

    #[test]
    fn finds_from_import() {
        let out = extract("from pathlib import Path\ndef f(): pass\n");
        assert!(out.imports.iter().any(|i| i.target == "pathlib"));
    }

    #[test]
    fn finds_call() {
        let out = extract("def caller():\n    callee()\ndef callee():\n    pass\n");
        assert!(out.references.iter().any(|r| r.called_name == "callee"));
    }
}
