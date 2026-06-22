use std::{
    collections::HashMap,
    io::IsTerminal,
    path::Path,
    sync::Mutex,
    time::Instant,
};

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
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

    ensure_gitignore(target)?;

    let db_path = mapit_dir.join("graph.sqlite");
    let store = GraphStore::open(&db_path)?;

    let project_cfg = mapit_core::config::load_project_config(&mapit_dir).unwrap_or_default();
    let extra_ignores = project_cfg.extra_ignore_patterns.clone();
    let start = Instant::now();

    // --- Walk ---
    println!("Scanning project structure...");
    let source_files = walker::walk(target, &extra_ignores)?;
    let mut lang_counts: HashMap<&str, u64> = HashMap::new();
    for sf in &source_files {
        *lang_counts.entry(sf.language.as_str()).or_default() += 1;
    }
    let mut lang_parts: Vec<String> = lang_counts
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    lang_parts.sort();
    println!(
        "  {} source files to analyze across {} languages: {}",
        source_files.len(),
        lang_counts.len(),
        lang_parts.join(", "),
    );

    // --- Manifest ---
    let manifest = if force {
        mapit_core::graph::incremental::ManifestFile::new()
    } else {
        let json_manifest = load_manifest(&mapit_dir).unwrap_or_default();
        let db_entry_count = store.manifest_entry_count()?;
        if db_entry_count > 0 && json_manifest.files.is_empty() {
            warn!("manifest.json is empty but SQLite has {db_entry_count} entries — rebuilding from DB");
            rebuild_manifest_from_store(&mapit_dir, &store).unwrap_or(json_manifest)
        } else {
            json_manifest
        }
    };

    // --- Hash & Diff ---
    let mut current_hashes = HashMap::new();
    let mut contents: Vec<(String, String)> = Vec::new();

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

    let diff = diff_manifest(&current_hashes, &manifest);
    let changed = mapit_core::graph::incremental::changed_count(&diff);
    let unchanged = diff
        .values()
        .filter(|s| **s == mapit_core::graph::incremental::FileStatus::Unchanged)
        .count();

    if !force && unchanged > 0 {
        println!("  {unchanged} files unchanged · {changed} files to (re)parse");
    }

    let content_map: HashMap<&str, &str> = contents
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();

    // --- Progress bar setup ---
    let use_progress = std::io::stderr().is_terminal() && !force && changed < source_files.len();
    let total_files = source_files.len() as u64;
    let pb = if use_progress {
        let pb = ProgressBar::new(total_files);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        pb.set_message("parsing...");
        Some(pb)
    } else {
        None
    };

    // --- Process files ---
    let mut file_inputs: Vec<FileInput> = Vec::new();
    let mut new_manifest = manifest.clone();
    let mut parsed_count: u64 = 0;
    let mut unsupported_count: u64 = 0;
    let mut failed_count: u64 = 0;

    // Language tracking for the progress bar "current" display
    let current_lang = Mutex::new(String::new());

    for sf in &source_files {
        let status = diff
            .get(&sf.relative_path)
            .cloned()
            .unwrap_or(mapit_core::graph::incremental::FileStatus::Added);

        let content = content_map
            .get(sf.relative_path.as_str())
            .copied()
            .unwrap_or("");

        if !force && status == mapit_core::graph::incremental::FileStatus::Unchanged {
            file_inputs.push(FileInput {
                relative_path: &sf.relative_path,
                language: &sf.language,
                size_bytes: sf.size_bytes,
                parse_result: ParseResult::Unchanged,
                source: None,
            });
            if let Some(p) = &pb {
                p.inc(1);
            }
            continue;
        }

        store.delete_edges_for_file(&sf.relative_path)?;
        store.delete_nodes_for_file(&sf.relative_path)?;

        if content.is_empty() {
            file_inputs.push(FileInput {
                relative_path: &sf.relative_path,
                language: &sf.language,
                size_bytes: sf.size_bytes,
                parse_result: ParseResult::Failed {
                    error: "could not read file".into(),
                },
                source: None,
            });
            failed_count += 1;
            if let Some(p) = &pb {
                p.inc(1);
            }
            continue;
        }

        if pb.is_some() {
            *current_lang.lock().unwrap() = sf.language.clone();
        }

        parsed_count += 1;
        let hash = current_hashes
            .get(&sf.relative_path)
            .cloned()
            .unwrap_or_default();

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
                unsupported_count += 1;
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
                    failed_count += 1;
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

        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }

    // --- Remove deleted files ---
    for (path, status) in &diff {
        if *status == mapit_core::graph::incremental::FileStatus::Deleted {
            store.delete_edges_for_file(path)?;
            store.delete_nodes_for_file(path)?;
            store.delete_manifest_entry(path)?;
            new_manifest.files.remove(path);
        }
    }

    if let Some(ref pb) = pb {
        pb.finish_and_clear();
    }

    // --- Build graph ---
    if !use_progress {
        println!("Building graph...");
    }
    let _build_start = Instant::now();
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
    save_manifest(&mapit_dir, &new_manifest)?;

    let node_count = store.node_count()?;
    let edge_count = store.edge_count()?;
    let elapsed = start.elapsed();

    if parsed_count > 0 || force {
        println!(
            "✓ Structural mapping complete in {}.{:02}s",
            elapsed.as_secs(),
            elapsed.subsec_millis() / 10,
        );
        println!(
            "  {} files parsed successfully · {} unsupported · {} failed",
            parsed_count, unsupported_count, failed_count,
        );
        println!("  {} symbols found · {} structural edges", node_count, edge_count);
    } else {
        // Use one line summary for incremental "nothing changed"
        let existing_ok = source_files.len() as u64 - unsupported_count - failed_count;
        println!(
            "✓ Already up to date · {} files · {} symbols · {} edges",
            existing_ok, node_count, edge_count,
        );
    }

    // --- Update config timestamps ---
    let mut cfg = mapit_core::config::load_project_config(&mapit_dir).unwrap_or_default();
    if force || cfg.last_full_map_at.is_none() {
        cfg.last_full_map_at = Some(chrono::Utc::now().to_rfc3339());
    } else {
        cfg.last_incremental_map_at = Some(chrono::Utc::now().to_rfc3339());
    }
    mapit_core::config::save_project_config(&mapit_dir, &cfg)?;

    info!("map complete: {node_count} nodes, {edge_count} edges, {parsed_count} files parsed in {elapsed:?}");
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
