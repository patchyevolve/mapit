use std::path::Path;
use anyhow::Result;
use mapit_core::graph::store::GraphStore;

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

    println!("{} matches for \"{name}\":", results.len());
    for node in &results {
        let base = node.base();
        let location = base
            .file_path
            .as_deref()
            .unwrap_or("?");
        let line = base
            .span
            .as_ref()
            .map(|s| s.start_line.to_string())
            .unwrap_or_else(|| "?".to_owned());
        let ai = match &base.ai_summary_status {
            mapit_core::graph::model::AiSummaryStatus::Ready => {
                base.ai_summary.as_deref().unwrap_or("")
            }
            mapit_core::graph::model::AiSummaryStatus::Pending => " [summary pending]",
            mapit_core::graph::model::AiSummaryStatus::Unavailable => "",
        };
        let type_label = match &base.node_type {
            mapit_core::graph::model::NodeType::Function => "fn",
            mapit_core::graph::model::NodeType::Type => "type",
            mapit_core::graph::model::NodeType::Macro => "macro",
            mapit_core::graph::model::NodeType::Global => "global",
            mapit_core::graph::model::NodeType::Module => "mod",
            mapit_core::graph::model::NodeType::File => "file",
            mapit_core::graph::model::NodeType::Feature => "feature",
            mapit_core::graph::model::NodeType::External => "external",
        };
        if !ai.is_empty() {
            println!("  {type_label:8} {name}  {location}:{line}  {ai}", name = base.name);
        } else {
            println!("  {type_label:8} {name}  {location}:{line}{ai}", name = base.name);
        }
    }
    Ok(())
}
