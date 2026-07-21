use std::io::{self, BufRead, Write};
use anyhow::{Context, Result};
use mapit_ai::{
    ollama::OllamaProvider,
    openai_compatible::OpenAiCompatibleProvider,
    provider::AiProvider,
};
use mapit_core::config::{self, GlobalConfig};

pub async fn run(_target: &std::path::Path) -> Result<()> {
    let config_dir = config::global_config_dir();

    // Step 1: Welcome
    println!("╔══════════════════════════════════════════╗");
    println!("║        mapit — Codebase Mapper           ║");
    println!("╚══════════════════════════════════════════╝");
    println!();
    println!("Let's set up your AI provider for codebase analysis.");
    println!("You can skip this and use structural mapping only, or");
    println!("change the provider later with `mapit config`.");
    println!();

    // Step 2: Provider selection
    let provider = select_provider()?;

    match provider.as_str() {
        "ollama" => setup_ollama(&config_dir)?,
        "openrouter" | "opencode" | "other" => {
            setup_openai_compatible(&config_dir, &provider)?
        }
        "skip" => {
            // Save a default global config (no provider testing)
            let cfg = GlobalConfig::default();
            config::save_global_config(&config_dir, &cfg)?;
            println!("✓ AI setup skipped. Structural mapping works without AI.");
            println!("  Run `mapit config set-provider` later to add AI.");
            return Ok(());
        }
        _ => anyhow::bail!("Unknown provider: {provider}"),
    }

    // Step 4: Confirm and save
    println!();
    println!("✓ Configuration saved to {:?}", config_dir);
    println!("  You can change this anytime with:");
    println!("    mapit config set-provider <provider>");
    println!("    mapit config set-model <model>");

    // Step 5: Prompt to map
    println!();
    println!("Ready to map your project! Run:");
    println!("  mapit");
    println!("Or, if you just want the structural map without opening the browser:");
    println!("  mapit map");

    Ok(())
}

fn select_provider() -> Result<String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!("Which AI provider would you like to use for codebase analysis?");
    println!();
    println!("  1. Ollama (local, private, free — recommended if installed)");
    println!("  2. OpenRouter (remote, free + paid models available)");
    println!("  3. Opencode-hosted endpoint");
    println!("  4. Other OpenAI-compatible endpoint (custom)");
    println!("  5. Skip AI setup (structural mapping only)");
    println!();

    loop {
        print!("Enter choice [1-5]: ");
        stdout.flush()?;
        let mut input = String::new();
        if stdin.lock().read_line(&mut input)? == 0 || input.trim().is_empty() {
            println!("No input provided. Skipping AI setup.");
            return Ok("skip".to_owned());
        }
        match input.trim() {
            "1" => return Ok("ollama".to_owned()),
            "2" => return Ok("openrouter".to_owned()),
            "3" => return Ok("opencode".to_owned()),
            "4" => return Ok("other".to_owned()),
            "5" => return Ok("skip".to_owned()),
            _ => println!("Please enter a number 1-5."),
        }
    }
}

fn setup_ollama(config_dir: &std::path::Path) -> Result<()> {
    let base_url = "http://localhost:11434".to_owned();
    let provider = OllamaProvider {
        base_url: base_url.clone(),
    };

    println!();
    println!("Checking Ollama at {base_url}...");

    match provider.list_models() {
        Ok(models) => {
            println!("✓ Ollama is reachable! Found {} model(s).", models.len());
            if models.is_empty() {
                println!("  No models pulled yet. You'll need to pull one.");
            } else {
                println!("  Available models:");
                for (i, m) in models.iter().enumerate() {
                    println!("    {}. {}", i + 1, m.name);
                }
            }
        }
        Err(e) => {
            println!("✗ Could not reach Ollama at {base_url}.");
            println!("  Error: {e}");
            println!();
            println!("  To install Ollama, visit: https://ollama.ai");
            println!("  After installing, run `ollama pull qwen2.5-coder:7b`");
            println!("  Then run `mapit init` again.");
            println!();
            println!("  You can also skip AI setup and use structural mapping only.");
            let proceed = prompt_yes_no("Proceed with Ollama anyway?")?;
            if !proceed {
                return setup_ollama_fallback();
            }
        }
    }

    // Choose model
    let model = prompt_string("Model name (e.g. qwen2.5-coder:7b, codellama, llama3.1)")?;

    let cfg = GlobalConfig {
        default_provider: "ollama".to_owned(),
        default_model: model,
        ollama_base_url: base_url,
        ..Default::default()
    };
    config::save_global_config(config_dir, &cfg)?;
    Ok(())
}

fn setup_ollama_fallback() -> Result<()> {
    println!("Choose a fallback or skip:");
    println!("  1. OpenRouter");
    println!("  2. Opencode");
    println!("  3. Other OpenAI-compatible");
    println!("  4. Skip AI setup");

    let choice = loop {
        print!("Choice [1-4]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        match input.trim() {
            "1" => break "openrouter",
            "2" => break "opencode",
            "3" => break "other",
            "4" => break "skip",
            _ => println!("Please enter 1-4."),
        }
    };

    match choice {
        "skip" => {
            let cfg = GlobalConfig::default();
            config::save_global_config(
                &config::global_config_dir(),
                &cfg,
            )?;
            println!("✓ AI setup skipped.");
            Ok(())
        }
        provider => setup_openai_compatible(&config::global_config_dir(), provider),
    }
}

fn setup_openai_compatible(config_dir: &std::path::Path, provider_name: &str) -> Result<()> {
    let (preset_url, preset_name) = match provider_name {
        "openrouter" => (
            "https://openrouter.ai/api/v1".to_owned(),
            "OpenRouter".to_owned(),
        ),
        "opencode" => (
            "https://opencode.ai/zen/v1".to_owned(),
            "Opencode".to_owned(),
        ),
        _ => (String::new(), "custom OpenAI-compatible".to_owned()),
    };

    println!();
    println!("Setting up {preset_name} provider:");

    let base_url = if preset_url.is_empty() {
        prompt_string("Base URL (e.g. https://api.openai.com/v1)")?
    } else {
        let url = prompt_string(&format!(
            "Base URL [{}]",
            preset_url
        ))?;
        if url.is_empty() {
            preset_url
        } else {
            url
        }
    };

    let api_key = rpassword::prompt_password("API key (input hidden): ")
        .context("Failed to read API key")?;
    if api_key.is_empty() {
        anyhow::bail!("API key cannot be empty.");
    }

    // Step 3: Privacy notice for non-Ollama
    println!();
    println!("Privacy notice:");
    println!("  mapit will send function source code and structural context to");
    println!("  the configured endpoint for AI enrichment tasks (summarization,");
    println!("  flaw detection, classification). No entire repository is sent at once.");
    println!("  Your API key is stored locally and never shared.");
    let confirmed = prompt_yes_no("Do you want to proceed?")?;
    if !confirmed {
        anyhow::bail!("Setup cancelled by user.");
    }

    let model = prompt_string("Model name (e.g. gpt-4o-mini, claude-3-haiku, qwen2.5-coder:7b)")?;

    // Test the connection
    println!("Testing connection to {base_url}...");
    let provider = OpenAiCompatibleProvider {
        base_url: base_url.clone(),
        api_key: api_key.clone(),
        model: model.clone(),
    };

    match provider.list_models() {
        Ok(models) => {
            println!("✓ Connection successful! {} model(s) available.", models.len());
            if models.iter().any(|m| m.id == model) {
                println!("  Model '{model}' is available.");
            } else {
                println!("  (Note: '{model}' not in the model list, but may still work.)");
            }
        }
        Err(e) => {
            eprintln!("⚠ Connection warning: {e}");
            eprintln!("  Config will be saved, but you may need to check the URL/key.");
        }
    }

    let cfg = GlobalConfig {
        default_provider: "openai-compatible".to_owned(),
        default_model: model.clone(),
        ..Default::default()
    };
    config::save_global_config(config_dir, &cfg)?;

    // Save API key to credentials.json (separate file, restricted permissions)
    let mut credentials = config::load_credentials(config_dir).unwrap_or_default();
    credentials.providers.insert(
        "openai-compatible".to_owned(),
        config::ProviderCredential {
            base_url,
            api_key,
            model,
        },
    );
    config::save_credentials(config_dir, &credentials)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn prompt_yes_no(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn prompt_string(prompt: &str) -> Result<String> {
    print!("{prompt}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().to_owned())
}
