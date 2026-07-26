use async_trait::async_trait;
use futures_util::StreamExt;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use keryx_domain::MessageRole;
use serde::Deserialize;
use serde_json::{json, Value};

/// Configuration for an OpenAI-compatible Chat Completions HTTP API.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    /// Base URL including `/v1` (e.g. `https://api.openai.com/v1`).
    pub base_url: String,
    /// Bearer API key from env/secret file (never browser session cookies).
    pub api_key: String,
    /// Default model id.
    pub model: String,
    /// Provider name for errors/logs (`openai`, `grok`).
    pub provider_name: String,
}

impl OpenAiCompatibleConfig {
    #[must_use]
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            provider_name: "openai".into(),
        }
    }

    #[must_use]
    pub fn grok(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.x.ai/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            provider_name: "grok".into(),
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

/// Shared OpenAI-compatible HTTP client used by OpenAI and Grok adapters.
pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, ModelError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| ModelError::new(e.to_string()))?;
        Ok(Self { config, client })
    }

    fn chat_url(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{base}/chat/completions")
    }

    fn build_messages(&self, request: &ModelRequest) -> Vec<Value> {
        let mut messages = Vec::new();
        for msg in &request.transcript {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "user", // fold tool observations into user channel for v1
            };
            messages.push(json!({
                "role": role,
                "content": msg.content,
            }));
        }
        // Ensure the current goal is present if transcript is empty.
        if messages.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": request.goal,
            }));
        }
        messages
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let url = self.chat_url();
        let body = json!({
            "model": self.config.model,
            "messages": self.build_messages(&request),
            "stream": true,
        });

        let response = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ModelError::new(format!("{} request failed: {e}", self.config.provider_name))
            })?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ModelError::new(format!(
                "{} HTTP {status}: {text}",
                self.config.provider_name
            )));
        }

        let mut deltas = Vec::new();
        let mut content = String::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ModelError::new(e.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].trim_end_matches('\r').to_string();
                buffer = buffer[idx + 1..].to_string();
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(delta) = parsed
                            .choices
                            .first()
                            .and_then(|c| c.delta.content.as_ref())
                        {
                            if !delta.is_empty() {
                                deltas.push(delta.clone());
                                content.push_str(delta);
                            }
                        }
                    }
                }
            }
        }

        if content.is_empty() && deltas.is_empty() {
            return Err(ModelError::new(format!(
                "{} stream produced no content",
                self.config.provider_name
            )));
        }

        let tokens_used = content.chars().count() as u64;
        Ok(ModelResponse {
            content,
            deltas,
            tool_calls: Vec::new(),
            tokens_used: tokens_used.max(1),
        })
    }
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

/// OpenAI Model provider (API credentials only).
pub type OpenAiProvider = OpenAiCompatibleProvider;

/// Grok (xAI) Model provider reusing the OpenAI-compatible client shape.
pub type GrokProvider = OpenAiCompatibleProvider;

/// Construct a configured OpenAI provider.
pub fn openai_provider(
    api_key: impl Into<String>,
    model: impl Into<String>,
) -> Result<OpenAiProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig::openai(api_key, model))
}

/// Construct a configured Grok/xAI provider.
pub fn grok_provider(
    api_key: impl Into<String>,
    model: impl Into<String>,
) -> Result<GrokProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig::grok(api_key, model))
}
