use std::path::Path;
use anyhow::Result;
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
    tasks::{self, SimulateOutput},
};
use mapit_core::{
    config::{load_global_config, load_project_config, GlobalConfig},
    graph::{
        model::{self, EdgeType, Node},
        store::GraphStore,
    },
};

pub async fn run(
    target: &Path,
    name: &str,
    level: &str,
) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` first.");
        return Ok(());
    }

    let global_cfg = load_global_config(&mapit_core::config::global_config_dir())
        .unwrap_or_default();
    let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
    let provider = create_provider(&global_cfg, &project_cfg)?;
    let model = project_cfg
        .model_override
        .as_deref()
        .unwrap_or(&global_cfg.default_model);

    let store = GraphStore::open(&db_path)?;

    let context = build_context(&store, target, name, level)?;
    let overview = get_project_overview(&store);

    println!("Simulating {level} `{name}`...");

    let result = tasks::simulate(
        provider.as_ref(),
        model,
        level,
        name,
        overview.as_deref(),
        &context,
    )?;

    print_simulation(&result, level);
    Ok(())
}

fn build_context(store: &GraphStore, target: &Path, name: &str, level: &str) -> Result<String> {
    match level {
        "function" => build_function_context(store, target, name),
        "file" => build_file_context(store, target, name),
        "module" => build_module_context(store, target, name),
        "project" => build_project_context(store),
        _ => anyhow::bail!("Unknown level '{level}'. Use: function, file, module, or project."),
    }
}

fn build_function_context(store: &GraphStore, target: &Path, name: &str) -> Result<String> {
    let nodes = store.search_nodes_by_name(name)?;
    let node = nodes.iter().find(|n| n.base().name == name)
        .or_else(|| nodes.first())
        .ok_or_else(|| anyhow::anyhow!("No function found matching '{name}'"))?;

    let base = node.base();
    let signature = match node {
        Node::Function(f) => &f.signature,
        _ => "",
    };
    let start_line = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
    let end_line = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
    let language = base.language.as_deref().unwrap_or("");
    let source_text = read_source_snippet(target, base.file_path.as_deref().unwrap_or(""), start_line, end_line);

    let callers = get_caller_names_with_summaries(store, &base.id);
    let callees = get_callee_names_with_summaries(store, &base.id);
    let has_incoming = match node { Node::Function(f) => f.has_incoming_calls, _ => false };
    let is_entry = match node { Node::Function(f) => f.is_entry_point_candidate, _ => false };
    let summary = base.ai_summary.as_deref().unwrap_or("(no summary)");

    Ok(format!(
        r#"Type: function
File: {file_path}:{sl}-{el}
Language: {lang}
Signature: {sig}
Summary: {summary}
has_incoming_calls: {has_incoming}
is_entry_point_candidate: {is_entry}

Source code:
```{lang}
{src}
```

Callers (with summaries):
{callers}

Callees (with summaries):
{callees}"#,
        file_path = base.file_path.as_deref().unwrap_or("?"),
        sl = start_line, el = end_line,
        lang = language, sig = signature,
        summary = summary,
        has_incoming = has_incoming,
        is_entry = is_entry,
        src = source_text,
        callers = callers,
        callees = callees,
    ))
}

fn build_file_context(store: &GraphStore, target: &Path, name: &str) -> Result<String> {
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
        "Type: file\nFile: {file_path}\nLanguage: {lang}\nFunctions in this file ({count}):\n",
        lang = language, count = funcs.len(),
    );
    for f in &funcs {
        let base = f.base();
        let sig = match f { Node::Function(g) => &g.signature, _ => "" };
        let sl = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
        let el = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
        let summary = base.ai_summary.as_deref().unwrap_or("(no summary)");
        let callers = get_caller_names_with_summaries(store, &base.id);
        let callees = get_callee_names_with_summaries(store, &base.id);
        let src = read_source_snippet(target, file_path, sl, el);
        ctx.push_str(&format!(
            "\n--- {name} (line {sl}-{el}) ---\nSignature: {sig}\nSummary: {summary}\nCallers: {callers}\nCallees: {callees}\nCode:\n```{lang}\n{src}\n```\n",
            name = base.name, lang = language, sig = sig, summary = summary, src = src, callers = callers, callees = callees,
        ));
    }
    Ok(ctx)
}

fn build_module_context(store: &GraphStore, _target: &Path, name: &str) -> Result<String> {
    let all = store.get_all_nodes()?;
    // Normalize: if name doesn't end with /, add it for matching
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

fn build_project_context(store: &GraphStore) -> Result<String> {
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
        "Type: project\nFiles: {file_count}\nFunctions: {func_count}\nCall-edges: {edge_count}\nEntry points: {entry_count}\nLanguages: {langs}\n",
        file_count = files.len(),
        func_count = funcs.len(),
        edge_count = call_edges,
        entry_count = entries.len(),
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

fn get_project_overview(store: &GraphStore) -> Option<String> {
    let all = store.get_all_nodes().ok()?;
    let first_file = all.iter().find(|n| n.base().node_type == model::NodeType::File)?;
    first_file.base().ai_summary.clone()
}

fn print_simulation(result: &SimulateOutput, level: &str) {
    println!("\n═══ {} Simulation ═══", level.to_uppercase());
    println!("{}", result.summary);
    println!("\n━━ Entry ━━\n{}", result.entry);
    if !result.inputs.is_empty() {
        println!("\n━━ Inputs ━━");
        for i in &result.inputs {
            println!("  • {} ({}):", i.name, i.io_type);
            println!("    From user: {}", i.from_user);
            println!("    From system: {}", i.from_system);
        }
    }
    if !result.steps.is_empty() {
        println!("\n━━ Steps ━━");
        for s in &result.steps {
            println!("  {}. {} — {}", s.order, s.action, s.detail);
        }
    }
    if !result.outputs.is_empty() {
        println!("\n━━ Outputs ━━");
        for o in &result.outputs {
            println!("  • {} ({}):", o.name, o.io_type);
            if !o.to_user.is_empty() {
                println!("    To user: {}", o.to_user);
            }
            if !o.to_system.is_empty() {
                println!("    To system: {}", o.to_system);
            }
            if !o.side_effects.is_empty() {
                println!("    Side effects: {}", o.side_effects);
            }
        }
    }
    println!("\n━━ Exit ━━\n{}", result.exit);
    if !result.errors.is_empty() {
        println!("\n━━ Error Paths ━━");
        for e in &result.errors {
            println!("  • If {}: {}", e.condition, e.result);
        }
    }
    println!();
}

fn read_source_snippet(project_root: &Path, file_path: &str, start_line: u32, end_line: u32) -> String {
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
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", full_path.display());
            String::new()
        }
    }
}

fn get_caller_names_with_summaries(store: &GraphStore, node_id: &str) -> String {
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

fn get_callee_names_with_summaries(store: &GraphStore, node_id: &str) -> String {
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

fn create_provider(
    global: &GlobalConfig,
    project: &mapit_core::config::ProjectConfig,
) -> Result<Box<dyn AiProvider>> {
    let provider_name = project
        .provider_override
        .as_deref()
        .unwrap_or(&global.default_provider);

    match provider_name {
        "ollama" => Ok(Box::new(OllamaProvider {
            base_url: global.ollama_base_url.clone(),
        })),
        "openai-compatible" => {
            let config_dir = mapit_core::config::global_config_dir();
            let creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();
            let entry = creds.providers.get("openai-compatible");
            let base_url = entry.map(|c| c.base_url.clone()).unwrap_or_default();
            let api_key = entry.map(|c| c.api_key.clone()).unwrap_or_default();
            Ok(Box::new(OpenAiCompatibleProvider {
                base_url,
                api_key,
                model: global.default_model.clone(),
            }))
        }
        other => anyhow::bail!("Unknown provider '{other}'. Use 'ollama' or 'openai-compatible'."),
    }
}
