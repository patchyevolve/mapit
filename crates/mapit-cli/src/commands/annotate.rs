use std::path::Path;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
    tasks::{self, BatchFlagFlawsOutput, BatchSummarizeOutput, SummarizeOutput},
};
use mapit_core::{
    config::{load_global_config, load_project_config, GlobalConfig},
    graph::{
        model::{AiSummaryStatus, Node},
        store::GraphStore,
    },
};

pub async fn run(target: &Path, all: bool, force: bool, skip_flaws: bool) -> Result<()> {
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

    // Determine which nodes to annotate
    let all_nodes = store.search_nodes_by_name("")?;
    let function_nodes: Vec<Node> = all_nodes
        .into_iter()
        .filter(|n| matches!(n, Node::Function(_)))
        .filter(|n| {
            if force {
                return true;
            }
            match &n.base().ai_summary_status {
                AiSummaryStatus::Pending => true,
                AiSummaryStatus::Ready => all,
                AiSummaryStatus::Unavailable => all,
            }
        })
        .collect();

    if function_nodes.is_empty() {
        println!("No functions need annotation.");
        return Ok(());
    }

    println!(
        "AI enrichment with provider: {} (model: {})",
        provider.id(),
        model
    );
    println!(
        "  {} functions across {} files to annotate (batch=file, flaws={})",
        function_nodes.len(),
        count_files(&function_nodes),
        if skip_flaws { "off" } else { "on" },
    );

    // ── Phase 0: Project-level overview ────────────────────────────────
    let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for node in &function_nodes {
        if let Some(fp) = node.base().file_path.as_deref() {
            for depth in 0..3 {
                if let Some(idx) = fp.match_indices('/').nth(depth).map(|(i, _)| i) {
                    dirs.insert(fp[..idx].to_owned());
                }
            }
        }
    }
    let file_tree: Vec<String> = dirs.iter().map(|d| format!("  {d}/")).collect();
    let entry_points: Vec<String> = function_nodes.iter()
        .filter(|n| matches!(n, Node::Function(f) if f.is_entry_point_candidate && !f.has_incoming_calls))
        .map(|n| format!("  {} ({})", n.base().name, n.base().file_path.as_deref().unwrap_or("")))
        .collect();
    let public_symbols: Vec<String> = function_nodes.iter()
        .filter(|n| !n.base().name.starts_with('_'))
        .map(|n| format!("  {} ({})", n.base().name, n.base().file_path.as_deref().unwrap_or("")))
        .take(50)
        .collect();
    let project_overview = match tasks::summarize_project(
        provider.as_ref(),
        model,
        &file_tree.join("\n"),
        &if entry_points.is_empty() { "  (none found)".into() } else { entry_points.join("\n") },
        &public_symbols.join("\n"),
    ) {
        Ok(out) => {
            println!("  Project overview: {}", out.overview);
            Some(out.overview)
        }
        Err(e) => {
            eprintln!("  Warning: project overview failed (continuing without): {e}");
            None
        }
    };

    // ── Pass 1: Batch summarize by file ────────────────────────────────
    let mut by_file: std::collections::HashMap<String, Vec<&Node>> = std::collections::HashMap::new();
    for node in &function_nodes {
        let fp = node.base().file_path.as_deref().unwrap_or("unknown").to_owned();
        by_file.entry(fp).or_default().push(node);
    }

    let pb = ProgressBar::new(function_nodes.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let mut annotated_count = 0u64;
    let mut failed_count = 0u64;

    for (file_path, nodes_in_file) in &by_file {
        let language = nodes_in_file[0].base().language.as_deref().unwrap_or("");
        let mut descs = Vec::new();
        let mut name_to_node: std::collections::HashMap<&str, &Node> = std::collections::HashMap::new();
        for (idx, node) in nodes_in_file.iter().enumerate() {
            let base = node.base();
            let sig = match node { Node::Function(f) => &f.signature, _ => "" };
            let callers = get_caller_names(&store, &base.id);
            let callees = get_callee_names(&store, &base.id);
            let sl = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
            let el = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
            let caller_context: Vec<String> = callers.iter().filter_map(|cname| {
                let cid = store.search_nodes_by_name(cname).ok()?.into_iter().find(|n| n.base().name == *cname)?.base().id.clone();
                store.get_node(&cid).ok().flatten().and_then(|n| n.base().ai_summary.clone()).map(|s| format!("{cname}: {s}"))
            }).collect();
            let caller_context_str = if caller_context.is_empty() {
                String::new()
            } else {
                format!("\n   Caller context:\n     {}", caller_context.join("\n     "))
            };
            let src = read_source_snippet(target, base.file_path.as_deref().unwrap_or(""), sl, el);
            let src_lines: Vec<&str> = src.lines().collect();
            let src_short = if src_lines.len() > 15 {
                format!("{}\n   (... truncated, {}+ lines total)", src_lines[..15].join("\n"), src_lines.len())
            } else {
                src.to_owned()
            };
            descs.push(format!(
                "{idx}. Function: `{}`\n   Signature: {}\n   Lines: {sl}-{el}\n   Callers: {}\n   Callees: {}{caller_context_str}\n   Code:\n   ```{language}\n{src_short}\n   ```",
                base.name, sig, callers.join(", "), callees.join(", "),
            ));
            name_to_node.entry(&base.name).or_insert(node);
        }

        pb.set_message(format!("{file_path} ({} funcs)", nodes_in_file.len()));

        // Batch summarize
        match tasks::summarize_batch(
            provider.as_ref(),
            model,
            file_path,
            language,
            project_overview.as_deref(),
            &descs,
        ) {
            Ok(BatchSummarizeOutput { summaries }) => {
                let mut applied = std::collections::HashSet::new();
                for entry in &summaries {
                    if let Some(node) = name_to_node.get(entry.name.as_str()) {
                        let mut updated = (*node).clone();
                        updated.base_mut().ai_summary = Some(entry.summary.clone());
                        updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
                        updated.base_mut().ai_model_used = Some(format!("{}/{}", provider.id(), model));
                        if let Err(e) = store.upsert_node(&updated) {
                            eprintln!("Failed to save batch annotation for {}: {e}", entry.name);
                            failed_count += 1;
                        } else {
                            annotated_count += 1;
                        }
                        applied.insert(entry.name.as_str());
                    }
                }
                // Mark any functions the AI skipped as Unavailable
                for (name, node) in &name_to_node {
                    if !applied.contains(name) {
                        let mut updated = (*node).clone();
                        updated.base_mut().ai_summary_status = AiSummaryStatus::Unavailable;
                        let _ = store.upsert_node(&updated);
                        failed_count += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("Batch summarize failed for {file_path}: {e}");
                for node in nodes_in_file {
                    let mut updated = (*node).clone();
                    updated.base_mut().ai_summary_status = AiSummaryStatus::Unavailable;
                    let _ = store.upsert_node(&updated);
                    failed_count += nodes_in_file.len() as u64;
                }
                pb.inc(nodes_in_file.len() as u64);
                continue;
            }
        }

        // ── Optional: Batch flaw flagging by file ────────────────────
        if !skip_flaws {
            let mut flaw_descs = Vec::new();
            for node in nodes_in_file {
                let base = node.base();
                let callers = get_caller_names(&store, &base.id);
                let callees = get_callee_names(&store, &base.id);
                let sig = match node { Node::Function(f) => &f.signature, _ => "" };
                let sl = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
                let el = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
                let lang = base.language.as_deref().unwrap_or("");
                let src = read_source_snippet(target, base.file_path.as_deref().unwrap_or(""), sl, el);
                let has_incoming = match node { Node::Function(f) => f.has_incoming_calls, _ => false };
                let is_entry = match node { Node::Function(f) => f.is_entry_point_candidate, _ => false };
                flaw_descs.push(format!(
                    r#"--- Function: `{}` ---
Signature: {sig}
Lines: {sl}-{el}
has_incoming_calls: {has_incoming}
is_entry_point_candidate: {is_entry}
Callers: {}
Callees: {}
Source code:
```{lang}
{src}
```"#,
                    base.name, callers.join(", "), callees.join(", "),
                ));
            }
            match tasks::flag_flaws_batch(
                provider.as_ref(),
                model,
                file_path,
                language,
                project_overview.as_deref(),
                &flaw_descs,
            ) {
                Ok(BatchFlagFlawsOutput { flaws }) => {
                    for node in nodes_in_file {
                        let base = node.base();
                        let is_dc_candidate = mapit_core::graph::model::is_dead_code_candidate(node);
                        let func_flaws = flaws.get(&base.name).map(|v| v.as_slice()).unwrap_or(&[]);
                        for (flaw_idx, flaw) in func_flaws.iter().enumerate() {
                            let kind = match flaw.kind.as_str() {
                                "dead_code" => mapit_core::graph::model::FlawKind::DeadCode,
                                "circular_dependency" => mapit_core::graph::model::FlawKind::CircularDependency,
                                "structural_smell" => mapit_core::graph::model::FlawKind::StructuralSmell,
                                "suspected_bug" => mapit_core::graph::model::FlawKind::SuspectedBug,
                                "missing_error_handling" => mapit_core::graph::model::FlawKind::MissingErrorHandling,
                                "resource_leak_pattern" => mapit_core::graph::model::FlawKind::ResourceLeakPattern,
                                _ => mapit_core::graph::model::FlawKind::StructuralSmell,
                            };
                            if kind == mapit_core::graph::model::FlawKind::DeadCode && !is_dc_candidate { continue; }
                            let flaw_flag = mapit_core::graph::model::FlawFlag {
                                id: format!("flaw_{}_{}", base.id, flaw_idx),
                                kind,
                                severity: match flaw.severity.as_str() {
                                    "info" => mapit_core::graph::model::FlawSeverity::Info,
                                    "warning" => mapit_core::graph::model::FlawSeverity::Warning,
                                    "high" => mapit_core::graph::model::FlawSeverity::High,
                                    _ => mapit_core::graph::model::FlawSeverity::Warning,
                                },
                                description: flaw.description.clone(),
                                confidence: flaw.confidence,
                                basis: match flaw.basis.as_str() {
                                    "structural" => mapit_core::graph::model::FlawBasis::Structural,
                                    "ai" => mapit_core::graph::model::FlawBasis::Ai,
                                    "structural+ai" => mapit_core::graph::model::FlawBasis::StructuralPlusAi,
                                    _ => mapit_core::graph::model::FlawBasis::Structural,
                                },
                                related_node_ids: None,
                            };
                            if let Err(e) = store.upsert_flaw(&flaw_flag, &base.id) {
                                eprintln!("Failed to persist flaw for {}: {e}", base.name);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Batch flaw-flagging failed for {file_path}: {e}");
                }
            }
        }

        pb.inc(nodes_in_file.len() as u64);
    }

    // ── Phase 2: File-level summarization ──────────────────────────────
    let all_annotated = store.get_all_nodes()?;
    let mut file_children: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for n in &all_annotated {
        if let Some(fp) = n.base().file_path.as_deref() {
            if matches!(n, Node::Function(_)) && n.base().ai_summary_status == AiSummaryStatus::Ready {
                let b = n.base();
                file_children.entry(fp.to_owned()).or_default().push(
                    format!("  - {} (function): {}", b.name, b.ai_summary.as_deref().unwrap_or("(no summary)"))
                );
            }
        }
    }
    let file_nodes: Vec<&Node> = all_annotated.iter()
        .filter(|n| n.base().node_type == mapit_core::graph::model::NodeType::File)
        .filter(|n| file_children.contains_key(n.base().file_path.as_deref().unwrap_or("")))
        .filter(|n| {
            if force { return true; }
            n.base().ai_summary_status != AiSummaryStatus::Ready
        })
        .collect();
    if !file_nodes.is_empty() {
        let pb2 = ProgressBar::new(file_nodes.len() as u64);
        pb2.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (file summaries)")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        for file_node in &file_nodes {
            let base = file_node.base();
            let fp = base.file_path.as_deref().unwrap_or("");
            let language = base.language.as_deref().unwrap_or("");
            let symbol_summaries = file_children.get(fp).cloned().unwrap_or_default();
            pb2.inc(1);
            if symbol_summaries.is_empty() { continue; }
            match tasks::summarize_file(provider.as_ref(), model, fp, language, &symbol_summaries) {
                Ok(SummarizeOutput { summary }) => {
                    let mut updated = (*file_node).clone();
                    updated.base_mut().ai_summary = Some(summary);
                    updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
                    updated.base_mut().ai_model_used = Some(format!("{}/{}", provider.id(), model));
                    if let Err(e) = store.upsert_node(&updated) {
                        pb2.println(format!("Failed to save file summary for {}: {e}", base.name));
                    }
                }
                Err(e) => {
                    pb2.println(format!("File summarize failed for {}: {e}", base.name));
                }
            }
        }
        pb2.finish_and_clear();
    }

    pb.finish_and_clear();
    println!(
        "✓ AI enrichment complete: {} annotated, {} failed",
        annotated_count, failed_count
    );
    Ok(())
}

fn count_files(nodes: &[Node]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for n in nodes {
        if let Some(fp) = n.base().file_path.as_deref() {
            seen.insert(fp);
        }
    }
    seen.len()
}

/// Read source code for a function from disk (line range, 0-indexed stored as 1-indexed).
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
            if start >= end {
                return String::new();
            }
            lines[start..end].join("\n")
        }
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", full_path.display());
            String::new()
        }
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

fn get_caller_names(store: &GraphStore, node_id: &str) -> Vec<String> {
    match store.edges_to(node_id) {
        Ok(edges) => edges
            .iter()
            .filter(|e| matches!(e.edge_type, mapit_core::graph::model::EdgeType::Calls))
            .filter_map(|e| {
                let n = store.get_node(&e.from_id).ok().flatten()?;
                let name = n.base().name.clone();
                match &e.condition {
                    Some(cond) if !cond.is_empty() => Some(format!("{name} (if {cond})")),
                    _ => Some(name),
                }
            })
            .collect(),
        Err(_) => vec![],
    }
}

fn get_callee_names(store: &GraphStore, node_id: &str) -> Vec<String> {
    match store.edges_from(node_id) {
        Ok(edges) => edges
            .iter()
            .filter(|e| matches!(e.edge_type, mapit_core::graph::model::EdgeType::Calls))
            .filter_map(|e| {
                let n = store.get_node(&e.to_id).ok().flatten()?;
                let name = n.base().name.clone();
                match &e.condition {
                    Some(cond) if !cond.is_empty() => Some(format!("{name} (if {cond})")),
                    _ => Some(name),
                }
            })
            .collect(),
        Err(_) => vec![],
    }
}
