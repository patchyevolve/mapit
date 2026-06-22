use std::path::Path;
use anyhow::Result;
use mapit_core::config;

pub async fn run(target: &Path) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let is_first_run = !db_path.exists();

    let config_dir = config::global_config_dir();
    let global_config_path = config_dir.join("global_config.json");

    // Per App-Flow §1: on first run, trigger first-time setup
    if is_first_run || !global_config_path.exists() {
        if !global_config_path.exists() {
            println!("First run detected — let's set up your AI provider first.");
            println!("(You can skip AI setup and just use structural mapping.)");
            println!();
            super::init::run(target).await?;
        }
    }

    super::map::run(target, false).await?;

    // Save to projects list
    if let Ok(abs) = target.canonicalize() {
        let projects_path = config_dir.join("projects.json");
        let mut projects: Vec<String> = if projects_path.exists() {
            std::fs::read_to_string(&projects_path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let path_str = abs.to_string_lossy().to_string();
        if !projects.contains(&path_str) {
            projects.push(path_str);
            if let Ok(text) = serde_json::to_string_pretty(&projects) {
                let _ = std::fs::write(&projects_path, text);
            }
        }
    }

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

    mapit_server::serve(&db_path, port, Some(target)).await?;

    Ok(())
}
