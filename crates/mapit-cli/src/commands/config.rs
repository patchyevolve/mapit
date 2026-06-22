use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ConfigAction {
    Show,
    SetProvider { provider: String },
    SetModel { model: String },
}

pub async fn run(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => println!("mapit config show — Phase 4"),
        ConfigAction::SetProvider { provider } => {
            println!("mapit config set-provider {provider} — Phase 4")
        }
        ConfigAction::SetModel { model } => println!("mapit config set-model {model} — Phase 4"),
    }
    Ok(())
}
