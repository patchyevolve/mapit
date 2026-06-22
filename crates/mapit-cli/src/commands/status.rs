use std::path::Path;
use anyhow::Result;
use mapit_core::{config::load_project_config, graph::store::GraphStore};

pub async fn run(target: &Path) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");

    if !db_path.exists() {
        println!("No map found for this directory. Run `mapit map` first.");
        return Ok(());
    }

    let store = GraphStore::open(&db_path)?;
    let cfg = load_project_config(&mapit_dir).unwrap_or_default();

    println!("mapit status");
    println!("  Nodes : {}", store.node_count()?);
    println!("  Edges : {}", store.edge_count()?);
    println!("  Funcs : {}", store.function_count()?);
    let flaw_count = store.flaw_count(None)?;
    println!("  Flaws : {}", flaw_count);
    if flaw_count > 0 {
        let hi = store.flaw_count(Some("high"))?;
        let warn = store.flaw_count(Some("warning"))?;
        let info = store.flaw_count(Some("info"))?;
        if hi > 0 {
            println!("         high: {hi}");
        }
        if warn > 0 {
            println!("         warning: {warn}");
        }
        if info > 0 {
            println!("         info: {info}");
        }
    }
    if let Some(t) = &cfg.last_full_map_at {
        println!("  Last full map   : {t}");
    }
    if let Some(t) = &cfg.last_incremental_map_at {
        println!("  Last incremental: {t}");
    }
    Ok(())
}
