use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, warn};

use crate::state::AppState;
use mapit_core::graph::model::Node;
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
    tasks::{self, SummarizeOutput},
};
use mapit_core::{
    config::{load_global_config, load_project_config, GlobalConfig, save_project_config},
    graph::{
        builder::{self, FileInput, ParseResult},
        incremental::{diff_manifest, load_manifest, save_manifest, rebuild_manifest_from_store},
        model::{self, AiSummaryStatus, FlawBasis, FlawFlag, FlawKind, FlawSeverity},
        store::GraphStore,
    },
    languages::adapter_for_language,
    walker,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Instant;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/project", get(project_handler))
        .route("/api/graph/node/{id}", get(node_handler))
        .route("/api/graph/neighbors/{id}", get(neighbors_handler))
        .route("/api/graph/trace/{id}", get(trace_handler))
        .route("/api/graph/nodes", get(nodes_handler))
        .route("/api/graph/edges", get(edges_handler))
        .route("/api/graph/features", get(features_handler))
        .route("/api/graph/flaws", get(flaws_handler))
        .route("/api/graph/search", get(search_handler))
        .route("/api/config", get(get_config_handler).put(put_config_handler))
        .route("/api/remap", post(remap_handler))
        .route("/api/annotate", post(annotate_handler))
        .route("/api/ask", post(ask_handler))
}

#[derive(Deserialize)]
struct NeighborsQuery {
    direction: Option<String>,
    depth: Option<u32>,
}

#[derive(Deserialize)]
struct FlawsQuery {
    severity: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct PutConfigBody {
    provider: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    extra_ignore_patterns: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RemapBody {
    force: Option<bool>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AnnotateBody {
    all: Option<bool>,
    force: Option<bool>,
}

#[derive(Deserialize)]
struct AskBody {
    question: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn project_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let file_count = store.manifest_entry_count().unwrap_or(0);
    let symbol_count = store.node_count().unwrap_or(0);
    let edge_count = store.edge_count().unwrap_or(0);
    let func_count = store.function_count().unwrap_or(0);
    let annotated = store.annotated_function_count().unwrap_or(0);
    let languages = store.get_distinct_languages().unwrap_or_default();
    let flaw_count = store.flaw_count(None).unwrap_or(0);

    let ai_coverage = if func_count > 0 {
        (annotated as f64 / func_count as f64 * 100.0 * 10.0).round() / 10.0
    } else {
        0.0
    };

    // Read project config for timestamps
    let project_cfg = mapit_core::config::load_project_config(&state.mapit_dir).unwrap_or_default();

    let config_dir = mapit_core::config::global_config_dir();
    let global = mapit_core::config::load_global_config(&config_dir).unwrap_or_default();

    Ok(Json(json!({
        "project_root": state.project_root.to_string_lossy(),
        "last_full_map_at": project_cfg.last_full_map_at,
        "last_incremental_map_at": project_cfg.last_incremental_map_at,
        "file_count": file_count,
        "symbol_count": symbol_count,
        "function_count": func_count,
        "flaw_count": flaw_count,
        "edge_count": edge_count,
        "languages": languages,
        "provider": global.default_provider,
        "model": global.default_model,
        "ai_annotation_coverage_pct": ai_coverage,
    })))
}

async fn node_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    match store.get_node(&id) {
        Ok(Some(node)) => Ok(Json(serialize_node(&node))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "node_not_found", "id": id })))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))),
    }
}

async fn neighbors_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<NeighborsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let direction = q.direction.as_deref().unwrap_or("both");
    let depth = q.depth.unwrap_or(1).min(10);

    let mut collected_nodes = Vec::new();
    let mut collected_edges = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut current = vec![id.clone()];

    for _d in 0..depth {
        let mut next = Vec::new();
        for nid in &current {
            if !visited.insert(nid.clone()) {
                continue;
            }
            if direction == "callers" || direction == "both" {
                if let Ok(edges) = store.edges_to(nid) {
                    for e in &edges {
                        if let Ok(Some(n)) = store.get_node(&e.from_id) {
                            collected_nodes.push(serialize_node(&n));
                            next.push(e.from_id.clone());
                        }
                    }
                    collected_edges.extend(edges);
                }
            }
            if direction == "callees" || direction == "both" {
                if let Ok(edges) = store.edges_from(nid) {
                    for e in &edges {
                        if let Ok(Some(n)) = store.get_node(&e.to_id) {
                            collected_nodes.push(serialize_node(&n));
                            next.push(e.to_id.clone());
                        }
                    }
                    collected_edges.extend(edges);
                }
            }
        }
        current = next;
    }

    // Add the center node
    if let Ok(Some(center)) = store.get_node(&id) {
        collected_nodes.push(serialize_node(&center));
    }

    Ok(Json(json!({
        "center_id": id,
        "nodes": collected_nodes,
        "edges": collected_edges
    })))
}

async fn trace_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(_q): Query<NeighborsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _ = &_q;
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let node = store.get_node(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "node_not_found", "id": id }))))?;

    // Check if node has CFG data
    let cfgs = if let Node::Function(f) = &node {
        f.control_flow.as_ref()
    } else {
        None
    };

    if let Some(cfg) = cfgs {
        let paths = mapit_core::control_flow::walk_trace(cfg);
        // Build a block_id -> block lookup (IDs are "blk_0", not usize indices)
        let mut block_idx = std::collections::HashMap::new();
        for (i, b) in cfg.blocks.iter().enumerate() {
            block_idx.insert(b.id.clone(), i);
        }
        let mut steps: Vec<Value> = Vec::new();
        for path in &paths {
            for block_id in &path.blocks {
                let block = block_idx.get(block_id).and_then(|i| cfg.blocks.get(*i));
                // Resolve edge_ids to full node objects for the frontend
                let calls: Vec<Value> = block.map(|b| {
                    b.calls_in_block.iter().filter_map(|c| {
                        let edge = store.get_edge(&c.edge_id).ok().flatten()?;
                        store.get_node(&edge.to_id).ok().flatten().map(|n| {
                            json!({ "node": serialize_node(&n), "order_hint": c.order_hint })
                        })
                    }).collect()
                }).unwrap_or_default();
                steps.push(json!({
                    "block_id": block_id,
                    "label": &path.label,
                    "calls": calls,
                    "branches": block.map(|b| b.next_blocks.iter().map(|t| {
                        json!({ "condition": t.condition, "next_block_id": t.block_id })
                    }).collect::<Vec<_>>()).unwrap_or_default()
                }));
            }
        }

        Ok(Json(json!({
            "entry_node_id": id,
            "steps": steps,
            "truncated_at_depth": false
        })))
    } else {
        Ok(Json(json!({
            "entry_node_id": id,
            "steps": [],
            "truncated_at_depth": false
        })))
    }
}

async fn nodes_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().unwrap();
    let filter_type = params.get("type").map(|s| s.as_str());
    let all = store.get_all_nodes().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let nodes: Vec<Value> = all
        .into_iter()
        .filter(|n| filter_type.map_or(true, |t| node_type_str(n) == t))
        .map(|n| serialize_node(&n))
        .collect();
    Ok(Json(json!({ "nodes": nodes })))
}

async fn edges_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().unwrap();
    let all = store.get_all_edges().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let edges: Vec<Value> = all.iter().map(|e| json!(e)).collect();
    Ok(Json(json!({ "edges": edges })))
}

async fn features_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let all_nodes = store.get_all_nodes().unwrap_or_default();
    let features: Vec<Value> = all_nodes.iter().filter_map(|n| {
        if matches!(n, Node::Feature(_)) {
            Some(serialize_node(n))
        } else {
            None
        }
    }).collect();
    Ok(Json(json!({ "features": features })))
}

async fn flaws_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FlawsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let flaws = store.query_flaws(q.severity.as_deref()).unwrap_or_default();
    let items: Vec<Value> = flaws.iter().map(|(flaw, name, file_path, primary_node_id)| {
        json!({
            "id": flaw.id,
            "kind": flaw.kind,
            "severity": flaw.severity,
            "description": flaw.description,
            "confidence": flaw.confidence,
            "basis": flaw.basis,
            "primary_node_id": primary_node_id,
            "primary_node_name": name,
            "file_path": file_path,
        })
    }).collect();
    Ok(Json(json!({ "flaws": items })))
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;
    let limit = q.limit.unwrap_or(50).min(200) as usize;
    let results = store.search_nodes_by_name(&q.q).unwrap_or_default();
    let items: Vec<Value> = results.iter().take(limit).map(|node| {
        json!({
            "node": serialize_node(node),
            "match_reason": "name"
        })
    }).collect();
    Ok(Json(json!({ "results": items })))
}

async fn get_config_handler(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_dir = mapit_core::config::global_config_dir();
    let global = mapit_core::config::load_global_config(&config_dir)
        .unwrap_or_default();
    let creds = mapit_core::config::load_credentials(&config_dir)
        .unwrap_or_default();
    let api_key_set = creds.providers.contains_key("openai-compatible");
    Ok(Json(json!({
        "provider": global.default_provider,
        "model": global.default_model,
        "base_url": global.ollama_base_url,
        "api_key_set": api_key_set,
        "ignore_patterns": global.default_ignore_patterns,
    })))
}

async fn put_config_handler(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<PutConfigBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_dir = mapit_core::config::global_config_dir();
    let mut global = mapit_core::config::load_global_config(&config_dir)
        .unwrap_or_default();
    if let Some(ref p) = body.provider {
        if p != "ollama" && p != "openai-compatible" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Unknown provider '{p}'. Use 'ollama' or 'openai-compatible'.") })),
            ));
        }
        global.default_provider = p.clone();
    }
    if let Some(ref m) = body.model {
        global.default_model = m.clone();
    }
    if let Some(ref u) = body.base_url {
        global.ollama_base_url = u.clone();
    }
    if let Some(patterns) = body.extra_ignore_patterns {
        global.default_ignore_patterns = patterns;
    }
    mapit_core::config::save_global_config(&config_dir, &global)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    if let Some(key) = body.api_key {
        let url = body.base_url.clone().unwrap_or_else(|| global.ollama_base_url.clone());
        let model = body.model.clone().unwrap_or_else(|| global.default_model.clone());
        let mut creds = mapit_core::config::load_credentials(&config_dir)
            .unwrap_or_default();
        creds.providers.insert(
            "openai-compatible".into(),
            mapit_core::config::ProviderCredential {
                base_url: url,
                api_key: key,
                model,
            },
        );
        mapit_core::config::save_credentials(&config_dir, &creds)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    }

    Ok(Json(json!({
        "provider": global.default_provider,
        "model": global.default_model,
        "base_url": global.ollama_base_url,
        "api_key_set": true,
    })))
}

async fn remap_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RemapBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mode = if body.force.unwrap_or(false) { "full" } else { "incremental" };
    let force = body.force.unwrap_or(false);
    let target = state.project_root.clone();
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let ws_tx = state.ws_tx.clone();

    let _ = ws_tx.send(json!({
        "event": "map_progress",
        "phase": "structural",
        "current": 0,
        "total": 0,
    }).to_string());

    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&mapit_dir)?;
        ensure_gitignore(&target)?;
        let store = GraphStore::open(&db_path)?;
        let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
        let extra_ignores = project_cfg.extra_ignore_patterns.clone();
        let _start = Instant::now();

        let source_files = walker::walk(&target, &extra_ignores)?;
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

        let manifest = if force {
            mapit_core::graph::incremental::ManifestFile::new()
        } else {
            let json_manifest = load_manifest(&mapit_dir).unwrap_or_default();
            let db_entry_count = store.manifest_entry_count().unwrap_or(0);
            if db_entry_count > 0 && json_manifest.files.is_empty() {
                warn!("manifest.json is empty but SQLite has {db_entry_count} entries — rebuilding from DB");
                rebuild_manifest_from_store(&mapit_dir, &store).unwrap_or(json_manifest)
            } else {
                json_manifest
            }
        };

        let diff = diff_manifest(&current_hashes, &manifest);
        let content_map: HashMap<&str, &str> = contents.iter().map(|(p, c)| (p.as_str(), c.as_str())).collect();

        let _ = ws_tx.send(json!({
            "event": "map_progress",
            "phase": "structural",
            "current": 0,
            "total": source_files.len(),
        }).to_string());

        let mut file_inputs: Vec<FileInput> = Vec::new();
        let mut new_manifest = manifest.clone();
        let mut current = 0usize;

        for sf in &source_files {
            current += 1;
            let status = diff
                .get(&sf.relative_path)
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
                let _ = ws_tx.send(json!({
                    "event": "map_progress",
                    "phase": "structural",
                    "current": current,
                    "total": source_files.len(),
                    "current_file": sf.relative_path,
                }).to_string());
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
                continue;
            }

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
                        store.upsert_manifest_entry(&sf.relative_path, &hash, Some(&sf.language), "ok", None)?;
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
                        store.upsert_manifest_entry(&sf.relative_path, &hash, Some(&sf.language), "parse_failed", Some(&err_str))?;
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

            let _ = ws_tx.send(json!({
                "event": "map_progress",
                "phase": "structural",
                "current": current,
                "total": source_files.len(),
                "current_file": sf.relative_path,
            }).to_string());
        }

        for (path, status) in &diff {
            if *status == mapit_core::graph::incremental::FileStatus::Deleted {
                store.delete_edges_for_file(path)?;
                store.delete_nodes_for_file(path)?;
                store.delete_manifest_entry(path)?;
                new_manifest.files.remove(path);
            }
        }

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

        let mut cfg = load_project_config(&mapit_dir).unwrap_or_default();
        if force || cfg.last_full_map_at.is_none() {
            cfg.last_full_map_at = Some(chrono::Utc::now().to_rfc3339());
        } else {
            cfg.last_incremental_map_at = Some(chrono::Utc::now().to_rfc3339());
        }
        save_project_config(&mapit_dir, &cfg)?;

        let _ = ws_tx.send(json!({
            "event": "map_phase_complete",
            "phase": "structural",
        }).to_string());

        anyhow::Ok(())
    }).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Join error: {e}") }))))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Remap error: {e}") }))))?;

    Ok(Json(json!({
        "status": "started",
        "mode": mode,
    })))
}

async fn annotate_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnnotateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let all = body.all.unwrap_or(false);
    let force = body.force.unwrap_or(false);
    let target = state.project_root.clone();
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let ws_tx = state.ws_tx.clone();

    tokio::task::spawn_blocking(move || {
        let store = GraphStore::open(&db_path)?;

        let global_cfg = load_global_config(&mapit_core::config::global_config_dir()).unwrap_or_default();
        let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
        let provider = create_provider(&global_cfg, &project_cfg)?;
        let model = project_cfg
            .model_override
            .as_deref()
            .unwrap_or(&global_cfg.default_model);

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

        let _ = ws_tx.send(json!({
            "event": "map_progress",
            "phase": "ai_enrichment",
            "current": 0,
            "total": function_nodes.len(),
        }).to_string());

        for (i, node) in function_nodes.iter().enumerate() {
            let base = node.base();
            let callers = get_caller_names(&store, &base.id);
            let callees = get_callee_names(&store, &base.id);
            let signature = match node {
                Node::Function(f) => &f.signature,
                _ => "",
            };
            let start_line = base.span.as_ref().map(|s| s.start_line).unwrap_or(0);
            let end_line = base.span.as_ref().map(|s| s.end_line).unwrap_or(0);
            let language = base.language.as_deref().unwrap_or("");
            let source_text = read_source_snippet(&target, base.file_path.as_deref().unwrap_or(""), start_line, end_line);
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
                        error!("Failed to save annotation for {}: {e}", base.name);
                    }
                }
                Err(e) => {
                    error!("AI summarize failed for {}: {e}", base.name);
                    let mut updated = node.clone();
                    updated.base_mut().ai_summary_status = AiSummaryStatus::Unavailable;
                    let _ = store.upsert_node(&updated);
                }
            }

            // Flaw flagging
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
                            error!("Failed to persist flaw for {}: {e}", base.name);
                        }
                    }
                }
                Err(e) => {
                    error!("AI flaw-flagging failed for {}: {e}", base.name);
                }
            }

            let _ = ws_tx.send(json!({
                "event": "map_progress",
                "phase": "ai_enrichment",
                "current": i + 1,
                "total": function_nodes.len(),
                "current_file": base.file_path,
            }).to_string());
        }

        let _ = ws_tx.send(json!({
            "event": "map_phase_complete",
            "phase": "ai_enrichment",
        }).to_string());

        anyhow::Ok(function_nodes.len())
    }).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Join error: {e}") }))))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Annotate error: {e}") }))))?;

    Ok(Json(json!({
        "status": "started",
    })))
}

async fn ask_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AskBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;

    // Simple search-based answer: find nodes matching the question
    let results = store.search_nodes_by_name(&body.question).unwrap_or_default();
    let referenced: Vec<String> = results.iter().map(|n| n.id().to_string()).collect();

    let answer = if referenced.is_empty() {
        "No relevant context found in the codebase.".to_string()
    } else {
        let names: Vec<&str> = results.iter().map(|n| n.base().name.as_str()).take(5).collect();
        format!("Found {} related symbols: {}", referenced.len(), names.join(", "))
    };

    let grounding = if referenced.is_empty() {
        "no_relevant_context_found"
    } else {
        "ok"
    };

    Ok(Json(json!({
        "answer": answer,
        "referenced_node_ids": referenced,
        "grounding_status": grounding,
    })))
}

// ---------------------------------------------------------------------------
// Serialization helper
// ---------------------------------------------------------------------------

fn node_type_str(node: &Node) -> &'static str {
    match node {
        Node::Feature(_) => "feature",
        Node::Module(_) => "module",
        Node::File(_) => "file",
        Node::Function(_) => "function",
        Node::External(_) => "external",
        Node::TypeNode(_) => "type",
        Node::Macro(_) => "macro",
        Node::Global(_) => "global",
    }
}

fn serialize_node(node: &Node) -> Value {
    let base = node.base();
    let mut fields = json!({
        "id": base.id,
        "name": base.name,
        "type": node_type_str(node),
        "language": base.language,
        "file_path": base.file_path,
        "span": base.span,
        "ai_summary": base.ai_summary,
        "ai_summary_status": base.ai_summary_status,
        "ai_model_used": base.ai_model_used,
        "structural_hash": base.structural_hash,
        "flaws": base.flaws,
    });
    // Merge in type-specific fields
    match node {
        Node::Function(f) => {
            if let Some(obj) = fields.as_object_mut() {
                obj.insert("signature".into(), json!(f.signature));
                obj.insert("has_incoming_calls".into(), json!(f.has_incoming_calls));
                obj.insert("is_entry_point_candidate".into(), json!(f.is_entry_point_candidate));
            }
        }
        Node::File(f) => {
            if let Some(obj) = fields.as_object_mut() {
                obj.insert("size_bytes".into(), json!(f.size_bytes));
                obj.insert("parse_status".into(), json!(f.parse_status));
                if let Some(e) = &f.parse_error {
                    obj.insert("parse_error".into(), json!(e));
                }
            }
        }
        Node::Feature(f) => {
            if let Some(obj) = fields.as_object_mut() {
                obj.insert("member_node_ids".into(), json!(f.member_node_ids));
                obj.insert("classification_confidence".into(), json!(f.classification_confidence));
            }
        }
        Node::External(e) => {
            if let Some(obj) = fields.as_object_mut() {
                obj.insert("reason".into(), json!(e.reason));
            }
        }
        Node::Module(_) | Node::TypeNode(_) | Node::Macro(_) | Node::Global(_) => {}
    }
    fields
}

fn ensure_gitignore(project_root: &std::path::Path) -> anyhow::Result<()> {
    let mapit_gitignore = project_root.join(".mapit").join(".gitignore");
    if !mapit_gitignore.exists() {
        std::fs::write(&mapit_gitignore, "# mapit metadata directory — all contents auto-generated\n*\n")?;
    }
    Ok(())
}

fn create_provider(global: &GlobalConfig, project: &mapit_core::config::ProjectConfig) -> anyhow::Result<Box<dyn AiProvider>> {
    let provider_name = project.provider_override.as_deref().unwrap_or(&global.default_provider);
    match provider_name {
        "ollama" => Ok(Box::new(OllamaProvider { base_url: global.ollama_base_url.clone() })),
        "openai-compatible" => {
            let config_dir = mapit_core::config::global_config_dir();
            let creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();
            let api_key = creds.providers.get("openai-compatible").map(|c| c.api_key.clone()).unwrap_or_default();
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

fn read_source_snippet(project_root: &std::path::Path, file_path: &str, start_line: u32, end_line: u32) -> String {
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
            warn!("Warning: could not read {}: {e}", full_path.display());
            String::new()
        }
    }
}
