//! AiProvider trait — the only integration point for new providers (TRD §5.1).
use anyhow::Result;
use serde::{Deserialize, Serialize};

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
