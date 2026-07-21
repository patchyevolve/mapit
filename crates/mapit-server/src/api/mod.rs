use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::state::AppState;
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
    tasks::{self, BatchFlagFlawsOutput, BatchSummarizeOutput, SummarizeOutput},
};
use mapit_core::graph::model::Node;
use mapit_core::{
    config::{load_global_config, load_project_config, save_project_config, GlobalConfig},
    graph::{
        builder::{self, FileInput, ParseResult},
        incremental::{diff_manifest, load_manifest, rebuild_manifest_from_store, save_manifest},
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
        .route(
            "/api/config",
            get(get_config_handler).put(put_config_handler),
        )
        .route("/api/config/test-connection", post(test_connection_handler))
        .route("/api/config/test-chat", post(test_chat_handler))
        .route("/api/remap", post(remap_handler))
        .route("/api/annotate", post(annotate_handler))
        .route("/api/annotate/cancel", post(cancel_annotate_handler))
        .route("/api/ask", post(ask_handler))
        .route("/api/simulate", post(simulate_handler))
        .route("/api/source", get(source_handler))
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
    /// If true, skip the flaw-flagging AI pass entirely.
    /// This saves ~1 AI call per function at the cost of losing flaw flags.
    skip_flaws: Option<bool>,
}

#[derive(Deserialize)]
struct AskBody {
    question: String,
}

#[derive(Deserialize)]
struct TestChatBody {
    message: Option<String>,
}

#[derive(Deserialize)]
struct SourceQuery {
    file: String,
    start: Option<u32>,
    end: Option<u32>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn project_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    match store.get_node(&id) {
        Ok(Some(node)) => Ok(Json(serialize_node(&node))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "node_not_found", "id": id })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

async fn neighbors_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<NeighborsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let node = store
        .get_node(&id)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "node_not_found", "id": id })),
            )
        })?;

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
                let calls: Vec<Value> = block
                    .map(|b| {
                        b.calls_in_block
                            .iter()
                            .filter_map(|c| {
                                let edge = store.get_edge(&c.edge_id).ok().flatten()?;
                                store.get_node(&edge.to_id).ok().flatten().map(|n| {
                            json!({ "node": serialize_node(&n), "order_hint": c.order_hint })
                        })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
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
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("lock poisoned: {}", e) })),
        )
    })?;
    let all = store.get_all_edges().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let edges: Vec<Value> = all.iter().map(|e| json!(e)).collect();
    Ok(Json(json!({ "edges": edges })))
}

async fn features_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let all_nodes = store.get_all_nodes().unwrap_or_default();
    let features: Vec<Value> = all_nodes
        .iter()
        .filter_map(|n| {
            if matches!(n, Node::Feature(_)) {
                Some(serialize_node(n))
            } else {
                None
            }
        })
        .collect();
    Ok(Json(json!({ "features": features })))
}

async fn flaws_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FlawsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let flaws = store.query_flaws(q.severity.as_deref()).unwrap_or_default();
    let items: Vec<Value> = flaws
        .iter()
        .map(|(flaw, name, file_path, primary_node_id)| {
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
        })
        .collect();
    Ok(Json(json!({ "flaws": items })))
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.store.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let limit = q.limit.unwrap_or(50).min(200) as usize;
    let results = store.search_nodes_by_name(&q.q).unwrap_or_default();
    let items: Vec<Value> = results
        .iter()
        .take(limit)
        .map(|node| {
            json!({
                "node": serialize_node(node),
                "match_reason": "name"
            })
        })
        .collect();
    Ok(Json(json!({ "results": items })))
}

async fn get_config_handler(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_dir = mapit_core::config::global_config_dir();
    let global = mapit_core::config::load_global_config(&config_dir).unwrap_or_default();
    let creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();
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
    let mut global = mapit_core::config::load_global_config(&config_dir).unwrap_or_default();
    if let Some(ref p) = body.provider {
        if p != "ollama" && p != "openai-compatible" {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    json!({ "error": format!("Unknown provider '{p}'. Use 'ollama' or 'openai-compatible'.") }),
                ),
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
    mapit_core::config::save_global_config(&config_dir, &global).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    if let Some(key) = body.api_key {
        let provider_name = body
            .provider
            .as_deref()
            .unwrap_or(&global.default_provider);
        let url = body
            .base_url
            .clone()
            .unwrap_or_else(|| global.ollama_base_url.clone());
        let model = body
            .model
            .clone()
            .unwrap_or_else(|| global.default_model.clone());
        let mut creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();
        creds.providers.insert(
            provider_name.to_owned(),
            mapit_core::config::ProviderCredential {
                base_url: url,
                api_key: key,
                model,
            },
        );
        mapit_core::config::save_credentials(&config_dir, &creds).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
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
    let mode = if body.force.unwrap_or(false) {
        "full"
    } else {
        "incremental"
    };
    let force = body.force.unwrap_or(false);
    let target = state.project_root.clone();
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let ws_tx = state.ws_tx.clone();

    if let Err(e) = ws_tx.send(
        json!({
            "event": "map_progress",
            "phase": "structural",
            "current": 0,
            "total": 0,
        })
        .to_string(),
    ) {
        warn!("WS broadcast (initial progress): {e}");
    }

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

        if let Err(e) = ws_tx.send(json!({
            "event": "map_progress",
            "phase": "structural",
            "current": 0,
            "total": source_files.len(),
        }).to_string()) {
            warn!("WS broadcast (reset progress): {e}");
        }

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
                if let Err(e) = ws_tx.send(json!({
                    "event": "map_progress",
                    "phase": "structural",
                    "current": current,
                    "total": source_files.len(),
                    "current_file": sf.relative_path,
                }).to_string()) {
                    warn!("WS broadcast (unchanged file progress): {e}");
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

            if let Err(e) = ws_tx.send(json!({
                "event": "map_progress",
                "phase": "structural",
                "current": current,
                "total": source_files.len(),
                "current_file": sf.relative_path,
            }).to_string()) {
                warn!("WS broadcast (processing file progress): {e}");
            }
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

        if let Err(e) = ws_tx.send(json!({
            "event": "map_phase_complete",
            "phase": "structural",
            "total": source_files.len(),
        }).to_string()) {
            warn!("WS broadcast (phase complete): {e}");
        }

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
    let cancel_flag = state.cancel_flag.clone();

    // Reset cancel flag for fresh run
    cancel_flag.store(false, Ordering::SeqCst);

    let handle = tokio::task::spawn_blocking(move || {
        let store = GraphStore::open(&db_path)?;

        let global_cfg =
            load_global_config(&mapit_core::config::global_config_dir()).unwrap_or_default();
        let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();

        let provider = create_provider(&global_cfg, &project_cfg)?;

        let model = project_cfg
            .model_override
            .as_deref()
            .unwrap_or(&global_cfg.default_model);

        // Use get_all_nodes (LIMIT 10000) not search_nodes_by_name (LIMIT 50)
        let all_nodes = store.get_all_nodes()?;
        let function_nodes: Vec<Node> = all_nodes
            .into_iter()
            .filter(|n| matches!(n, Node::Function(_)))
            .filter(|n| {
                if force {
                    return true;
                }
                match &n.base().ai_summary_status {
                    AiSummaryStatus::Pending => true,     // always annotate pending
                    AiSummaryStatus::Ready => all,        // only re-annotate ready if --all
                    AiSummaryStatus::Unavailable => true, // retry unavailable ones (provider may now work)
                }
            })
            .collect();

        let _ = ws_tx.send(
            json!({
                "event": "map_progress",
                "phase": "ai_enrichment",
                "current": 0,
                "total": function_nodes.len(),
            })
            .to_string(),
        );

        let skip_flaws = body.skip_flaws.unwrap_or(false);

        // ── Phase 0: Project-level overview ────────────────────────────
        // One cheap AI call to understand the whole system before summarizing
        // individual functions. The overview is injected into every batch prompt
        // so each function summary understands its role in the larger system.
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
                info!("Project overview: {}", out.overview);
                Some(out.overview)
            }
            Err(e) => {
                warn!("Project overview failed (continuing without): {e}");
                None
            }
        };

        // ── Pass 1: Batch summarize by file ────────────────────────────
        // Group function nodes by file_path so we can call the AI once per file
        let mut by_file: std::collections::HashMap<&str, Vec<&Node>> = std::collections::HashMap::new();
        for node in &function_nodes {
            let fp = node.base().file_path.as_deref().unwrap_or("unknown");
            by_file.entry(fp).or_default().push(node);
        }

        // Sort files by dependency depth (callee files first, caller files last).
        // This way, when we annotate a file, its callees' summaries already exist.
        let mut func_to_file: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for node in &function_nodes {
            func_to_file.insert(&node.base().id, node.base().file_path.as_deref().unwrap_or("unknown"));
        }
        let mut file_callee_set: std::collections::HashMap<&str, std::collections::HashSet<&str>> = std::collections::HashMap::new();
        for (&fp, nodes) in &by_file {
            let mut callee_fps = std::collections::HashSet::new();
            for node in nodes {
                if let Ok(edges) = store.edges_from(&node.base().id) {
                    for edge in &edges {
                        if matches!(edge.edge_type, model::EdgeType::Calls) {
                            if let Some(&cfp) = func_to_file.get(edge.to_id.as_str()) {
                                if cfp != fp { callee_fps.insert(cfp); }
                            }
                        }
                    }
                }
            }
            file_callee_set.insert(fp, callee_fps);
        }
        fn compute_file_depth<'a>(
            fp: &'a str,
            callee_map: &std::collections::HashMap<&'a str, std::collections::HashSet<&'a str>>,
            memo: &mut std::collections::HashMap<&'a str, usize>,
        ) -> usize {
            if let Some(&d) = memo.get(fp) { return d; }
            // Insert with 0 before recursing to break circular dependencies
            memo.insert(fp, 0);
            let mut max_d = 0usize;
            if let Some(callees) = callee_map.get(fp) {
                for callee in callees {
                    let d = compute_file_depth(callee, callee_map, memo) + 1;
                    max_d = max_d.max(d);
                }
            }
            memo.insert(fp, max_d);
            max_d
        }
        let all_fps: Vec<&str> = by_file.keys().copied().collect();
        let mut depth_memo: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for fp in &all_fps { compute_file_depth(fp, &file_callee_set, &mut depth_memo); }
        let mut sorted_fps: Vec<&&str> = all_fps.iter().collect();
        sorted_fps.sort_by_key(|fp| depth_memo.get(*fp).copied().unwrap_or(0));

        let mut processed_count = 0usize;

        for file_path in sorted_fps {
            let nodes_in_file = &by_file[file_path];
            if cancel_flag.load(Ordering::SeqCst) {
                info!("Annotation cancelled by user");
                let _ = ws_tx.send(
                    json!({"event": "map_phase_complete", "phase": "ai_enrichment", "total": function_nodes.len()}).to_string(),
                );
                return anyhow::Ok(0);
            }

            // Build per-function descriptions with source code (first 15 lines) and caller context
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
                // Include caller summaries when already annotated (cross-file context)
                let caller_context: Vec<String> = callers.iter().filter_map(|cname| {
                    let cid = store.search_nodes_by_name(cname).ok()?.into_iter().find(|n| n.base().name == *cname)?.base().id.clone();
                    store.get_node(&cid).ok().flatten().and_then(|n| n.base().ai_summary.clone()).map(|s| format!("{cname}: {s}"))
                }).collect();
                let caller_context_str = if caller_context.is_empty() {
                    String::new()
                } else {
                    format!("\n   Caller context:\n     {}", caller_context.join("\n     "))
                };
                let src = read_source_snippet(&target, base.file_path.as_deref().unwrap_or(""), sl, el);
                descs.push(format!(
                    "{idx}. Function: `{}`\n   Signature: {}\n   Lines: {sl}-{el}\n   Callers: {}\n   Callees: {}{caller_context_str}\n   Code:\n   ```{language}\n{src}\n   ```",
                    base.name, sig, callers.join(", "), callees.join(", "),
                ));
                name_to_node.entry(&base.name).or_insert(node);
            }

            let _ = ws_tx.send(
                json!({
                    "event": "map_progress",
                    "phase": "ai_enrichment",
                    "current": processed_count,
                    "total": function_nodes.len(),
                    "current_file": file_path,
                    "current_symbol": format!("{} functions", nodes_in_file.len()),
                }).to_string(),
            );

            // Batch summarize all functions in this file
            let batch_result = tasks::summarize_batch(
                provider.as_ref(),
                model,
                file_path,
                language,
                project_overview.as_deref(),
                &descs,
            );

            match batch_result {
                Ok(BatchSummarizeOutput { summaries }) => {
                    let mut applied = std::collections::HashSet::new();
                    for entry in &summaries {
                        if let Some(node) = name_to_node.get(entry.name.as_str()) {
                            let mut updated = (*node).clone();
                            updated.base_mut().ai_summary = Some(entry.summary.clone());
                            updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
                            updated.base_mut().ai_model_used = Some(format!("{}/{}", provider.id(), model));
                            if let Err(e) = store.upsert_node(&updated) {
                                error!("Failed to save batch annotation for {}: {e}", entry.name);
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
                        }
                    }
                }
                Err(e) => {
                    // Batch failed — mark ALL functions in this file Unavailable
                    let err_msg = format!("Batch summarize failed for {file_path}: {e}");
                    error!("{err_msg}");
                    for node in nodes_in_file {
                        let mut updated = (*node).clone();
                        updated.base_mut().ai_summary_status = AiSummaryStatus::Unavailable;
                        let _ = store.upsert_node(&updated);
                    }
                    let _ = ws_tx.send(
                        json!({
                            "event": "error",
                            "scope": "ai_call",
                            "message": err_msg,
                            "detail": "Check Settings → API Connection or try a different model/provider."
                        }).to_string(),
                    );
                    // Continue to next file rather than aborting the whole run
                    processed_count += nodes_in_file.len();
                    continue;
                }
            }

            // ── Optional: Batch flaw flagging by file ──────────────────
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
                    let src = read_source_snippet(&target, base.file_path.as_deref().unwrap_or(""), sl, el);
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
                let flaw_result = tasks::flag_flaws_batch(
                    provider.as_ref(),
                    model,
                    file_path,
                    language,
                    project_overview.as_deref(),
                    &flaw_descs,
                );
                match flaw_result {
                    Ok(BatchFlagFlawsOutput { flaws }) => {
                        for node in nodes_in_file {
                            let base = node.base();
                            let is_dc_candidate = model::is_dead_code_candidate(node);
                            let func_flaws = flaws.get(&base.name).map(|v| v.as_slice()).unwrap_or(&[]);
                            for (flaw_idx, flaw) in func_flaws.iter().enumerate() {
                                let kind = match flaw.kind.as_str() {
                                    "dead_code" => FlawKind::DeadCode,
                                    "circular_dependency" => FlawKind::CircularDependency,
                                    "structural_smell" => FlawKind::StructuralSmell,
                                    "suspected_bug" => FlawKind::SuspectedBug,
                                    "missing_error_handling" => FlawKind::MissingErrorHandling,
                                    "resource_leak_pattern" => FlawKind::ResourceLeakPattern,
                                    _ => FlawKind::StructuralSmell,
                                };
                                if kind == FlawKind::DeadCode && !is_dc_candidate { continue; }
                                let flaw_flag = FlawFlag {
                                    id: format!("flaw_{}_{}", base.id, flaw_idx),
                                    kind,
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
                    }
                    Err(e) => {
                        error!("Batch flaw-flagging failed for {file_path}: {e}");
                    }
                }
            }

            processed_count += nodes_in_file.len();
        }

        // ── 2. File-level summarization ──────────────────────────────────────
        let all_annotated = store.get_all_nodes()?;
        let mut file_children: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for n in &all_annotated {
            if let Some(fp) = n.base().file_path.as_deref() {
                if matches!(n, Node::Function(_)) && n.base().ai_summary_status == AiSummaryStatus::Ready {
                    let b = n.base();
                    let entry = format!("  - {} (function): {}",
                        b.name, b.ai_summary.as_deref().unwrap_or("(no summary)"));
                    file_children.entry(fp.to_owned()).or_default().push(entry);
                }
            }
        }
        let file_nodes: Vec<&Node> = all_annotated
            .iter()
            .filter(|n| n.base().node_type == model::NodeType::File)
            .filter(|n| file_children.contains_key(n.base().file_path.as_deref().unwrap_or("")))
            .filter(|n| {
                if force { return true; }
                n.base().ai_summary_status != AiSummaryStatus::Ready
            })
            .collect();

        for (fi, file_node) in file_nodes.iter().enumerate() {
            if cancel_flag.load(Ordering::SeqCst) { return anyhow::Ok(function_nodes.len()); }

            let base = file_node.base();
            let fp = base.file_path.as_deref().unwrap_or("");
            let language = base.language.as_deref().unwrap_or("");
            let symbol_summaries = file_children.get(fp).cloned().unwrap_or_default();
            if symbol_summaries.is_empty() { continue; }

            let _ = ws_tx.send(json!({
                "event": "map_progress",
                "phase": "ai_enrichment",
                "current": fi + 1,
                "total": file_nodes.len(),
                "current_file": base.file_path,
                "current_symbol": format!("[file] {}", base.name),
            }).to_string());

            match tasks::summarize_file(
                provider.as_ref(),
                model,
                base.file_path.as_deref().unwrap_or(""),
                language,
                &symbol_summaries,
            ) {
                Ok(SummarizeOutput { summary }) => {
                    let mut updated = (*file_node).clone();
                    updated.base_mut().ai_summary = Some(summary);
                    updated.base_mut().ai_summary_status = AiSummaryStatus::Ready;
                    updated.base_mut().ai_model_used =
                        Some(format!("{}/{}", provider.id(), model));
                    if let Err(e) = store.upsert_node(&updated) {
                        error!("Failed to save file summary for {}: {e}", base.name);
                    }
                }
                Err(e) => {
                    error!("AI file summarize failed for {}: {e}", base.name);
                }
            }
        }

        let _ = ws_tx.send(
            json!({
                "event": "map_phase_complete",
                "phase": "ai_enrichment",
                "total": function_nodes.len(),
            })
            .to_string(),
        );

        anyhow::Ok(function_nodes.len())
    });

    // Fire-and-forget — return 202 immediately; progress arrives via WebSocket.
    // Errors in the background task are logged, not surfaced as HTTP errors.
    tokio::spawn(async move {
        match handle.await {
            Ok(Ok(n)) => tracing::info!("Annotation complete: {n} functions processed"),
            Ok(Err(e)) => error!("Annotation task error: {e:#}"),
            Err(e) => error!("Annotation join error: {e}"),
        }
    });

    Ok(Json(json!({
        "status": "started",
    })))
}

async fn cancel_annotate_handler(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    state.cancel_flag.store(true, Ordering::SeqCst);
    Json(json!({ "status": "cancelling" }))
}

// ---------------------------------------------------------------------------
// Simulation handler
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SimulateBody {
    name: String,
    level: Option<String>,
}

async fn simulate_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SimulateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let name = body.name.clone();
    let level = body.level.unwrap_or_else(|| "function".into());
    let db_path = state.project_root.join(".mapit/graph.sqlite");
    let target = state.project_root.clone();

    let result = tokio::task::spawn_blocking(move || {
        let store = GraphStore::open(&db_path)?;
        let global_cfg = load_global_config(&mapit_core::config::global_config_dir()).unwrap_or_default();
        let mapit_dir = target.join(".mapit");
        let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
        let provider = create_provider(&global_cfg, &project_cfg)?;
        let model = project_cfg.model_override.as_deref().unwrap_or(&global_cfg.default_model);

        let overview = String::new();
        let context = String::new();

        anyhow::Ok(json!({
            "status": "simulate_endpoint_active",
            "name": name,
            "level": level,
        }))
    }).await;

    match result {
        Ok(Ok(val)) => Ok(Json(val)),
        Ok(Err(e)) => Ok(Json(json!({"error": e.to_string()}))),
        Err(e) => Ok(Json(json!({"error": e.to_string()}))),
    }
}

async fn ask_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AskBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let (results, file_summaries) = {
        let store = state.store.lock().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
        let nodes = store.search_nodes_by_text(&body.question).unwrap_or_default();
        // Build a map of file_path → file-level summary for any file
        // that contains a matching node
        let file_paths: std::collections::HashSet<String> = nodes
            .iter()
            .filter_map(|n| n.base().file_path.clone())
            .collect();
        let mut summaries: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Ok(all) = store.get_all_nodes() {
            for n in &all {
                if n.base().node_type != model::NodeType::File { continue; }
                let fp = match n.base().file_path.as_deref() {
                    Some(fp) if file_paths.contains(fp) => fp,
                    _ => continue,
                };
                if let Some(s) = n.base().ai_summary.as_deref() {
                    summaries.insert(fp.to_owned(), s.to_owned());
                }
            }
        }
        (nodes, summaries)
    };

    let referenced: Vec<String> = results.iter().map(|n| n.id().to_string()).collect();
    let question = body.question;

    // Try to use AI
    let mapit_dir = state.project_root.join(".mapit");
    let global_cfg = load_global_config(&mapit_core::config::global_config_dir()).unwrap_or_default();
    let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
    let model = project_cfg
        .model_override
        .as_deref()
        .unwrap_or(&global_cfg.default_model);

    let make_name_fallback = || -> Json<Value> {
        let answer = if referenced.is_empty() {
            "No relevant context found in the codebase.".to_string()
        } else {
            let names: Vec<&str> = results
                .iter()
                .map(|n| n.base().name.as_str())
                .take(5)
                .collect();
            format!(
                "Found {} related symbols: {}",
                referenced.len(),
                names.join(", ")
            )
        };
        let grounding = if referenced.is_empty() {
            "no_relevant_context_found"
        } else {
            "ok"
        };
        Json(json!({
            "answer": answer,
            "referenced_node_ids": referenced,
            "grounding_status": grounding,
        }))
    };

    // Build AI context from matching nodes
    let mut context_parts: Vec<String> = Vec::new();
    for node in results.iter().take(15) {
        let base = node.base();
        let type_str = match node {
            Node::Function(_) => "function",
            _ => "symbol",
        };
        let mut part = format!(
            "Symbol: {}\n  File: {}\n  Type: {}\n",
            base.name,
            base.file_path.as_deref().unwrap_or("unknown"),
            type_str,
        );
        if let Some(s) = &base.ai_summary {
            part.push_str(&format!("  Summary: {s}\n"));
        }
        // Also include the file-level summary if available
        if let Some(fp) = base.file_path.as_deref() {
            if let Some(fs) = file_summaries.get(fp) {
                part.push_str(&format!("  File overview: {fs}\n"));
            }
        }
        if let Node::Function(f) = node {
            if f.has_incoming_calls {
                part.push_str("  Called by other functions: yes\n");
            }
            if f.is_entry_point_candidate {
                part.push_str("  Entry point candidate: yes\n");
            }
        }
        context_parts.push(part);
    }

    let context = if context_parts.is_empty() {
        "No relevant symbols found in the codebase.".to_string()
    } else {
        let count = context_parts.len();
        let body = context_parts.join("\n");
        format!(
            "The codebase has {count} symbol(s) relevant to the question:\n\n{body}"
        )
    };

    let provider = match create_provider(&global_cfg, &project_cfg) {
        Ok(p) => p,
        Err(e) => {
            warn!("AI provider not available for ask: {e:#}");
            return Ok(make_name_fallback());
        }
    };

    match tasks::answer(provider.as_ref(), &model, &context, &question) {
        Ok(output) => {
            let grounding = if output.referenced_node_ids.is_empty() && referenced.is_empty() {
                "no_relevant_context_found"
            } else {
                "ok"
            };
            let mut all_refs = output.referenced_node_ids.clone();
            for id in &referenced {
                if !all_refs.contains(id) {
                    all_refs.push(id.clone());
                }
            }
            Ok(Json(json!({
                "answer": output.answer,
                "referenced_node_ids": all_refs,
                "grounding_status": grounding,
            })))
        }
        Err(e) => {
            warn!("AI answer failed: {e:#}");
            Ok(make_name_fallback())
        }
    }
}

async fn source_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SourceQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Reject absolute paths and path traversal
    let file_path = q.file.trim_start_matches('/');
    if file_path.contains("..") || file_path.starts_with('/') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_path" })),
        ));
    }
    let abs_path = state.project_root.join(file_path);
    // Security: ensure path is inside project root after canonicalization
    let canonical_project =
        std::fs::canonicalize(&state.project_root).unwrap_or_else(|_| state.project_root.clone());
    let canonical_file = std::fs::canonicalize(&abs_path).unwrap_or_else(|_| abs_path.clone());
    if !canonical_file.starts_with(&canonical_project) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_path" })),
        ));
    }
    let content = std::fs::read_to_string(&abs_path).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": e.to_string() })),
        )
    })?;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start_idx = q
        .start
        .map(|s| (s as usize).saturating_sub(1))
        .unwrap_or(0)
        .min(total);
    let end_idx = q.end.map(|e| (e as usize).min(total)).unwrap_or(total);
    let slice = &lines[start_idx..end_idx];
    let language = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    Ok(Json(json!({
        "content": slice.join("\n"),
        "language": language,
        "start_line": start_idx + 1,
        "end_line": end_idx,
    })))
}

async fn test_connection_handler(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_dir = mapit_core::config::global_config_dir();
    let global = mapit_core::config::load_global_config(&config_dir).unwrap_or_default();
    let creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();

    let provider_id = global.default_provider.clone();
    let base_url = global.ollama_base_url.clone();
    let model = global.default_model.clone();

    let result = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let provider: Box<dyn mapit_ai::provider::AiProvider> =
            if provider_id == "openai-compatible" {
                let cred = creds
                    .providers
                    .get("openai-compatible")
                    .cloned()
                    .unwrap_or_else(|| mapit_core::config::ProviderCredential {
                        base_url: base_url,
                        api_key: String::new(),
                        model: model,
                    });
                Box::new(mapit_ai::openai_compatible::OpenAiCompatibleProvider {
                    base_url: cred.base_url,
                    api_key: cred.api_key,
                    model: cred.model,
                })
            } else {
                Box::new(mapit_ai::ollama::OllamaProvider { base_url })
            };
        match provider.list_models() {
            Ok(models) => {
                let latency = start.elapsed().as_millis() as u64;
                let names: Vec<String> = models.into_iter().map(|m| m.id).collect();
                let count = names.len();
                Ok((latency, names, count))
            }
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match result {
        Ok((latency_ms, models, count)) => Ok(Json(json!({
            "ok": true,
            "message": format!("Connected \u{00b7} {} model{} available", count, if count == 1 { "" } else { "s" }),
            "latency_ms": latency_ms,
            "models": models,
        }))),
        Err(err) => Ok(Json(json!({
            "ok": false,
            "message": err,
            "latency_ms": null,
            "models": [],
        }))),
    }
}

async fn test_chat_handler(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<TestChatBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_dir = mapit_core::config::global_config_dir();
    let global = mapit_core::config::load_global_config(&config_dir).unwrap_or_default();
    let creds = mapit_core::config::load_credentials(&config_dir).unwrap_or_default();

    let provider_id = global.default_provider.clone();
    let base_url = global.ollama_base_url.clone();
    let model = global.default_model.clone();
    let message = body
        .message
        .unwrap_or_else(|| "Respond with exactly one word: OK".to_string());

    let result = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let provider: Box<dyn mapit_ai::provider::AiProvider> =
            if provider_id == "openai-compatible" {
                let cred = creds
                    .providers
                    .get("openai-compatible")
                    .cloned()
                    .unwrap_or_else(|| mapit_core::config::ProviderCredential {
                        base_url: base_url,
                        api_key: String::new(),
                        model: model.clone(),
                    });
                Box::new(mapit_ai::openai_compatible::OpenAiCompatibleProvider {
                    base_url: cred.base_url,
                    api_key: cred.api_key,
                    model: model,
                })
            } else {
                Box::new(mapit_ai::ollama::OllamaProvider { base_url })
            };
        let request = mapit_ai::provider::AiRequest {
            model: String::new(), // empty → provider uses its own stored model name
            system_prompt: None,
            user_prompt: message,
            expect_json: false,
        };
        match provider.complete(request) {
            Ok(resp) => {
                let latency = start.elapsed().as_millis() as u64;
                Ok((latency, resp.content))
            }
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    match result {
        Ok((latency_ms, response)) => Ok(Json(json!({
            "ok": true,
            "response": response,
            "latency_ms": latency_ms,
        }))),
        Err(err) => Ok(Json(json!({
            "ok": false,
            "error": err,
            "latency_ms": null,
        }))),
    }
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
                obj.insert(
                    "is_entry_point_candidate".into(),
                    json!(f.is_entry_point_candidate),
                );
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
                obj.insert(
                    "classification_confidence".into(),
                    json!(f.classification_confidence),
                );
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
        std::fs::write(
            &mapit_gitignore,
            "# mapit metadata directory — all contents auto-generated\n*\n",
        )?;
    }
    Ok(())
}

fn create_provider(
    global: &GlobalConfig,
    project: &mapit_core::config::ProjectConfig,
) -> anyhow::Result<Box<dyn AiProvider>> {
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

fn read_source_snippet(
    project_root: &std::path::Path,
    file_path: &str,
    start_line: u32,
    end_line: u32,
) -> String {
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
