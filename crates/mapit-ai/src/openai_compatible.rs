//! OpenAI-compatible provider (Phase 5 stub).
use anyhow::Result;
use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};

pub struct OpenAiCompatibleProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl AiProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str { "openai-compatible" }
    fn list_models(&self) -> Result<Vec<ModelInfo>> { Ok(vec![]) /* TODO Phase 5 */ }
    fn complete(&self, _request: AiRequest) -> Result<AiResponse> {
        anyhow::bail!("OpenAiCompatibleProvider not yet implemented (Phase 5)")
    }
    fn supports_streaming(&self) -> bool { false }
}
