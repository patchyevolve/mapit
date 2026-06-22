//! `mapit map [--force]` — structural mapping pass with incremental support.

use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

use mapit_core::{
    graph::{
        builder::{self, FileInput, ParseResult},
        incremental::{diff_manifest, load_manifest, save_manifest, rebuild_manifest_from_store},
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
    println!("  {} source files found", source_files.len());

    // --- Incremental: load manifest.json, validate against SQLite ---
    let manifest = if force {
        // Force full re-map — ignore existing manifest
        mapit_core::graph::incremental::ManifestFile::new()
    } else {
        let json_manifest = load_manifest(&mapit_dir).unwrap_or_default();
        // Validate: if SQLite has entries and manifest is empty (e.g. crash mid-write),
        // rebuild manifest from SQLite (doc §5 mismatch recovery rule).
        let db_entry_count = store.manifest_entry_count()?;
        if db_entry_count > 0 && json_manifest.files.is_empty() {
            warn!("manifest.json is empty but SQLite has {db_entry_count} entries — rebuilding from DB");
            rebuild_manifest_from_store(&mapit_dir, &store).unwrap_or(json_manifest)
        } else {
            json_manifest
        }
    };

    // Compute current content hashes
    let mut current_hashes = std::collections::HashMap::new();
    // Also keep content strings alive for FileInput refs
    let mut contents: Vec<(String, String)> = Vec::new(); // (relative_path, content)

    for sf in &source_files {
        match std::fs::read_to_string(&sf.path) {
            Ok(c) => {
                let hash = {
                    let h = Sha256::digest(c.as_bytes());
                    hex::encode(&h[..16])
                };
                current_hashes.insert(sf.relative_path.clone(), hash);
                contents.push((sf.relative_path.clone(), c));
            }
            Err(e) => {
                warn!("Cannot read {}: {e} — skipping", sf.relative_path);
                contents.push((sf.relative_path.clone(), String::new()));
            }
        }
    }

    // Diff against stored manifest
    let diff = diff_manifest(&current_hashes, &manifest);
    let changed = mapit_core::graph::incremental::changed_count(&diff);
    let unchanged = diff.values().filter(|s| **s == mapit_core::graph::incremental::FileStatus::Unchanged).count();

    if !force && unchanged > 0 {
        println!("  {unchanged} files unchanged · {changed} files to (re)parse");
    }

    // Build content map for O(1) lookup
    let content_map: std::collections::HashMap<&str, &str> = contents
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();

    // Build FileInputs
    let mut file_inputs: Vec<FileInput> = Vec::new();
    let mut parsed_count = 0usize;
    let mut new_manifest = manifest.clone();

    for sf in &source_files {
        let status = diff.get(&sf.relative_path)
            .cloned()
            .unwrap_or(mapit_core::graph::incremental::FileStatus::Added);

        let content = content_map.get(sf.relative_path.as_str()).copied().unwrap_or("");

        if !force && status == mapit_core::graph::incremental::FileStatus::Unchanged {
            file_inputs.push(FileInput {
                relative_path: &sf.relative_path,
                language: &sf.language,
                size_bytes: sf.size_bytes,
                parse_result: ParseResult::Unchanged,
                source: None,
            });
            continue;
        }

        // Delete prior graph data for modified/added files
        store.delete_edges_for_file(&sf.relative_path)?;
        store.delete_nodes_for_file(&sf.relative_path)?;

        if content.is_empty() {
            file_inputs.push(FileInput {
                relative_path: &sf.relative_path,
                language: &sf.language,
                size_bytes: sf.size_bytes,
                parse_result: ParseResult::Failed { error: "could not read file".into() },
                source: None,
            });
            continue;
        }

        parsed_count += 1;
        let hash = current_hashes.get(&sf.relative_path).cloned().unwrap_or_default();

        let parse_result = match adapter_for_language(&sf.language) {
            None => {
                new_manifest.files.insert(
                    sf.relative_path.clone(),
                    mapit_core::graph::incremental::ManifestEntry {
                        content_hash: hash,
                        language: Some(sf.language.clone()),
                        last_parsed_at: chrono::Utc::now().to_rfc3339(),
                        parse_status: "unsupported".into(),
                    },
                );
                ParseResult::Unsupported
            }
            Some(adapter) => match adapter.extract(&sf.relative_path, content) {
                Ok(output) => {
                    new_manifest.files.insert(
                        sf.relative_path.clone(),
                        mapit_core::graph::incremental::ManifestEntry {
                            content_hash: hash.clone(),
                            language: Some(sf.language.clone()),
                            last_parsed_at: chrono::Utc::now().to_rfc3339(),
                            parse_status: "ok".into(),
                        },
                    );
                    // Keep SQLite manifest in sync too
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
                    new_manifest.files.insert(
                        sf.relative_path.clone(),
                        mapit_core::graph::incremental::ManifestEntry {
                            content_hash: hash.clone(),
                            language: Some(sf.language.clone()),
                            last_parsed_at: chrono::Utc::now().to_rfc3339(),
                            parse_status: "parse_failed".into(),
                        },
                    );
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
            source: Some(content),
        });
    }

    // Remove deleted files from manifest and store
    for (path, status) in &diff {
        if *status == mapit_core::graph::incremental::FileStatus::Deleted {
            store.delete_edges_for_file(path)?;
            store.delete_nodes_for_file(path)?;
            store.delete_manifest_entry(path)?;
            new_manifest.files.remove(path);
        }
    }

    // Build graph from parsed files
    println!("Building graph...");
    let build_output = builder::build(&file_inputs)?;

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

    store.recompute_incoming_calls()?;

    // Atomically save updated manifest.json
    save_manifest(&mapit_dir, &new_manifest)?;

    let node_count = store.node_count()?;
    let edge_count = store.edge_count()?;

    if parsed_count > 0 || force {
        println!("✓ Structural mapping complete\n  {} nodes · {} edges", node_count, edge_count);
    } else {
        println!("✓ Already up to date ({} nodes · {} edges)", node_count, edge_count);
    }

    // Update project config timestamps
    let mut cfg = mapit_core::config::load_project_config(&mapit_dir).unwrap_or_default();
    if force || cfg.last_full_map_at.is_none() {
        cfg.last_full_map_at = Some(chrono::Utc::now().to_rfc3339());
    } else {
        cfg.last_incremental_map_at = Some(chrono::Utc::now().to_rfc3339());
    }
    mapit_core::config::save_project_config(&mapit_dir, &cfg)?;

    info!("map complete: {node_count} nodes, {edge_count} edges, {parsed_count} files parsed");
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
        std::fs::write(&gi_path, "# mapit metadata\n.mapit/\n")?;
    }
    Ok(())
}
