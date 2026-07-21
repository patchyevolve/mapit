use std::path::Path;
use anyhow::{Context, Result};
use mapit_core::config;

pub async fn run(target: &Path, cli_port: Option<u16>) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` first.");
        return Ok(());
    }

    let config_dir = config::global_config_dir();
    let global = config::load_global_config(&config_dir).unwrap_or_default();
    let preferred = cli_port.unwrap_or(global.ui_preferences.preferred_port);

    let port = mapit_server::find_free_port(preferred).await
        .context("no free port available")?;

    if port != preferred {
        println!("Port {preferred} is in use — using port {port} instead.");
    }

    println!("Starting mapit server on http://127.0.0.1:{port}");
    println!("Open http://127.0.0.1:{port} in your browser.");
    println!("Press Ctrl+C to stop the server.");

    mapit_server::serve(&db_path, port, Some(target)).await
        .context("server error")?;

    Ok(())
}
