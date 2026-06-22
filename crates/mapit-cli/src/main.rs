//! mapit CLI entry point (Phase 4 will wire all commands fully).
//! For now: argument structure is defined, commands beyond `map` are stubs.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(
    name = "mapit",
    about = "AI-powered interactive codebase mapper",
    version
)]
struct Cli {
    /// Target directory to analyze (defaults to current directory).
    #[arg(long, short, global = true, default_value = ".")]
    path: std::path::PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run first-time setup without mapping.
    Init,
    /// Run (or re-run) the structural mapping pass.
    Map {
        /// Force a full re-map ignoring the incremental manifest.
        #[arg(long)]
        force: bool,
    },
    /// Run (or resume) the AI enrichment pass.
    Annotate {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        force: bool,
    },
    /// Open the web app without re-mapping.
    Open,
    /// Print a concise status summary.
    Status,
    /// Quick symbol search.
    Find { name: String },
    /// Print full AI summary + callers/callees for a symbol.
    Explain { name: String },
    /// Print a textual execution-order trace from an entry point.
    Trace {
        name: String,
        #[arg(long, default_value = "6")]
        depth: usize,
    },
    /// List AI-flagged flaws.
    Flaws {
        #[arg(long)]
        severity: Option<String>,
    },
    /// Config management.
    Config {
        #[command(subcommand)]
        action: commands::config::ConfigAction,
    },
    /// List previously mapped projects.
    Projects {
        #[command(subcommand)]
        action: commands::projects::ProjectsAction,
    },
    /// Ask a free-form question about the codebase.
    Ask { question: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing — respects RUST_LOG env var.
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let target = cli.path.canonicalize().unwrap_or(cli.path);

    match cli.command {
        None => commands::default_run::run(&target).await,
        Some(Commands::Init) => commands::init::run(&target).await,
        Some(Commands::Map { force }) => commands::map::run(&target, force).await,
        Some(Commands::Annotate { all, force }) => {
            commands::annotate::run(&target, all, force).await
        }
        Some(Commands::Open) => commands::open::run(&target).await,
        Some(Commands::Status) => commands::status::run(&target).await,
        Some(Commands::Find { name }) => commands::find::run(&target, &name).await,
        Some(Commands::Explain { name }) => commands::explain::run(&target, &name).await,
        Some(Commands::Trace { name, depth }) => {
            commands::trace::run(&target, &name, depth).await
        }
        Some(Commands::Flaws { severity }) => {
            commands::flaws::run(&target, severity.as_deref()).await
        }
        Some(Commands::Config { action }) => commands::config::run(action).await,
        Some(Commands::Projects { action }) => commands::projects::run(action).await,
        Some(Commands::Ask { question }) => commands::ask::run(&target, &question).await,
    }
}
