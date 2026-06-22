//! `mapit map [--force]` — structural mapping pass.

use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

use mapit_core::{
    graph::{
        builder::{self, FileInput, ParseResult},
        store::GraphStore,
    },
    languages::adapter_for_language,
    walker,
};

pub async fn run(target: &Path, force: bool) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    std::fs::create_dir_all(&mapit_dir)?;

    // Auto-add .mapit to .gitignore if not already present (TRD §7.1)
    ensure_gitignore(target)?;

    let db_path = mapit_dir.join("graph.sqlite");
    let store = GraphStore::open(&db_path)?;

    // Load project config for extra ignore patterns
    let project_cfg = mapit_core::config::load_project_config(&mapit_dir).unwrap_or_default();
    let extra_ignores = project_cfg.extra_ignore_patterns.clone();

    // Walk the directory
    println!("Scanning project structure...");
    let source_files = walker::walk(target, &extra_ignores)?;
    println!(
        "  {} source files found",
        source_files.len()
    );

    // Build file inputs, checking incremental manifest unless --force
    let mut file_inputs: Vec<FileInput> = Vec::new();
    let mut changed_count = 0usize;
    let mut unchanged_count = 0usize;

    for sf in &source_files {
        let content = match std::fs::read_to_string(&sf.path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Cannot read {}: {e} — skipping", sf.relative_path);
                file_inputs.push(FileInput {
                    relative_path: &sf.relative_path,
                    language: &sf.language,
                    size_bytes: sf.size_bytes,
                    parse_result: ParseResult::Failed {
                        error: e.to_string(),
                    },
                });
                continue;
            }
        };

        let hash = {
            let h = Sha256::digest(content.as_bytes());
            hex::encode(&h[..16])
        };

        // Check if unchanged
        if !force {
            if let Ok(Some(stored_hash)) = store.get_manifest_hash(&sf.relative_path) {
                if stored_hash == hash {
                    unchanged_count += 1;
                    file_inputs.push(FileInput {
                        relative_path: &sf.relative_path,
                        language: &sf.language,
                        size_bytes: sf.size_bytes,
                        parse_result: ParseResult::Unchanged,
                    });
                    continue;
                }
            }
        }

        // Parse the file
        changed_count += 1;
        let parse_result = match adapter_for_language(&sf.language) {
            None => {
                store.upsert_manifest_entry(
                    &sf.relative_path,
                    &hash,
                    Some(&sf.language),
                    "unsupported",
                    None,
                )?;
                ParseResult::Unsupported
            }
            Some(adapter) => match adapter.extract(&sf.relative_path, &content) {
                Ok(output) => {
                    store.upsert_manifest_entry(
                        &sf.relative_path,
                        &hash,
                        Some(&sf.language),
                        "ok",
                        None,
                    )?;
                    ParseResult::Ok(output)
                }
                Err(e) => {
                    let err_str = e.to_string();
                    error!("Parse failed for {}: {err_str}", sf.relative_path);
                    store.upsert_manifest_entry(
                        &sf.relative_path,
                        &hash,
                        Some(&sf.language),
                        "parse_failed",
                        Some(&err_str),
                    )?;
                    ParseResult::Failed { error: err_str }
                }
            },
        };

        file_inputs.push(FileInput {
            relative_path: &sf.relative_path,
            language: &sf.language,
            size_bytes: sf.size_bytes,
            parse_result,
        });
    }

    if !force && unchanged_count > 0 {
        println!(
            "  {unchanged_count} files unchanged · {changed_count} files to (re)parse"
        );
    }

    // For changed files: delete their prior nodes/edges from the store
    for fi in &file_inputs {
        match &fi.parse_result {
            ParseResult::Unchanged => continue,
            _ => {
                store.delete_edges_for_file(fi.relative_path)?;
                store.delete_nodes_for_file(fi.relative_path)?;
            }
        }
    }

    // Build the graph
    println!("Building graph...");
    let build_output = builder::build(&file_inputs)?;

    // Persist nodes and edges
    for node in &build_output.nodes {
        if let Err(e) = store.upsert_node(node) {
            error!("Failed to persist node {}: {e}", node.id());
        }
    }
    for edge in &build_output.edges {
        if let Err(e) = store.upsert_edge(edge) {
            error!("Failed to persist edge {}: {e}", edge.id);
        }
    }

    // Recompute has_incoming_calls for all function nodes
    store.recompute_incoming_calls()?;

    let node_count = store.node_count()?;
    let edge_count = store.edge_count()?;

    println!(
        "✓ Structural mapping complete\n  {} nodes · {} edges",
        node_count, edge_count
    );

    // Update project config timestamps
    let mut cfg = mapit_core::config::load_project_config(&mapit_dir).unwrap_or_default();
    cfg.last_full_map_at = Some(chrono::Utc::now().to_rfc3339());
    mapit_core::config::save_project_config(&mapit_dir, &cfg)?;

    info!("map complete: {node_count} nodes, {edge_count} edges");
    Ok(())
}

fn ensure_gitignore(project_root: &Path) -> Result<()> {
    let gi_path = project_root.join(".gitignore");
    let entry = "\n# mapit metadata\n.mapit/\n";

    if gi_path.exists() {
        let content = std::fs::read_to_string(&gi_path)?;
        if !content.contains(".mapit") {
            let mut f = std::fs::OpenOptions::new().append(true).open(&gi_path)?;
            use std::io::Write;
            f.write_all(entry.as_bytes())?;
        }
    } else {
        std::fs::write(&gi_path, format!("# mapit metadata\n.mapit/\n"))?;
    }
    Ok(())
}
