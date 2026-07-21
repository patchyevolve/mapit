//! mapit CLI entry point.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(
    name = "mapit",
    about = "codebase mapper — builds a live call graph from your source tree",
    version
)]
struct Cli {
    /// Target directory to analyze (defaults to current directory).
    #[arg(long, short, global = true, default_value = ".")]
    path: std::path::PathBuf,

    /// Port for the web UI server (overrides config).
    #[arg(long, global = true)]
    port: Option<u16>,

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
        /// Skip the flaw-flagging AI pass (saves ~1 call per function).
        #[arg(long)]
        no_flaws: bool,
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
    /// Simulate runtime behavior of a function, file, module, or the whole project.
    Simulate {
        name: String,
        /// Level of simulation: function, file, module, project.
        #[arg(long, default_value = "function")]
        level: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let target = cli.path.canonicalize().unwrap_or(cli.path);
    let port = cli.port;

    match cli.command {
        None => commands::default_run::run(&target, port).await,
        Some(Commands::Init) => commands::init::run(&target).await,
        Some(Commands::Map { force }) => commands::map::run(&target, force).await,
        Some(Commands::Annotate { all, force, no_flaws }) => {
            commands::annotate::run(&target, all, force, no_flaws).await
        }
        Some(Commands::Open) => commands::open::run(&target, port).await,
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
        Some(Commands::Simulate { name, level }) => {
            commands::simulate::run(&target, &name, &level).await
        }
    }
}
