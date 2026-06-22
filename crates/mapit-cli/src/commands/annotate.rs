use std::path::Path;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
    tasks::{self, SummarizeOutput},
};
use mapit_core::{
    config::{load_global_config, load_project_config, GlobalConfig},
    graph::{
        model::{self, AiSummaryStatus, FlawBasis, FlawFlag, FlawKind, FlawSeverity, Node},
        store::GraphStore,
    },
};


pub async fn run(target: &Path, all: bool, force: bool) -> Result<()> {
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
    println!("  {} functions to annotate", function_nodes.len());

    let pb = ProgressBar::new(function_nodes.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({msg})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let mut annotated_count = 0u64;
    let mut failed_count = 0u64;

    for node in &function_nodes {
        let base = node.base();
        pb.set_message(base.name.clone());

        let callers = get_caller_names(&store, &base.id);
        let callees = get_callee_names(&store, &base.id);
        let signature = match node {
            Node::Function(f) => &f.signature,
            _ => "",
        };
        let start_line = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
        let end_line = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
        let language = base.language.as_deref().unwrap_or("");
        // Read the actual source file to give the AI code context
        let source_text = read_source_snippet(
            target,
            base.file_path.as_deref().unwrap_or(""),
            start_line,
            end_line,
        );
        let node_type = format!("{:?}", base.node_type);

        // Summarize
        match tasks::summarize(
            provider.as_ref(),
            model,
            &base.name,
            &node_type,
            base.file_path.as_deref().unwrap_or(""),
            start_line,
            end_line,
            language,
            &source_text,
            signature,
            &callers,
            &callees,
        ) {
            Ok(SummarizeOutput { summary }) => {
                let mut updated = node.clone();
                updated.base_mut().ai_summary = Some(summary);
                updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
                updated.base_mut().ai_model_used = Some(format!("{}/{}", provider.id(), model));
                if let Err(e) = store.upsert_node(&updated) {
                    eprintln!("Failed to save annotation for {}: {e}", base.name);
                    failed_count += 1;
                } else {
                    annotated_count += 1;
                }
            }
            Err(e) => {
                eprintln!("AI summarize failed for {}: {e}", base.name);
                let mut updated = node.clone();
                updated.base_mut().ai_summary_status = AiSummaryStatus::Unavailable;
                let _ = store.upsert_node(&updated);
                failed_count += 1;
            }
        }

        // Flaw-flagging for all functions (dead_code gated on structural facts
        // inside the AI prompt — the AI receives has_incoming_calls so it can
        // make the assessment per 03-graph-data-model.md §3)
        let has_incoming = match node { Node::Function(f) => f.has_incoming_calls, _ => false };
        let is_entry = match node { Node::Function(f) => f.is_entry_point_candidate, _ => false };
        match tasks::flag_flaws(
            provider.as_ref(),
            model,
            &base.name,
            base.file_path.as_deref().unwrap_or(""),
            start_line,
            end_line,
            has_incoming,
            is_entry,
            language,
            &source_text,
            signature,
            &callers,
            &callees,
        ) {
            Ok(output) => {
                for flaw in &output.flaws {
                    let flaw_flag = FlawFlag {
                        id: format!("flaw_{}", base.name),
                        kind: match flaw.kind.as_str() {
                            "dead_code" => FlawKind::DeadCode,
                            "circular_dependency" => FlawKind::CircularDependency,
                            "structural_smell" => FlawKind::StructuralSmell,
                            "suspected_bug" => FlawKind::SuspectedBug,
                            "missing_error_handling" => FlawKind::MissingErrorHandling,
                            "resource_leak_pattern" => FlawKind::ResourceLeakPattern,
                            _ => FlawKind::StructuralSmell,
                        },
                        severity: match flaw.severity.as_str() {
                            "info" => FlawSeverity::Info,
                            "warning" => FlawSeverity::Warning,
                            "high" => FlawSeverity::High,
                            _ => FlawSeverity::Warning,
                        },
                        description: flaw.description.clone(),
                        confidence: flaw.confidence,
                        basis: match flaw.basis.as_str() {
                            "structural" => FlawBasis::Structural,
                            "ai" => FlawBasis::Ai,
                            "structural+ai" => FlawBasis::StructuralPlusAi,
                            _ => FlawBasis::Structural,
                        },
                        related_node_ids: None,
                    };
                    if let Err(e) = store.upsert_flaw(&flaw_flag, &base.id) {
                        eprintln!("Failed to persist flaw for {}: {e}", base.name);
                    }
                }
            }
            Err(e) => {
                eprintln!("AI flaw-flagging failed for {}: {e}", base.name);
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();
    println!(
        "✓ AI enrichment complete: {} annotated, {} failed",
        annotated_count, failed_count
    );
    Ok(())
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
            let api_key = creds
                .providers
                .get("openai-compatible")
                .map(|c| c.api_key.clone())
                .unwrap_or_default();
            Ok(Box::new(OpenAiCompatibleProvider {
                base_url: global.ollama_base_url.clone(),
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
            .filter(|e| matches!(e.edge_type, model::EdgeType::Calls))
            .filter_map(|e| store.get_node(&e.from_id).ok().flatten())
            .map(|n| n.base().name.clone())
            .collect(),
        Err(_) => vec![],
    }
}

fn get_callee_names(store: &GraphStore, node_id: &str) -> Vec<String> {
    match store.edges_from(node_id) {
        Ok(edges) => edges
            .iter()
            .filter(|e| matches!(e.edge_type, model::EdgeType::Calls))
            .filter_map(|e| store.get_node(&e.to_id).ok().flatten())
            .map(|n| n.base().name.clone())
            .collect(),
        Err(_) => vec![],
    }
}
