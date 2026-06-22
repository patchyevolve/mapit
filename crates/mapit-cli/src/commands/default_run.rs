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
        println!("✓ First map complete.");
    }

    // Start server and open browser
    let global = config::load_global_config(&config_dir).unwrap_or_default();
    let port = global.ui_preferences.preferred_port;

    println!("Starting mapit server on http://127.0.0.1:{port}");
    if webbrowser::open(&format!("http://127.0.0.1:{port}")).is_ok() {
        println!("Opened browser. Press Ctrl+C to stop.");
    } else {
        println!("Open http://127.0.0.1:{port} in your browser.");
        println!("Press Ctrl+C to stop.");
    }

    mapit_server::serve(&db_path, port).await?;

    Ok(())
}
