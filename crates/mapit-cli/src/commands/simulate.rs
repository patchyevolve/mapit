use std::path::Path;
use anyhow::Result;
use mapit_ai::{
    provider::{self as ai_provider, AiProvider},
    tasks::{self, SimulateOutput},
};
use mapit_core::{
    config::{load_global_config, load_project_config},
    graph::context as ctx,
};

pub async fn run(
    target: &Path,
    name: &str,
    level: &str,
) -> Result<()> {
    let mapit_dir = target.join(".mapit");
    let db_path = mapit_dir.join("graph.sqlite");
    if !db_path.exists() {
        println!("No map found. Run `mapit map` first.");
        return Ok(());
    }

    let global_cfg = load_global_config(&mapit_core::config::global_config_dir())
        .unwrap_or_default();
    let project_cfg = load_project_config(&mapit_dir).unwrap_or_default();
    let provider = ai_provider::create_provider(&global_cfg, &project_cfg)?;
    let model = project_cfg
        .model_override
        .as_deref()
        .unwrap_or(&global_cfg.default_model);

    let store = mapit_core::graph::store::GraphStore::open(&db_path)?;

    let context = ctx::build_sim_context(&store, target, name, level)?;
    let overview = ctx::get_project_overview(&store);

    println!("Simulating {level} `{name}`...");

    let result = tasks::simulate(
        provider.as_ref(),
        model,
        level,
        name,
        overview.as_deref(),
        &context,
    )?;

    print_simulation(&result, level);
    Ok(())
}

fn print_simulation(result: &SimulateOutput, level: &str) {
    println!("\n═══ {} Simulation ═══", level.to_uppercase());
    println!("{}", result.summary);
    println!("\n━━ Entry ━━\n{}", result.entry);
    if !result.inputs.is_empty() {
        println!("\n━━ Inputs ━━");
        for i in &result.inputs {
            println!("  • {} ({}):", i.name, i.io_type);
            println!("    From user: {}", i.from_user);
            println!("    From system: {}", i.from_system);
        }
    }
    if !result.steps.is_empty() {
        println!("\n━━ Steps ━━");
        for s in &result.steps {
            println!("  {}. {} — {}", s.order, s.action, s.detail);
        }
    }
    if !result.outputs.is_empty() {
        println!("\n━━ Outputs ━━");
        for o in &result.outputs {
            println!("  • {} ({}):", o.name, o.io_type);
            if !o.to_user.is_empty() {
                println!("    To user: {}", o.to_user);
            }
            if !o.to_system.is_empty() {
                println!("    To system: {}", o.to_system);
            }
            if !o.side_effects.is_empty() {
                println!("    Side effects: {}", o.side_effects);
            }
        }
    }
    println!("\n━━ Exit ━━\n{}", result.exit);
    if !result.errors.is_empty() {
        println!("\n━━ Error Paths ━━");
        for e in &result.errors {
            println!("  • If {}: {}", e.condition, e.result);
        }
    }
    println!();
}
