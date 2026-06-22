use std::path::Path;
use anyhow::{Context, Result};
use mapit_core::config;

pub async fn run(target: &Path) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` first.");
        return Ok(());
    }

    let config_dir = config::global_config_dir();
    let global = config::load_global_config(&config_dir).unwrap_or_default();
    let port = global.ui_preferences.preferred_port;

    println!("Starting mapit server on http://127.0.0.1:{port}");

    if webbrowser::open(&format!("http://127.0.0.1:{port}")).is_ok() {
        println!("Opened browser. Press Ctrl+C to stop the server.");
    } else {
        println!("Open http://127.0.0.1:{port} in your browser.");
        println!("Press Ctrl+C to stop the server.");
    }

    mapit_server::serve(&db_path, port, Some(target)).await
        .context("server error")?;

    Ok(())
}
