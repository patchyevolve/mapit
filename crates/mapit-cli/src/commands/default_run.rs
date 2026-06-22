//! Default `mapit` (no subcommand) — init if first run, map, then open hints.
use std::path::Path;
use anyhow::Result;
use mapit_core::config;

pub async fn run(target: &Path) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let is_first_run = !db_path.exists();

    // Check if global config exists; suggest init if not
    let config_dir = config::global_config_dir();
    let global_config_path = config_dir.join("global_config.json");
    if !global_config_path.exists() {
        println!("No AI provider configured. Run `mapit init` to set one up,");
        println!("or proceed with structural mapping only (no AI enrichment).");
        println!();
    }

    super::map::run(target, false).await?;

    if is_first_run {
        println!("✓ First map complete. Run `mapit open` to view (Phase 7).");
        println!("  Try:  mapit find <name>   — search for a symbol");
        println!("        mapit explain <name> — show symbol details");
        println!("        mapit status         — show graph summary");
    } else {
        println!("Run `mapit open` to view the interactive graph (Phase 7).");
    }
    Ok(())
}
