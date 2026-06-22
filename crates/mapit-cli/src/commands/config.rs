use anyhow::Result;
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
            let mut global = config::load_global_config(&config_dir).unwrap_or_default();
            global.default_provider = provider.clone();
            config::save_global_config(&config_dir, &global)?;
            println!("Default provider set to \"{provider}\".");
        }
        ConfigAction::SetModel { model } => {
            let mut global = config::load_global_config(&config_dir).unwrap_or_default();
            global.default_model = model.clone();
            config::save_global_config(&config_dir, &global)?;
            println!("Default model set to \"{model}\".");
        }
    }
    Ok(())
}
