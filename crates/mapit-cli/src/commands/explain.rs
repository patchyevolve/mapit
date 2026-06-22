use std::path::Path;
use anyhow::Result;
use mapit_core::graph::{
    model::{EdgeType, Node},
    store::GraphStore,
};

pub async fn run(target: &Path, name: &str) -> Result<()> {
    let db_path = target.join(".mapit").join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` first.");
        return Ok(());
    }

    let store = GraphStore::open(&db_path)?;
    let results = store.search_nodes_by_name(name)?;
    if results.is_empty() {
        println!("No symbols matching \"{name}\".");
        return Ok(());
    }

    let node = results
        .iter()
        .find(|n| n.base().name == name)
        .unwrap_or(&results[0]);

    let base = node.base();

    let location = base
        .file_path
        .as_deref()
        .unwrap_or("?");
    let line = base
        .span
        .as_ref()
        .map(|s| format!("{}:{}", s.start_line, s.end_line))
        .unwrap_or_else(|| "?".to_owned());

    println!("{}", base.name);
    println!("  Location : {location}:{line}");

    if let Node::Function(f) = node {
        println!("  Signature: {}", f.signature);
        println!("  Entry pt : {}", f.is_entry_point_candidate);
        println!("  Callers  : {}", if f.has_incoming_calls { "yes" } else { "no" });
        if let Some(cfg) = &f.control_flow {
            println!("  CFG      : {} blocks", cfg.blocks.len());
        }
    }

    let summary = base.ai_summary.as_deref().unwrap_or(match &base.ai_summary_status {
        mapit_core::graph::model::AiSummaryStatus::Pending => "not yet generated",
        mapit_core::graph::model::AiSummaryStatus::Unavailable => "unavailable",
        mapit_core::graph::model::AiSummaryStatus::Ready => "",
    });
    println!("  Summary  : {summary}");

    // Callers (incoming calls edges)
    let all_in = store.edges_to(&base.id)?;
    let callers: Vec<String> = all_in
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .filter_map(|e| store.get_node(&e.from_id).ok()?)
        .map(|n| n.base().name.clone())
        .collect();
    if callers.is_empty() {
        println!("  Called by: (none)");
    } else {
        println!("  Called by: {}", callers.join(", "));
    }

    // Callees (outgoing calls edges)
    let all_out = store.edges_from(&base.id)?;
    let callees: Vec<String> = all_out
        .iter()
        .filter(|e| matches!(e.edge_type, EdgeType::Calls))
        .filter_map(|e| store.get_node(&e.to_id).ok()?)
        .map(|n| n.base().name.clone())
        .collect();
    if callees.is_empty() {
        println!("  Calls    : (none)");
    } else {
        println!("  Calls    : {}", callees.join(", "));
    }

    // Flaws
    let flaws = store.get_flaws_for_node(&base.id)?;
    for flaw in &flaws {
        println!("  [{:?}] {:?} — {}", flaw.severity, flaw.kind, flaw.description);
    }

    Ok(())
}
