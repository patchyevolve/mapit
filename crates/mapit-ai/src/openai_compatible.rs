use anyhow::{Context, Result};
use serde::Deserialize;
use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};

pub struct OpenAiCompatibleProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

impl AiProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        "openai-compatible"
    }

    fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let client = reqwest::blocking::Client::new();
        let resp: OpenAiModelsResponse = client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .context("OpenAI-compatible list_models failed")?
            .json()
            .context("OpenAI-compatible list_models parse failed")?;
        Ok(resp
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
            })
            .collect())
    }

    fn complete(&self, request: AiRequest) -> Result<AiResponse> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
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

        let effective_model = if request.model.is_empty() {
            &self.model
        } else {
            &request.model
        };

        let body = serde_json::json!({
            "model": effective_model,
            "messages": messages,
            "temperature": 0.1,
        });

        let client = reqwest::blocking::Client::new();
        let resp: ChatCompletionResponse = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .context("OpenAI-compatible complete request failed")?
            .json()
            .context("OpenAI-compatible complete response parse failed")?;

        let finish_reason = resp
            .choices
            .first()
            .and_then(|c| c.finish_reason.clone());
        let content = resp
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(AiResponse {
            content,
            model_used: resp.model.unwrap_or_else(|| effective_model.clone()),
            finish_reason,
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
