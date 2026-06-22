use std::path::Path;
use anyhow::Result;
use mapit_core::{
    config::{load_global_config, global_config_dir, load_credentials},
    graph::store::GraphStore,
};
use mapit_ai::{provider::AiProvider, tasks};

async fn inner(target: &Path, question: &str) -> Result<String> {
    let db_path = target.join(".mapit").join("graph.sqlite");
    if !db_path.exists() {
        return Ok("No map found. Run `mapit map` first.".into());
    }

    let store = GraphStore::open(&db_path)?;
    let global = load_global_config(&global_config_dir()).unwrap_or_default();

    let provider: Box<dyn AiProvider> = match global.default_provider.as_str() {
        "ollama" => Box::new(mapit_ai::ollama::OllamaProvider {
            base_url: global.ollama_base_url.clone(),
        }),
        "openai-compatible" => {
            let creds = load_credentials(&global_config_dir()).unwrap_or_default();
            let entry = creds.providers.get("openai-compatible");
            Box::new(mapit_ai::openai_compatible::OpenAiCompatibleProvider {
                base_url: global.ollama_base_url.clone(),
                api_key: entry.map(|c| c.api_key.clone()).unwrap_or_default(),
                model: global.default_model.clone(),
            })
        }
        _ => return Ok("No AI provider configured. Run `mapit init`.".into()),
    };

    let model = &global.default_model;

    // Search for relevant nodes
    let results = store.search_nodes_by_name(question).unwrap_or_default();
    let context_nodes: Vec<String> = results.iter().take(5).map(|n| n.base().name.clone()).collect();

    let result = tasks::answer(&*provider, model, question, &context_nodes)
        .map_err(|e| format!("AI call failed: {e}"))?;

    Ok(result.answer)
}

pub async fn run(target: &Path, question: &str) -> Result<()> {
    match inner(target, question).await {
        Ok(answer) => println!("{answer}"),
        Err(e) => eprintln!("Error: {e}"),
    }
    Ok(())
}
