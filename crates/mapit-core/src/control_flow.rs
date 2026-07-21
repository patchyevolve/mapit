//! Control-flow graph (CFG) extraction for function bodies.
//! Implements docs/03-graph-data-model.md §5 exactly.
//!
//! Supported in Phase 3: Rust and C function bodies.
//! The CFG is intentionally structural (not data-flow): it shows *what order*
//! calls are reached and *under what condition*, not what values flow where.
//!
//! Output feeds into:
//! - `FunctionNode.control_flow` (stored in `extra_json`)
//! - `/api/graph/trace/:id` (Phase 6) — walks the CFG to produce ordered traces

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

pub use crate::graph::model::{
    BlockKind, BlockTransition, CallInBlock, ControlFlowBlock, ControlFlowGraph,
};
use crate::graph::model::{EdgeType, compute_edge_id};

/// Language selector for CFG extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfgLanguage {
    Rust,
    C,
}

/// Extract the CFG for a single function body.
///
/// `caller_node_id` is the stable node ID of the enclosing function
/// (used for edge ID computation).
/// `source` is the full file source.
/// `function_source_offset` is the byte offset of the function node in the
/// file (so line numbers in the function body are absolute).
pub fn extract_cfg(
    language: CfgLanguage,
    function_body_source: &str,
    caller_node_id: &str,
) -> Result<ControlFlowGraph> {
    let mut extractor = CfgExtractor::new(caller_node_id);
    extractor.extract(language, function_body_source)?;
    Ok(extractor.finish())
}

/// Find the first compound-statement or block child of a node.
/// Free function (not a method) to avoid borrow conflicts when called
/// before a mutable `self` borrow.
fn find_compound_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "block" | "block_expression" | "compound_statement") {
            return Some(child);
        }
    }
    None
}

struct CfgExtractor<'a> {
    caller_node_id: &'a str,
    blocks: Vec<ControlFlowBlock>,
    next_block_id: usize,
    entry_block_id: Option<String>,
    call_order: i32,
}

impl<'a> CfgExtractor<'a> {
    fn new(caller_node_id: &'a str) -> Self {
        Self {
            caller_node_id,
            blocks: Vec::new(),
            next_block_id: 0,
            entry_block_id: None,
            call_order: 0,
        }
    }

    fn new_block_id(&mut self) -> String {
        let id = format!("blk_{}", self.next_block_id);
        self.next_block_id += 1;
        id
    }

    fn push_block(&mut self, block: ControlFlowBlock) {
        if self.entry_block_id.is_none() {
            self.entry_block_id = Some(block.id.clone());
        }
        self.blocks.push(block);
    }

    fn finish(self) -> ControlFlowGraph {
        ControlFlowGraph {
            entry_block_id: self.entry_block_id.unwrap_or_else(|| "blk_0".to_owned()),
            blocks: self.blocks,
        }
    }

    fn extract(&mut self, language: CfgLanguage, body_source: &str) -> Result<()> {
        let mut parser = Parser::new();
        match language {
            CfgLanguage::Rust => {
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .context("failed to set rust grammar")?;
            }
            CfgLanguage::C => {
                parser
                    .set_language(&tree_sitter_c::LANGUAGE.into())
                    .context("failed to set c grammar")?;
            }
        }

        let tree = parser
            .parse(body_source, None)
            .context("failed to parse function body")?;

        let root = tree.root_node();
        // The root is the function body block (compound_statement / block)
        let entry_id = self.new_block_id();
        // Mark entry *before* visiting children, so nested handler blocks
        // (pushed during visit_stmt → handle_if etc.) don't steal the entry slot.
        self.entry_block_id = Some(entry_id.clone());
        let mut seq_block = ControlFlowBlock {
            id: entry_id.clone(),
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        self.visit_block_children(root, &mut seq_block, body_source, language);
        self.push_block(seq_block);
        Ok(())
    }

    fn visit_block_children(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_stmt(child, current_block, source, lang);
        }
    }

    fn visit_stmt(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        match node.kind() {
            // Unwrap expression_statement to process the inner expression
            // (Rust/C both wrap statements in expression_statement)
            "expression_statement" => {
                if let Some(inner) = node.named_child(0) {
                    self.visit_stmt(inner, current_block, source, lang);
                }
            }
            "if_expression" | "if_statement" => {
                self.handle_if(node, current_block, source, lang);
            }
            "match_expression" => {
                self.handle_match(node, current_block, source, lang);
            }
            "loop_expression"
            | "while_expression"
            | "for_expression"
            | "while_statement"
            | "for_statement"
            | "do_statement" => {
                self.handle_loop(node, current_block, source, lang);
            }
            // (covered by if_statement above)
            "switch_statement" => {
                self.handle_switch(node, current_block, source, lang);
            }
            "return_expression" | "return_statement" => {
                let call_refs = self.extract_calls_from_expr(node, source);
                for (called_name, line) in call_refs {
                    self.add_call(current_block, &called_name, line);
                }
                current_block.kind = BlockKind::Return;
            }
            _ => {
                if matches!(node.kind(), "block" | "block_expression" | "compound_statement") {
                    self.visit_block_children(node, current_block, source, lang);
                } else {
                    let calls = self.extract_calls_from_expr(node, source);
                    for (called_name, line) in calls {
                        self.add_call(current_block, &called_name, line);
                    }
                }
            }
        }
    }

    /// Wire the exit blocks of a body subgraph to a merge block.
    /// Exit blocks are those with empty `next_blocks` — they are the terminal
    /// blocks of the body's subgraph.  For simple sequential bodies this is
    /// the body block itself; for nested control flow it is the inner merge
    /// block(s).
    fn wire_exits_to_merge(
        &mut self,
        body_block: &mut ControlFlowBlock,
        before_body_len: usize,
        merge_id: &str,
    ) {
        if body_block.next_blocks.is_empty() {
            body_block.next_blocks.push(BlockTransition {
                block_id: merge_id.to_owned(),
                condition: None,
            });
        }
        for b in &mut self.blocks[before_body_len..] {
            if b.next_blocks.is_empty() {
                b.next_blocks.push(BlockTransition {
                    block_id: merge_id.to_owned(),
                    condition: None,
                });
            }
        }
    }

    fn handle_if(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        let condition_text = self.extract_condition_text(node, source);

        let then_id = self.new_block_id();
        let mut then_block = ControlFlowBlock {
            id: then_id.clone(),
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        // Visit the then-body — resolve body node before any mutable borrow
        let then_body = node.child_by_field_name("consequence")
            .or_else(|| find_compound_child(node));
        let before_then = self.blocks.len();
        if let Some(body) = then_body {
            self.visit_block_children(body, &mut then_block, source, lang);
        }

        let else_id = self.new_block_id();
        let mut else_block = ControlFlowBlock {
            id: else_id.clone(),
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        let before_else = self.blocks.len();
        let has_else = if let Some(alt) = node.child_by_field_name("alternative") {
            self.visit_block_children(alt, &mut else_block, source, lang);
            true
        } else {
            false
        };

        let merge_id = self.new_block_id();
        let merge_block = ControlFlowBlock {
            id: merge_id.clone(),
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        self.wire_exits_to_merge(&mut then_block, before_then, &merge_id);
        self.wire_exits_to_merge(&mut else_block, before_else, &merge_id);

        current_block.kind = BlockKind::Branch;
        current_block.next_blocks.push(BlockTransition {
            block_id: then_id,
            condition: Some(condition_text.clone()),
        });
        let else_cond = if has_else {
            Some(format!("else (not ({condition_text}))"))
        } else {
            Some(format!("skip (not ({condition_text}))"))
        };
        current_block.next_blocks.push(BlockTransition {
            block_id: else_id,
            condition: else_cond,
        });

        self.push_block(then_block);
        self.push_block(else_block);
        self.push_block(merge_block);
    }

    fn handle_match(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        let value_text = node
            .child_by_field_name("value")
            .map(|n| self.node_text(n, source).to_owned())
            .unwrap_or_else(|| "?".to_owned());

        let merge_id = self.new_block_id();
        current_block.kind = BlockKind::Branch;

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for arm in body.children(&mut cursor) {
                if arm.kind() != "match_arm" {
                    continue;
                }
                let pattern = arm
                    .child_by_field_name("pattern")
                    .map(|n| self.node_text(n, source).to_owned())
                    .unwrap_or_else(|| "_".to_owned());

                let arm_id = self.new_block_id();
                let mut arm_block = ControlFlowBlock {
                    id: arm_id.clone(),
                    kind: BlockKind::Sequential,
                    calls_in_block: Vec::new(),
                    next_blocks: vec![BlockTransition {
                        block_id: merge_id.clone(),
                        condition: None,
                    }],
                };

                if let Some(value) = arm.child_by_field_name("value") {
                    self.visit_stmt(value, &mut arm_block, source, lang);
                }

                current_block.next_blocks.push(BlockTransition {
                    block_id: arm_id,
                    condition: Some(format!("match {value_text} => {pattern}")),
                });
                self.push_block(arm_block);
            }
        }

        // Push completed current_block as a branch block, then repurpose
        // it as the merge block so subsequent statements go into merge.
        let old_calls = std::mem::take(&mut current_block.calls_in_block);
        let old_next = std::mem::take(&mut current_block.next_blocks);
        let old_kind = current_block.kind.clone();
        let old_id = current_block.id.clone();
        self.push_block(ControlFlowBlock {
            id: old_id,
            kind: old_kind,
            calls_in_block: old_calls,
            next_blocks: old_next,
        });
        current_block.id = merge_id;
        current_block.kind = BlockKind::Sequential;
    }

    fn handle_loop(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        let loop_id = self.new_block_id();
        let mut loop_block = ControlFlowBlock {
            id: loop_id.clone(),
            kind: BlockKind::Loop,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        let condition = match node.kind() {
            "while_expression" | "while_statement" => node
                .child_by_field_name("condition")
                .map(|n| format!("while ({})", self.node_text(n, source))),
            "for_expression" | "for_statement" => node
                .child_by_field_name("pattern")
                .zip(node.child_by_field_name("value"))
                .map(|(p, v)| {
                    format!(
                        "for {} in {}",
                        self.node_text(p, source),
                        self.node_text(v, source)
                    )
                }),
            "loop_expression" => Some("loop".to_owned()),
            _ => None,
        };

        let body = node.child_by_field_name("body")
            .or_else(|| find_compound_child(node));
        if let Some(body) = body {
            self.visit_block_children(body, &mut loop_block, source, lang);
        }

        let after_id = self.new_block_id();
        let after_block = ControlFlowBlock {
            id: after_id.clone(),
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };

        loop_block.next_blocks.push(BlockTransition {
            block_id: after_id.clone(),
            condition: condition.map(|c| format!("exit: not ({c})")),
        });

        current_block.next_blocks.push(BlockTransition {
            block_id: loop_id,
            condition: None,
        });

        self.push_block(loop_block);
        self.push_block(after_block);
    }

    fn handle_switch(
        &mut self,
        node: Node,
        current_block: &mut ControlFlowBlock,
        source: &str,
        lang: CfgLanguage,
    ) {
        let _cond = node
            .child_by_field_name("condition")
            .map(|n| self.node_text(n, source).to_owned())
            .unwrap_or_else(|| "?".to_owned());

        let merge_id = self.new_block_id();
        current_block.kind = BlockKind::Branch;

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            let mut case_blocks: Vec<(String, ControlFlowBlock)> = Vec::new();

            for child in body.children(&mut cursor) {
                match child.kind() {
                    "case_statement" => {
                        let current_case_label = child
                            .child_by_field_name("value")
                            .map(|n| self.node_text(n, source).to_owned())
                            .unwrap_or_else(|| "?".to_owned());
                        let id = self.new_block_id();
                        case_blocks.push((current_case_label.clone(), ControlFlowBlock {
                            id,
                            kind: BlockKind::Sequential,
                            calls_in_block: Vec::new(),
                            next_blocks: Vec::new(),
                        }));
                    }
                    _ => {
                        if let Some((_, ref mut blk)) = case_blocks.last_mut() {
                            self.visit_stmt(child, blk, source, lang);
                        }
                    }
                }
            }

            for i in 0..case_blocks.len() {
                let next_id = if i + 1 < case_blocks.len() {
                    case_blocks[i + 1].1.id.clone()
                } else {
                    merge_id.clone()
                };
                case_blocks[i].1.next_blocks.push(BlockTransition {
                    block_id: next_id,
                    condition: None,
                });
            }

            for (label, blk) in &case_blocks {
                current_block.next_blocks.push(BlockTransition {
                    block_id: blk.id.clone(),
                    condition: Some(format!("case {label}")),
                });
            }

            for (_, blk) in case_blocks {
                self.push_block(blk);
            }
        }

        let merge_block = ControlFlowBlock {
            id: merge_id,
            kind: BlockKind::Sequential,
            calls_in_block: Vec::new(),
            next_blocks: Vec::new(),
        };
        self.push_block(merge_block);
    }

    fn extract_calls_from_expr(&self, node: Node, source: &str) -> Vec<(String, u32)> {
        let mut calls = Vec::new();
        self.collect_calls(node, source, &mut calls);
        calls
    }

    fn collect_calls(&self, node: Node, source: &str, out: &mut Vec<(String, u32)>) {
        match node.kind() {
            "call_expression" => {
                let name = node
                    .child_by_field_name("function")
                    .map(|f| self.extract_callee_name(f, source))
                    .unwrap_or_default();
                if !name.is_empty() {
                    out.push((name, node.start_position().row as u32 + 1));
                }
                if let Some(func) = node.child_by_field_name("function") {
                    let mut cursor = func.walk();
                    for child in func.children(&mut cursor) {
                        self.collect_calls(child, source, out);
                    }
                }
                if let Some(args) = node.child_by_field_name("arguments") {
                    let mut cursor = args.walk();
                    for child in args.children(&mut cursor) {
                        self.collect_calls(child, source, out);
                    }
                }
            }
            "macro_invocation" => {
                let name = node
                    .child_by_field_name("macro")
                    .map(|m| format!("{}!", self.node_text(m, source)))
                    .unwrap_or_default();
                if !name.is_empty() {
                    out.push((name, node.start_position().row as u32 + 1));
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.collect_calls(child, source, out);
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.collect_calls(child, source, out);
                }
            }
        }
    }

    fn extract_callee_name(&self, node: Node, source: &str) -> String {
        match node.kind() {
            "identifier" => self.node_text(node, source).to_owned(),
            "scoped_identifier" | "qualified_identifier" => {
                self.node_text(node, source).to_owned()
            }
            "field_expression" | "member_expression" => node
                .child_by_field_name("field")
                .or_else(|| node.child_by_field_name("property"))
                .map(|f| self.node_text(f, source).to_owned())
                .unwrap_or_else(|| self.node_text(node, source).to_owned()),
            _ => self.node_text(node, source).to_owned(),
        }
    }

    fn add_call(&mut self, block: &mut ControlFlowBlock, called_name: &str, _line: u32) {
        let edge_id = compute_edge_id(
            self.caller_node_id,
            called_name, // used as a proxy — real to_id resolved later
            &EdgeType::Calls,
            Some(self.call_order),
        );
        block.calls_in_block.push(CallInBlock {
            edge_id,
            order_hint: self.call_order,
        });
        self.call_order += 1;
    }

    fn node_text<'s>(&self, node: Node, source: &'s str) -> &'s str {
        node.utf8_text(source.as_bytes()).unwrap_or("")
    }

    fn extract_condition_text(&self, node: Node, source: &str) -> String {
        node.child_by_field_name("condition")
            .or_else(|| node.child_by_field_name("value"))
            .map(|n| self.node_text(n, source).to_owned())
            .unwrap_or_else(|| "?".to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub label: String,
    pub blocks: Vec<String>,
}

/// Walk a `ControlFlowGraph` from its entry block and return all possible
/// paths to terminal (no-outgoing) blocks.  Loop cycles are broken at the
/// second visit to prevent infinite expansion.
///
/// This is the `/api/graph/trace/:id` primitive (data model §5 / 05-backend-schema.md §6).
pub fn walk_trace(cfg: &ControlFlowGraph) -> Vec<TracePath> {
    let mut paths: Vec<TracePath> = Vec::new();
    let mut stack: Vec<(/* block_id */ String, /* visited */ Vec<String>, /* conditions */ Vec<String>)> = Vec::new();

    let block_map: std::collections::HashMap<&str, &ControlFlowBlock> = cfg
        .blocks
        .iter()
        .map(|b| (b.id.as_str(), b))
        .collect();

    let entry = match block_map.get(cfg.entry_block_id.as_str()) {
        Some(b) => b,
        None => return paths,
    };

    stack.push((entry.id.clone(), Vec::new(), Vec::new()));

    while let Some((current_id, mut visited, conditions)) = stack.pop() {
        // Cycle guard: skip if we've seen this block on this path
        if visited.contains(&current_id) {
            continue;
        }
        visited.push(current_id.clone());

        let block = match block_map.get(current_id.as_str()) {
            Some(b) => b,
            None => continue,
        };

        if block.next_blocks.is_empty() {
            let label = if conditions.is_empty() {
                String::new()
            } else {
                conditions.join(" → ")
            };
            paths.push(TracePath { label, blocks: visited });
        } else {
            for transition in block.next_blocks.iter().rev() {
                let mut next_conditions = conditions.clone();
                if let Some(ref cond) = transition.condition {
                    next_conditions.push(cond.clone());
                }
                stack.push((transition.block_id.clone(), visited.clone(), next_conditions));
            }
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust_cfg(body: &str) -> ControlFlowGraph {
        extract_cfg(CfgLanguage::Rust, body, "test_caller_id").unwrap()
    }

    fn c_cfg(body: &str) -> ControlFlowGraph {
        extract_cfg(CfgLanguage::C, body, "test_caller_id").unwrap()
    }

    #[test]
    fn sequential_calls_in_one_block() {
        let cfg = rust_cfg("{ foo(); bar(); }");
        assert!(!cfg.blocks.is_empty());
        let entry = cfg.blocks.iter().find(|b| b.id == cfg.entry_block_id).unwrap();
        assert_eq!(entry.calls_in_block.len(), 2);
        // Order hints must be ascending
        assert!(entry.calls_in_block[0].order_hint < entry.calls_in_block[1].order_hint);
    }

    #[test]
    fn if_produces_branch_block_with_two_paths() {
        let cfg = rust_cfg("{ if x { foo(); } else { bar(); } }");
        let branch = cfg.blocks.iter().find(|b| b.kind == BlockKind::Branch);
        assert!(branch.is_some(), "expected a branch block");
        let branch = branch.unwrap();
        assert_eq!(branch.next_blocks.len(), 2, "branch must have 2 outgoing paths");
        assert!(branch.next_blocks.iter().all(|t| t.condition.is_some()));
    }

    #[test]
    fn if_without_else_still_has_two_paths() {
        let cfg = rust_cfg("{ if x { foo(); } }");
        let branch = cfg.blocks.iter().find(|b| b.kind == BlockKind::Branch);
        assert!(branch.is_some());
        assert_eq!(branch.unwrap().next_blocks.len(), 2);
    }

    #[test]
    fn loop_produces_loop_block() {
        let cfg = rust_cfg("{ loop { work(); } }");
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Loop));
    }

    #[test]
    fn while_loop_produces_loop_block() {
        let cfg = rust_cfg("{ while running { step(); } }");
        assert!(cfg.blocks.iter().any(|b| b.kind == BlockKind::Loop));
    }

    #[test]
    fn match_produces_branch_with_correct_arm_count() {
        let cfg = rust_cfg(r#"{ match x { 1 => a(), 2 => b(), _ => c(), } }"#);
        let branch = cfg.blocks.iter().find(|b| b.kind == BlockKind::Branch);
        assert!(branch.is_some(), "expected branch for match");
        assert_eq!(
            branch.unwrap().next_blocks.len(),
            3,
            "match with 3 arms should have 3 branch targets"
        );
    }

    #[test]
    fn entry_block_id_exists_in_blocks() {
        let cfg = rust_cfg("{ foo(); }");
        assert!(
            cfg.blocks.iter().any(|b| b.id == cfg.entry_block_id),
            "entry_block_id must refer to a real block"
        );
    }

    #[test]
    fn c_if_produces_branch() {
        let cfg = c_cfg("{ if (x > 0) { foo(); } else { bar(); } }");
        let branch = cfg.blocks.iter().find(|b| b.kind == BlockKind::Branch);
        assert!(branch.is_some(), "expected branch block for C if");
        assert_eq!(branch.unwrap().next_blocks.len(), 2);
    }

    #[test]
    fn c_sequential_calls() {
        let cfg = c_cfg("{ foo(); bar(); baz(); }");
        let entry = cfg.blocks.iter().find(|b| b.id == cfg.entry_block_id).unwrap();
        assert_eq!(entry.calls_in_block.len(), 3);
    }

    #[test]
    fn nested_if_produces_multiple_branch_blocks() {
        let cfg = rust_cfg("{ if a { if b { c(); } } }");
        let branch_count = cfg.blocks.iter().filter(|b| b.kind == BlockKind::Branch).count();
        assert!(branch_count >= 2, "nested if should produce ≥2 branch blocks, got {branch_count}");
    }

    #[test]
    fn trace_if_produces_two_labeled_paths() {
        let cfg = rust_cfg("{ if x { foo(); } else { bar(); } }");
        let paths = walk_trace(&cfg);

        assert_eq!(
            paths.len(),
            2,
            "if-else must produce exactly 2 trace paths, got {}",
            paths.len()
        );

        // One path should have the condition text "x" and the other
        // "else (not (x))" (or similar) — the then-branch uses the raw
        // condition expression text, the else/skip branch wraps it.
        let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("x") && !l.contains("not")),
            "expected a path with the condition 'x', got {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.contains("not (x)") || l.contains("else")),
            "expected a path with 'else' or 'not (x)' condition, got {labels:?}"
        );

        // Each path should have at least 2 blocks (branch → merge, plus the then/else body)
        for p in &paths {
            assert!(p.blocks.len() >= 2, "each trace path should have ≥2 blocks, got {:?}", p.blocks);
        }
    }

    #[test]
    fn trace_if_without_else_produces_two_paths() {
        let cfg = rust_cfg("{ if x { foo(); } }");
        let paths = walk_trace(&cfg);

        assert_eq!(
            paths.len(),
            2,
            "bare if must produce 2 paths (taken + skip), got {}",
            paths.len()
        );

        let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("skip")),
            "bare if must have a 'skip' path, got {labels:?}"
        );
    }

    #[test]
    fn trace_sequential_produces_single_path() {
        let cfg = rust_cfg("{ foo(); bar(); }");
        let paths = walk_trace(&cfg);
        assert_eq!(paths.len(), 1, "sequential body must produce 1 path, got {}", paths.len());
    }

    #[test]
    fn trace_c_if_produces_two_paths() {
        let cfg = c_cfg("{ if (x > 0) { foo(); } else { bar(); } }");
        let paths = walk_trace(&cfg);
        assert_eq!(paths.len(), 2, "C if-else must produce 2 paths, got {}", paths.len());
        let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
        assert!(
            labels.iter().any(|l| l.contains("x > 0") && !l.contains("not")),
            "C if path should contain 'x > 0', got {labels:?}"
        );
    }

    #[test]
    fn trace_loop_produces_at_least_one_path() {
        let cfg = rust_cfg("{ loop { work(); } }");
        let paths = walk_trace(&cfg);
        assert!(!paths.is_empty(), "loop should produce at least one trace path");
        for p in &paths {
            if !p.label.is_empty() {
                assert!(
                    p.label.contains("exit") || p.label.contains("not"),
                    "loop exit condition should mention 'exit' or 'not', got '{}'",
                    p.label
                );
            }
        }
    }
}
