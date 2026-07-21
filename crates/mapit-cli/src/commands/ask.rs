use std::path::Path;
use anyhow::Result;
use mapit_core::{
    config::{load_global_config, global_config_dir, load_credentials},
    graph::{
        model::{ExternalReason, Node},
        store::GraphStore,
    },
};
use mapit_ai::{provider::AiProvider, tasks};

fn format_node(n: &Node) -> String {
    let base = n.base();
    let loc = base
        .span
        .as_ref()
        .map(|s| format!("{}:{}", s.start_line, s.end_line))
        .unwrap_or_default();
    let fp = base.file_path.as_deref().unwrap_or("");
    let lang = base.language.as_deref().unwrap_or("");
    let extra = match n {
        Node::Function(f) => {
            let sig = &f.signature;
            let sig_preview = if sig.len() > 120 {
                format!("{}...", &sig[..117])
            } else {
                sig.clone()
            };
            format!(
                " [fn] calls_in={} entry={} sig={}",
                f.has_incoming_calls, f.is_entry_point_candidate, sig_preview
            )
        }
        Node::External(e) => {
            let reason = match e.reason {
                ExternalReason::NoSourcePresent => "no_source",
                ExternalReason::DynamicDispatch => "dynamic_dispatch",
                ExternalReason::UnrecognizedBinding => "unrecognized",
            };
            format!(" [extern] reason={reason}")
        }
        Node::File(_) => " [file]".into(),
        Node::Feature(f) => format!(" [feature] members={}", f.member_node_ids.len()),
        _ => String::new(),
    };
    format!("{base_name} @ {fp}:{loc} ({lang}){extra}", base_name = base.name)
}

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
            let base_url = entry.map(|c| c.base_url.clone()).unwrap_or_default();
            let api_key = entry.map(|c| c.api_key.clone()).unwrap_or_default();
            Box::new(mapit_ai::openai_compatible::OpenAiCompatibleProvider {
                base_url,
                api_key,
                model: global.default_model.clone(),
            })
        }
        _ => return Ok("No AI provider configured. Run `mapit init`.".into()),
    };

    let model = &global.default_model;

    // Search for relevant nodes by name; if none match, try individual words
    let mut results = store.search_nodes_by_name(question).unwrap_or_default();
    if results.is_empty() {
        for word in question.split_whitespace() {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric());
            if word.len() > 2 {
                let word_results = store.search_nodes_by_name(word).unwrap_or_default();
                results.extend(word_results);
            }
        }
    }
    // Deduplicate by id, limit to 12
    results.sort_by(|a, b| a.base().id.cmp(&b.base().id));
    results.dedup_by(|a, b| a.base().id == b.base().id);
    results.truncate(12);

    let context: String = if results.is_empty() {
        "No matching nodes found in the codebase graph.".into()
    } else {
        results.iter().map(format_node).collect::<Vec<_>>().join("\n")
    };

    let result = tasks::answer(&*provider, model, &context, question)
        .map_err(|e| anyhow::anyhow!("AI call failed: {e}"))?;

    Ok(result.answer)
}

pub async fn run(target: &Path, question: &str) -> Result<()> {
    match inner(target, question).await {
        Ok(answer) => println!("{answer}"),
        Err(e) => eprintln!("Error: {e}"),
    }
    Ok(())
}
