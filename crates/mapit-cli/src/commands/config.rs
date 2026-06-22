use std::io::{self, BufRead, Write};
use anyhow::{Context, Result};
use clap::Subcommand;
use mapit_core::config;

#[derive(Subcommand)]
pub enum ConfigAction {
    Show,
    SetProvider { provider: String },
    SetModel { model: String },
}

pub async fn run(action: ConfigAction) -> Result<()> {
    let config_dir = config::global_config_dir();

    match action {
        ConfigAction::Show => {
            let global = config::load_global_config(&config_dir).unwrap_or_default();
            println!("Global config ({:?}/global_config.json):", config_dir);
            println!("  Default provider : {}", global.default_provider);
            println!("  Default model    : {}", global.default_model);
            println!("  Ollama base URL  : {}", global.ollama_base_url);
            println!("  UI port          : {}", global.ui_preferences.preferred_port);
            println!("  UI theme         : {}", global.ui_preferences.theme);
            println!("  Ignore patterns  : {} entries", global.default_ignore_patterns.len());
        }
        ConfigAction::SetProvider { provider } => {
            match provider.as_str() {
                "ollama" => {
                    let mut global = config::load_global_config(&config_dir).unwrap_or_default();
                    global.default_provider = "ollama".into();
                    config::save_global_config(&config_dir, &global)?;
                    println!("Default provider set to \"ollama\".");
                }
                "openai-compatible" => {
                    // Interactive prompt per App Flow §1
                    println!("Setting up OpenAI-compatible provider...\n");

                    let base_url = prompt_string("Base URL (e.g. https://api.openai.com/v1)")?;
                    let api_key = rpassword::prompt_password("API key (input hidden): ")
                        .context("Failed to read API key")?;
                    let model = prompt_string("Model name (e.g. gpt-4o-mini, claude-3-haiku)")?;

                    let mut global = config::load_global_config(&config_dir).unwrap_or_default();
                    global.default_provider = "openai-compatible".into();
                    global.default_model = model.clone();
                    global.ollama_base_url = base_url.clone();
                    config::save_global_config(&config_dir, &global)?;

                    // Save credentials
                    let mut creds = config::load_credentials(&config_dir).unwrap_or_default();
                    creds.providers.insert(
                        "openai-compatible".into(),
                        config::ProviderCredential {
                            base_url,
                            api_key,
                            model,
                        },
                    );
                    config::save_credentials(&config_dir, &creds)?;

                    println!("✓ OpenAI-compatible provider configured.");
                }
                other => {
                    anyhow::bail!("Unknown provider '{other}'. Use 'ollama' or 'openai-compatible'.");
                }
            }
        }
        ConfigAction::SetModel { model } => {
            let mut global = config::load_global_config(&config_dir).unwrap_or_default();
            global.default_model = model.clone();
            config::save_global_config(&config_dir, &global)?;

            // If using openai-compatible, also update credentials
            if global.default_provider == "openai-compatible" {
                let mut creds = config::load_credentials(&config_dir).unwrap_or_default();
                if let Some(entry) = creds.providers.get_mut("openai-compatible") {
                    entry.model = model.clone();
                    config::save_credentials(&config_dir, &creds)?;
                }
            }

            println!("Default model set to \"{model}\".");
        }
    }
    Ok(())
}

fn prompt_string(prompt: &str) -> Result<String> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().to_owned())
}
