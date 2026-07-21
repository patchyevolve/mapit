use anyhow::Result;
use clap::Subcommand;
use mapit_core::config;

#[derive(Subcommand)]
pub enum ProjectsAction {
    List,
    Remove { path: String },
}

pub async fn run(action: ProjectsAction) -> Result<()> {
    let config_dir = config::global_config_dir();
    let projects_path = config_dir.join("projects.json");
    let mut projects: Vec<String> = if projects_path.exists() {
        let text = std::fs::read_to_string(&projects_path)?;
        serde_json::from_str(&text).unwrap_or_default()
    } else {
        Vec::new()
    };

    match action {
        ProjectsAction::List => {
            if projects.is_empty() {
                println!("No projects mapped yet.");
            } else {
                for p in &projects {
                    println!("  {p}");
                }
            }
        }
        ProjectsAction::Remove { path } => {
            let resolved = std::path::Path::new(&path).canonicalize().ok().map(|p| p.to_string_lossy().to_string()).unwrap_or(path.clone());
            let before = projects.len();
            projects.retain(|p| *p != resolved && *p != path);
            if projects.len() < before {
                std::fs::write(&projects_path, serde_json::to_string_pretty(&projects)?)?;
                println!("Removed from project history.");
            } else {
                println!("Project not found in history.");
            }
        }
    }
    Ok(())
}
