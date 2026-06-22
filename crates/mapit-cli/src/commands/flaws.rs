use std::path::Path;
use anyhow::Result;
use mapit_core::graph::store::GraphStore;

pub async fn run(target: &Path, severity: Option<&str>) -> Result<()> {
    let db_path = target.join(".mapit").join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` AI pass first.");
        return Ok(());
    }

    let store = GraphStore::open(&db_path)?;
    let flaws = store.query_flaws(severity)?;

    if flaws.is_empty() {
        match severity {
            Some(s) => println!("No flaws with severity \"{s}\"."),
            None => println!("No flaws found."),
        }
        return Ok(());
    }

    for (flaw, node_name, file_path) in &flaws {
        let loc = file_path.as_deref().unwrap_or("?");
        println!(
            "  [{:5}] {:30} {:15} {:6.0}%  {}  {}",
            format!("{:?}", flaw.severity),
            format!("{:?}", flaw.kind),
            node_name,
            flaw.confidence * 100.0,
            flaw.description,
            loc,
        );
    }
    println!("Total: {} flaw(s)", flaws.len());
    Ok(())
}
