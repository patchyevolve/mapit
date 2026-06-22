//! Ollama provider (Phase 5 stub).
use anyhow::Result;
use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};

pub struct OllamaProvider {
    pub base_url: String,
}

impl AiProvider for OllamaProvider {
    fn id(&self) -> &str { "ollama" }
    fn list_models(&self) -> Result<Vec<ModelInfo>> { Ok(vec![]) /* TODO Phase 5 */ }
    fn complete(&self, _request: AiRequest) -> Result<AiResponse> {
        anyhow::bail!("OllamaProvider not yet implemented (Phase 5)")
    }
    fn supports_streaming(&self) -> bool { false }
}
