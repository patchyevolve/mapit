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

use crate::state::AppState;
use mapit_core::graph::model::Node;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/project", get(project_handler))
        .route("/api/graph/node/{id}", get(node_handler))
        .route("/api/graph/neighbors/{id}", get(neighbors_handler))
        .route("/api/graph/trace/{id}", get(trace_handler))
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
        let mut steps: Vec<Value> = Vec::new();
        for path in &paths {
            for block_id in &path.blocks {
                let idx: usize = match block_id.parse() {
                    Ok(i) => i,
                    Err(_) => continue,
                };
                let block = cfg.blocks.get(idx);
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
    if let Some(p) = body.provider {
        global.default_provider = p;
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
    // Emit start event
    let _ = state.ws_tx.send(json!({
        "event": "map_progress",
        "phase": "structural",
        "current": 0,
        "total": 0,
    }).to_string());

    // Update progress
    {
        let mut p = state.progress.lock().unwrap();
        p.phase = "structural".into();
    }

    // In a real implementation, this would spawn a background task.
    // For v1, accept and report.
    let _ = state.ws_tx.send(json!({
        "event": "map_phase_complete",
        "phase": "structural",
    }).to_string());

    Ok(Json(json!({
        "status": "started",
        "mode": mode,
        "note": "Background remap not yet implemented - structural map complete"
    })))
}

async fn annotate_handler(
    State(state): State<Arc<AppState>>,
    Json(_body): Json<AnnotateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _ = state.ws_tx.send(json!({
        "event": "map_progress",
        "phase": "ai_enrichment",
        "current": 0,
        "total": 0,
    }).to_string());

    let _ = state.ws_tx.send(json!({
        "event": "map_phase_complete",
        "phase": "ai_enrichment",
    }).to_string());

    Ok(Json(json!({
        "status": "started",
        "note": "Background annotate not yet implemented"
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

fn serialize_node(node: &Node) -> Value {
    let base = node.base();
    json!({
        "id": base.id,
        "name": base.name,
        "node_type": base.node_type,
        "language": base.language,
        "file_path": base.file_path,
        "span": base.span,
        "ai_summary": base.ai_summary,
        "ai_summary_status": base.ai_summary_status,
        "ai_model_used": base.ai_model_used,
        "structural_hash": base.structural_hash,
        "flaws": base.flaws,
    })
}
