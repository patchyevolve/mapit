use anyhow::{Context, Result};
use serde::Deserialize;
use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};

pub struct OllamaProvider {
    pub base_url: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

impl AiProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp: OllamaTagsResponse = reqwest::blocking::get(&url)
            .context("Ollama list_models failed")?
            .json()
            .context("Ollama list_models parse failed")?;
        Ok(resp
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name.clone(),
                name: m.name,
            })
            .collect())
    }

    fn complete(&self, request: AiRequest) -> Result<AiResponse> {
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let mut messages = Vec::new();
        if let Some(sys) = &request.system_prompt {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys,
            }));
        }
        messages.push(serde_json::json!({
            "role": "user",
            "content": request.user_prompt,
        }));

        let body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "stream": false,
            "format": if request.expect_json { "json" } else { "" },
        });

        let client = reqwest::blocking::Client::new();
        let resp: OllamaChatResponse = client
            .post(&url)
            .json(&body)
            .send()
            .context("Ollama complete request failed")?
            .json()
            .context("Ollama complete response parse failed")?;

        Ok(AiResponse {
            content: resp.message.content,
            model_used: request.model,
            finish_reason: Some("stop".to_owned()),
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
