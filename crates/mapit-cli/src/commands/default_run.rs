use std::path::Path;
use anyhow::{Context, Result};
use mapit_core::config;
use tokio::io::{AsyncBufReadExt, BufReader};

const SPLASH: &str = r#"
                    __  __    _    ____ ___ ___
                   |  \/  |  / \  |  _ \_ _/ _ \
                   | |\/| | / _ \ | |_) | | | | |
                   | |  | |/ ___ \|  __/| | |_| |
                   |_|  |_/_/   \_\_|  |___\___/
"#;

const USE_CASES: &str = r#"
  ┌───────────────────────────────────────────────────────────────┐
  │                                                               │
  │  🗺  Interactive codebase mapper with AI-powered insights      │
  │                                                               │
  │  • Structural mapping  – build a call-graph from your code     │
  │  • AI enrichment       – auto-summarize symbols & flag flaws   │
  │  • Interactive graph   – explore, trace, search                │
  │  • Execution simulation– animate call paths through the graph   │
  │  • Live web UI         – visual, real-time interactive view    │
  │                                                               │
  └───────────────────────────────────────────────────────────────┘
"#;

fn show_splash() {
    print!("\x1B[2J\x1B[H");
    println!("\x1b[36m{}\x1b[0m", SPLASH);
    println!("{}", USE_CASES);
}

fn show_help(port: u16) {
    println!();
    println!("  \x1b[1mCommands\x1b[0m  (connected to http://127.0.0.1:{port})");
    println!("  ─────────────────────────────────────────────");
    println!("  \x1b[33mannotate\x1b[0m   – Run AI enrichment (summaries + flaws)");
    println!("  \x1b[33mremap\x1b[0m       – Re-run structural mapping");
    println!("  \x1b[33mstatus\x1b[0m      – Show project stats");
    println!("  \x1b[33mflaws\x1b[0m       – List AI-detected flaws");
    println!("  \x1b[33msearch <q>\x1b[0m  – Search symbols");
    println!("  \x1b[33mopen\x1b[0m        – Open web UI in browser");
    println!("  \x1b[33mhelp\x1b[0m        – Show this help");
    println!("  \x1b[33mexit\x1b[0m        – Stop server and quit");
    println!();
}

async fn interactive_loop(port: u16) -> Result<()> {
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    show_help(port);

    loop {
        print!("\x1b[32mmapit>\x1b[0m ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let line = match lines.next_line().await? {
            Some(l) => l.trim().to_string(),
            None => break,
        };

        if line.is_empty() { continue; }

        match line.split_whitespace().next().unwrap_or("") {
            "exit" | "quit" | "q" => {
                println!("Shutting down...");
                break;
            }
            "help" | "h" | "?" => show_help(port),
            "open" => {
                if webbrowser::open(&format!("http://127.0.0.1:{port}")).is_ok() {
                    println!("Opened browser.");
                } else {
                    eprintln!("Failed to open browser. Visit http://127.0.0.1:{port} manually.");
                }
            }
            "status" => {
                match client.get(format!("{}/api/project", base)).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
                        }
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            "annotate" => {
                let payload = serde_json::json!({ "all": true, "force": false, "skip_flaws": false });
                match client.post(format!("{}/api/annotate", base)).json(&payload).send().await {
                    Ok(resp) => match resp.json::<serde_json::Value>().await {
                        Ok(body) => println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default()),
                        Err(e) => eprintln!("Error parsing response: {e}"),
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            "remap" => {
                let force = line.split_whitespace().any(|w| w == "--force");
                let payload = serde_json::json!({ "force": force });
                match client.post(format!("{}/api/remap", base)).json(&payload).send().await {
                    Ok(resp) => match resp.json::<serde_json::Value>().await {
                        Ok(body) => println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default()),
                        Err(e) => eprintln!("Error parsing response: {e}"),
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            "flaws" => {
                match client.get(format!("{}/api/graph/flaws", base)).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            let flaws = &body["flaws"];
                            let arr = flaws.as_array().map(|a| a.len()).unwrap_or(0);
                            println!("Found {arr} flaws:");
                            if let Some(entries) = flaws.as_array() {
                                for f in entries {
                                    let kind = f["kind"].as_str().unwrap_or("?");
                                    let desc = f["description"].as_str().unwrap_or("");
                                    let file = f["file_path"].as_str().unwrap_or("");
                                    let name = f["primary_node_name"].as_str().unwrap_or("");
                                    println!("  \x1b[31m{kind}\x1b[0m {name} — {desc}  (\x1b[2m{file}\x1b[0m)");
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            cmd if cmd == "search" => {
                let query = line.split_whitespace().skip(1).collect::<Vec<_>>().join(" ");
                if query.is_empty() {
                    println!("Usage: search <query>");
                    continue;
                }
                match client.get(format!("{}/api/graph/search?q={}", base, urlencoding(&query))).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            let results = &body["results"];
                            if let Some(arr) = results.as_array() {
                                if arr.is_empty() {
                                    println!("No results for \"{query}\"");
                                } else {
                                    for r in arr {
                                        let name = r["node"]["name"].as_str().unwrap_or("?");
                                        let reason = r["match_reason"].as_str().unwrap_or("");
                                        let file = r["node"]["file_path"].as_str().unwrap_or("");
                                        println!("  {name}  (\x1b[2m{reason}, {file}\x1b[0m)");
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            other => {
                eprintln!("Unknown command: {other}. Type \x1b[33mhelp\x1b[0m for available commands.");
            }
        }
    }
    Ok(())
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

pub async fn run(target: &Path, cli_port: Option<u16>) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    let is_first_run = !db_path.exists();

    let config_dir = config::global_config_dir();
    let global_config_path = config_dir.join("global_config.json");

    show_splash();

    // Per App-Flow §1: on first run, trigger first-time setup
    if is_first_run || !global_config_path.exists() {
        if !global_config_path.exists() {
            println!("\x1b[33mFirst run detected — let's set up your AI provider.\x1b[0m");
            println!("  (You can skip AI setup and just use structural mapping.)");
            println!();
            super::init::run(target).await?;
        }
    }

    println!("\x1b[36m→ Running structural mapping...\x1b[0m");
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
        println!("\x1b[32m✓ First map complete.\x1b[0m");
    }

    // Start server and open browser — auto-bind if port is busy
    let global = config::load_global_config(&config_dir).unwrap_or_default();
    let preferred = cli_port.unwrap_or(global.ui_preferences.preferred_port);

    let port = mapit_server::find_free_port(preferred).await
        .context("no free port available")?;

    if port != preferred {
        println!("\x1b[33mPort {preferred} is in use — using port {port} instead.\x1b[0m");
    }

    println!("\x1b[36m→ Starting web server on http://127.0.0.1:{port}\x1b[0m");

    let server_db = db_path.clone();
    let server_target = target.to_owned();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = mapit_server::serve(&server_db, port, Some(&server_target)).await {
            eprintln!("\x1b[31mServer error: {e}\x1b[0m");
        }
    });

    // Small delay for server to start
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    if webbrowser::open(&format!("http://127.0.0.1:{port}")).is_ok() {
        println!("\x1b[32m✓ Browser opened.\x1b[0m");
    } else {
        println!("  Open http://127.0.0.1:{port} in your browser.");
    }
    println!("\x1b[90m────────────────────────────────────────────────\x1b[0m");
    println!("\x1b[2mServer logs appear below. Type commands at the prompt.\x1b[0m");
    println!();

    let _ = interactive_loop(port).await;

    server_handle.abort();
    println!("\x1b[33mServer stopped. Goodbye!\x1b[0m");
    Ok(())
}
