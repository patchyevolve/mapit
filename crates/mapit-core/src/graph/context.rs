use std::path::Path;
use anyhow::Result;

use super::model::{self, EdgeType, Node};
use super::store::GraphStore;

pub fn build_function_context(store: &GraphStore, target: &Path, name: &str) -> Result<String> {
    let nodes = store.search_nodes_by_name(name)?;
    let node = nodes.iter().find(|n| n.base().name == name)
        .or_else(|| nodes.first())
        .ok_or_else(|| anyhow::anyhow!("No function found matching '{name}'"))?;
    let base = node.base();
    let signature = match node {
        Node::Function(f) => &f.signature,
        _ => "",
    };
    let sl = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
    let el = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
    let language = base.language.as_deref().unwrap_or("");
    let src = read_source_snippet(target, base.file_path.as_deref().unwrap_or(""), sl, el);
    let callers = get_caller_names_with_summaries(store, &base.id);
    let callees = get_callee_names_with_summaries(store, &base.id);
    let summary = base.ai_summary.as_deref().unwrap_or("(no summary)");
    let has_incoming = match node { Node::Function(f) => f.has_incoming_calls, _ => false };
    let is_entry = match node { Node::Function(f) => f.is_entry_point_candidate, _ => false };
    Ok(format!(
        r#"Type: function
File: {fp}:{sl}-{el}
Language: {language}
Signature: {sig}
Summary: {summary}
has_incoming_calls: {has_incoming}
is_entry_point_candidate: {is_entry}
Source code:
```{language}
{src}
```

Callers (with summaries):
{callers}

Callees (with summaries):
{callees}"#,
        fp = base.file_path.as_deref().unwrap_or("?"), sig = signature,
    ))
}

pub fn build_file_context(store: &GraphStore, target: &Path, name: &str) -> Result<String> {
    let all = store.get_all_nodes()?;
    let file_node = all.iter().find(|n| {
        n.base().node_type == model::NodeType::File
            && n.base().name == name
    }).ok_or_else(|| anyhow::anyhow!("No file found matching '{name}'"))?;
    let file_path = file_node.base().file_path.as_deref().unwrap_or("");
    let language = file_node.base().language.as_deref().unwrap_or("");
    let mut funcs: Vec<&Node> = all.iter()
        .filter(|n| matches!(n, Node::Function(_)) && n.base().file_path.as_deref() == Some(file_path))
        .collect();
    funcs.sort_by_key(|n| n.base().span.as_ref().map(|s| s.start_line).unwrap_or(0));
    let mut ctx = format!(
        "Type: file\nFile: {file_path}\nLanguage: {language}\nFunctions in this file ({count}):\n",
        count = funcs.len(),
    );
    for f in &funcs {
        let b = f.base();
        let sig = match f { Node::Function(g) => &g.signature, _ => "" };
        let sl = b.span.as_ref().map(|s| s.start_line).unwrap_or(0);
        let el = b.span.as_ref().map(|s| s.end_line).unwrap_or(0);
        let s = b.ai_summary.as_deref().unwrap_or("(no summary)");
        let callers = get_caller_names_with_summaries(store, &b.id);
        let callees = get_callee_names_with_summaries(store, &b.id);
        let src = read_source_snippet(target, file_path, sl, el);
        ctx.push_str(&format!(
            "\n--- {name} (line {sl}-{el}) ---\nSignature: {sig}\nSummary: {s}\nCallers: {callers}\nCallees: {callees}\nCode:\n```{language}\n{src}\n```\n",
            name = b.name,
        ));
    }
    Ok(ctx)
}

pub fn build_module_context(store: &GraphStore, _target: &Path, name: &str) -> Result<String> {
    let all = store.get_all_nodes()?;
    let prefix = if name.ends_with('/') { name.to_owned() } else { format!("{name}/") };
    let mut files: Vec<&Node> = all.iter()
        .filter(|n| n.base().node_type == model::NodeType::File)
        .filter(|n| n.base().file_path.as_deref().map_or(false, |fp| fp.starts_with(&prefix) || fp == name))
        .collect();
    files.sort_by_key(|n| n.base().name.clone());
    let mut ctx = format!(
        "Type: module (subfolder)\nModule path: {name}\nFiles in this module ({count}):\n",
        count = files.len(),
    );
    for f in &files {
        let fp = f.base().file_path.as_deref().unwrap_or("?");
        let lang = f.base().language.as_deref().unwrap_or("");
        let summary = f.base().ai_summary.as_deref().unwrap_or("(no file summary)");
        ctx.push_str(&format!("\n  {fp} ({lang}): {summary}\n"));
        let funcs: Vec<&Node> = all.iter()
            .filter(|n| matches!(n, Node::Function(_)) && n.base().file_path.as_deref() == Some(fp))
            .collect();
        for func in &funcs {
            let b = func.base();
            let s = b.ai_summary.as_deref().unwrap_or("(no summary)");
            ctx.push_str(&format!("    - {} (fn): {s}\n", b.name));
        }
    }
    Ok(ctx)
}

pub fn build_project_context(store: &GraphStore) -> Result<String> {
    let all = store.get_all_nodes()?;
    let files: Vec<&Node> = all.iter()
        .filter(|n| n.base().node_type == model::NodeType::File)
        .collect();
    let funcs: Vec<&Node> = all.iter()
        .filter(|n| matches!(n, Node::Function(_)))
        .collect();
    let entries: Vec<&Node> = funcs.iter()
        .filter(|n| matches!(n, Node::Function(f) if f.is_entry_point_candidate && !f.has_incoming_calls))
        .copied()
        .collect();
    let edges = store.get_all_edges().unwrap_or_default();
    let call_edges = edges.iter().filter(|e| matches!(e.edge_type, EdgeType::Calls)).count();
    let mut ctx = format!(
        "Type: project\nFiles: {fc}\nFunctions: {fnc}\nCall-edges: {ec}\nEntry points: {epc}\nLanguages: {langs}\n",
        fc = files.len(), fnc = funcs.len(), ec = call_edges, epc = entries.len(),
        langs = store.get_distinct_languages().unwrap_or_default().join(", "),
    );
    ctx.push_str("\nEntry points:\n");
    for e in &entries {
        let b = e.base();
        ctx.push_str(&format!("  {} ({})\n", b.name, b.file_path.as_deref().unwrap_or("?")));
    }
    ctx.push_str("\nFiles and their public functions:\n");
    for f in &files {
        let fp = f.base().file_path.as_deref().unwrap_or("?");
        let lang = f.base().language.as_deref().unwrap_or("");
        let summary = f.base().ai_summary.as_deref().unwrap_or("(no summary)");
        ctx.push_str(&format!("\n  {fp} ({lang})\n    Overview: {summary}\n"));
        let file_funcs: Vec<&Node> = funcs.iter()
            .filter(|n| n.base().file_path.as_deref() == Some(fp))
            .copied()
            .collect();
        for func in &file_funcs {
            let b = func.base();
            let s = b.ai_summary.as_deref().unwrap_or("(no summary)");
            ctx.push_str(&format!("    - {}: {s}\n", b.name));
        }
    }
    Ok(ctx)
}

pub fn get_project_overview(store: &GraphStore) -> Option<String> {
    let all = store.get_all_nodes().ok()?;
    let first_file = all.iter().find(|n| n.base().node_type == model::NodeType::File)?;
    first_file.base().ai_summary.clone()
}

pub fn get_caller_names_with_summaries(store: &GraphStore, node_id: &str) -> String {
    match store.edges_to(node_id) {
        Ok(edges) => {
            let callers: Vec<String> = edges.iter()
                .filter(|e| matches!(e.edge_type, EdgeType::Calls))
                .filter_map(|e| {
                    let node = store.get_node(&e.from_id).ok().flatten()?;
                    let s = node.base().ai_summary.as_deref().unwrap_or("(no summary)");
                    Some(format!("  - {}: {}", node.base().name, s))
                })
                .collect();
            if callers.is_empty() { "(none)".into() } else { callers.join("\n") }
        }
        Err(_) => "(none)".into(),
    }
}

pub fn get_callee_names_with_summaries(store: &GraphStore, node_id: &str) -> String {
    match store.edges_from(node_id) {
        Ok(edges) => {
            let callees: Vec<String> = edges.iter()
                .filter(|e| matches!(e.edge_type, EdgeType::Calls))
                .filter_map(|e| {
                    let node = store.get_node(&e.to_id).ok().flatten()?;
                    let s = node.base().ai_summary.as_deref().unwrap_or("(no summary)");
                    Some(format!("  - {}: {}", node.base().name, s))
                })
                .collect();
            if callees.is_empty() { "(none)".into() } else { callees.join("\n") }
        }
        Err(_) => "(none)".into(),
    }
}

pub fn read_source_snippet(project_root: &Path, file_path: &str, start_line: u32, end_line: u32) -> String {
    if file_path.is_empty() || start_line == 0 || end_line == 0 {
        return String::new();
    }
    let full_path = project_root.join(file_path);
    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = (start_line.saturating_sub(1)) as usize;
            let end = (end_line as usize).min(lines.len());
            if start >= end { return String::new(); }
            lines[start..end].join("\n")
        }
        Err(_) => String::new(),
    }
}

pub fn build_sim_context(store: &GraphStore, target: &Path, name: &str, level: &str) -> Result<String> {
    match level {
        "function" => build_function_context(store, target, name),
        "file" => build_file_context(store, target, name),
        "module" => build_module_context(store, target, name),
        "project" => build_project_context(store),
        _ => anyhow::bail!("Unknown level '{level}'. Use: function, file, module, or project."),
    }
}
