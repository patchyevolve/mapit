use std::path::Path;
use anyhow::Result;
use mapit_core::{
    control_flow::walk_trace_with_depth,
    graph::{
        model::Node,
        store::GraphStore,
    },
};

pub async fn run(target: &Path, name: &str, depth: usize) -> Result<()> {
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

    match node {
        Node::Function(f) => {
            if let Some(cfg) = &f.control_flow {
                let paths = walk_trace_with_depth(cfg, depth);
                println!(
                    "Trace from \"{}\" ({} paths, max depth {depth}):",
                    f.base.name,
                    paths.len()
                );
                for (i, path) in paths.iter().enumerate() {
                    let label = if path.label.is_empty() {
                        "sequential".to_owned()
                    } else {
                        path.label.clone()
                    };
                    let block_ids: Vec<&str> = path.blocks.iter()
                        .take(depth)
                        .map(|s| s.as_str())
                        .collect();
                    let suffix = if path.blocks.len() > depth { "..." } else { "" };
                    println!(
                        "  Path {}: [{}]{}  ({})",
                        i + 1,
                        block_ids.join(" → "),
                        suffix,
                        label,
                    );
                }
            } else {
                println!("\"{}\" has no control-flow data (re-run `mapit map` to extract).", name);
            }
        }
        _ => {
            println!("\"{}\" is not a function — cannot trace.", name);
        }
    }
    Ok(())
}
