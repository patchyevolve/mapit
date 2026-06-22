use crate::provider::{AiProvider, AiRequest, AiResponse, ModelInfo};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

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
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let http_resp = client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .context("OpenAI-compatible list_models: request failed")?;
        let status = http_resp.status();
        let body_text = http_resp
            .text()
            .context("OpenAI-compatible list_models: failed to read response body")?;
        if !status.is_success() {
            let api_err = serde_json::from_str::<serde_json::Value>(&body_text)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            v.get("message")
                                .and_then(|m| m.as_str())
                                .map(|s| s.to_string())
                        })
                })
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(anyhow::anyhow!("{}", api_err));
        }
        let resp: OpenAiModelsResponse = serde_json::from_str(&body_text).with_context(|| {
            let snippet = &body_text[..body_text.len().min(300)];
            format!("OpenAI-compatible list_models: unexpected response shape: {snippet}")
        })?;
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

        // Use the provider's stored model unless the caller explicitly set one
        let effective_model = if request.model.is_empty()
            || request.model == "openai-compatible"
            || request.model == "ollama"
        {
            self.model.as_str()
        } else {
            request.model.as_str()
        };

        let body = serde_json::json!({
            "model": effective_model,
            "messages": messages,
            "temperature": 0.1,
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;

        let http_resp = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .context("OpenAI-compatible complete: request failed")?;

        let status = http_resp.status();
        let body_text = http_resp
            .text()
            .context("OpenAI-compatible complete: failed to read response body")?;

        if !status.is_success() {
            // Extract a readable message from the error body (OpenAI error shape)
            let api_err = serde_json::from_str::<serde_json::Value>(&body_text)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            v.get("message")
                                .and_then(|m| m.as_str())
                                .map(|s| s.to_string())
                        })
                })
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(anyhow::anyhow!("{} (model: {})", api_err, effective_model));
        }

        let resp: ChatCompletionResponse = serde_json::from_str(&body_text).with_context(|| {
            let snippet = &body_text[..body_text.len().min(300)];
            format!("OpenAI-compatible complete: unexpected response shape: {snippet}")
        })?;

        let finish_reason = resp.choices.first().and_then(|c| c.finish_reason.clone());
        let content = resp
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(AiResponse {
            content,
            model_used: resp.model.unwrap_or_else(|| effective_model.to_string()),
            finish_reason,
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
