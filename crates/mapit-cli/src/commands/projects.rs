use anyhow::Result;
use clap::Subcommand;
use mapit_core::config;

#[derive(Subcommand)]
pub enum ProjectsAction {
    List,
}

pub async fn run(action: ProjectsAction) -> Result<()> {
    match action {
        ProjectsAction::List => {
            let config_dir = config::global_config_dir();
            let projects_path = config_dir.join("projects.json");
            if projects_path.exists() {
                let text = std::fs::read_to_string(&projects_path)?;
                let projects: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
                if projects.is_empty() {
                    println!("No projects mapped yet.");
                } else {
                    for p in &projects {
                        println!("  {p}");
                    }
                }
            } else {
                println!("No projects mapped yet.");
            }
        }
    }
    Ok(())
}
