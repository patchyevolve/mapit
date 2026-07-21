//! AiProvider trait — the only integration point for new providers (TRD §5.1).
use anyhow::Result;
use serde::{Deserialize, Serialize};
use mapit_core::config::{GlobalConfig, load_credentials, ProjectConfig};

use crate::ollama::OllamaProvider;
use crate::openai_compatible::OpenAiCompatibleProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub model: String,
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    /// If true, the caller expects a JSON response.
    pub expect_json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub model_used: String,
    pub finish_reason: Option<String>,
}

pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn list_models(&self) -> Result<Vec<ModelInfo>>;
    fn complete(&self, request: AiRequest) -> Result<AiResponse>;
    fn supports_streaming(&self) -> bool;
}

pub fn create_provider(global: &GlobalConfig, project: &ProjectConfig) -> Result<Box<dyn AiProvider>> {
    let provider_name = project
        .provider_override
        .as_deref()
        .unwrap_or(&global.default_provider);
    match provider_name {
        "ollama" => Ok(Box::new(OllamaProvider {
            base_url: global.ollama_base_url.clone(),
        })),
        "openai-compatible" => {
            let config_dir = mapit_core::config::global_config_dir();
            let creds = load_credentials(&config_dir).unwrap_or_default();
            let entry = creds.providers.get("openai-compatible");
            let base_url = entry.map(|c| c.base_url.clone()).unwrap_or_default();
            let api_key = entry.map(|c| c.api_key.clone()).unwrap_or_default();
            Ok(Box::new(OpenAiCompatibleProvider {
                base_url,
                api_key,
                model: global.default_model.clone(),
            }))
        }
        other => anyhow::bail!("Unknown provider '{other}'. Use 'ollama' or 'openai-compatible'."),
    }
}
