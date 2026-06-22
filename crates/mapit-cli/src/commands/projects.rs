use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProjectsAction {
    List,
}

pub async fn run(action: ProjectsAction) -> Result<()> {
    match action {
        ProjectsAction::List => println!("mapit projects list — Phase 4"),
    }
    Ok(())
}
