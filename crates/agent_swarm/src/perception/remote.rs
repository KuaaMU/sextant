//! RemoteLlm: OpenAI-compatible API client for remote inference.
//!
//! Works with OpenRouter, OpenAI, vLLM, and any compatible endpoint.
//! Default config targets OpenRouter.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::llm::LlmBackend;

/// Configuration for a remote LLM endpoint.
#[derive(Clone, Debug)]
pub struct RemoteLlmConfig {
    /// API base URL (e.g., "https://openrouter.ai/api/v1").
    pub api_base: String,
    /// API key for authentication.
    pub api_key: String,
    /// Model ID (e.g., "qwen/qwen3-8b", "openai/gpt-4o-mini").
    pub model: String,
    /// Temperature for sampling.
    pub temperature: f32,
    /// Max tokens to generate.
    pub max_tokens: u32,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl RemoteLlmConfig {
    /// OpenRouter preset.
    pub fn openrouter(api_key: &str, model: &str) -> Self {
        Self {
            api_base: "https://openrouter.ai/api/v1".to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            temperature: 0.7,
            max_tokens: 512,
            timeout_secs: 30,
        }
    }

    /// Local vLLM / Ollama preset.
    pub fn local(port: u16, model: &str) -> Self {
        Self {
            api_base: format!("http://localhost:{}/v1", port),
            api_key: "not-needed".to_string(),
            model: model.to_string(),
            temperature: 0.7,
            max_tokens: 512,
            timeout_secs: 60,
        }
    }
}

impl Default for RemoteLlmConfig {
    fn default() -> Self {
        Self {
            api_base: "https://openrouter.ai/api/v1".to_string(),
            api_key: String::new(),
            model: "qwen/qwen3-8b".to_string(),
            temperature: 0.7,
            max_tokens: 512,
            timeout_secs: 30,
        }
    }
}

// ── OpenAI-compatible API types ─────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ApiError {
    error: Option<ApiErrorDetail>,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

// ── RemoteLlm backend ──────────────────────────────────────────────

/// Remote LLM backend using OpenAI-compatible chat completions API.
pub struct RemoteLlm {
    config: RemoteLlmConfig,
    client: reqwest::Client,
}

impl RemoteLlm {
    pub fn new(config: RemoteLlmConfig) -> Self {
        info!(
            "Creating RemoteLlm: model={} base={}",
            config.model, config.api_base
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }

    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.config.api_base)
    }
}

#[async_trait]
impl LlmBackend for RemoteLlm {
    async fn infer(&self, prompt: &str) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are a quantitative trading analyst. Always respond with valid JSON as instructed.".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                },
            ],
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        debug!(
            "RemoteLlm request: model={}, prompt_len={}, url={}",
            self.config.model,
            prompt.len(),
            self.chat_url()
        );

        let resp = self
            .client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            // Try to parse error message
            let msg = serde_json::from_str::<ApiError>(&body)
                .ok()
                .and_then(|e| e.error)
                .map(|e| e.message)
                .unwrap_or_else(|| body.clone());
            return Err(anyhow::anyhow!("API error ({}): {}", status, msg));
        }

        let chat_resp: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {} — body: {}", e, &body[..200.min(body.len())]))?;

        let content = chat_resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        debug!("RemoteLlm response: {} chars", content.len());
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openrouter_preset() {
        let config = RemoteLlmConfig::openrouter("sk-test", "qwen/qwen3-8b");
        assert_eq!(config.api_base, "https://openrouter.ai/api/v1");
        assert_eq!(config.model, "qwen/qwen3-8b");
    }

    #[test]
    fn test_local_preset() {
        let config = RemoteLlmConfig::local(8000, "qwen3-8b");
        assert_eq!(config.api_base, "http://localhost:8000/v1");
    }

    #[test]
    fn test_chat_url() {
        let llm = RemoteLlm::new(RemoteLlmConfig::openrouter("sk-test", "qwen/qwen3-8b"));
        assert_eq!(llm.chat_url(), "https://openrouter.ai/api/v1/chat/completions");
    }
}
