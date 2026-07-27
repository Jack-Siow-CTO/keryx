//! ChatGPT **subscription** provider via Codex Responses wire
//! (`https://chatgpt.com/backend-api/codex/responses`).
//!
//! Uses the same OAuth access token as `codex login` / `~/.codex/auth.json`
//! (Plus/Pro plan usage), **not** an OpenAI platform API key.
//!
//! Unofficial relative to public Platform docs; fixture-locked. Prefer official
//! API keys when available. ToS risk remains operator-owned (ADR 0010).

use super::auth::ConsumerWebConfig;
use super::error::{map_http_status, redact_secrets};
use super::parse::parse_sse_text_deltas;
use async_trait::async_trait;
use base64::Engine;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use keryx_domain::MessageRole;
use serde_json::{json, Value};
use uuid::Uuid;

/// ChatGPT subscription Model provider (`openai_codex`).
pub struct ChatGptCodexProvider {
    config: ConsumerWebConfig,
    client: reqwest::Client,
    /// Reasoning effort for Codex Responses (`low` \| `medium` \| `high`, etc.).
    reasoning_effort: Option<String>,
}

impl ChatGptCodexProvider {
    pub fn new(config: ConsumerWebConfig) -> Result<Self, ModelError> {
        Self::new_with_reasoning(config, None)
    }

    pub fn new_with_reasoning(
        config: ConsumerWebConfig,
        reasoning_effort: Option<String>,
    ) -> Result<Self, ModelError> {
        let token_missing = match config.auth.bearer_token.as_ref() {
            None => true,
            Some(s) => s.is_empty(),
        };
        if token_missing {
            return Err(ModelError::new(
                "openai_codex: missing ChatGPT access token (from `codex login` / auth.json)",
            ));
        }
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .build()
            .map_err(|e| ModelError::new(e.to_string()))?;
        Ok(Self {
            config,
            client,
            reasoning_effort,
        })
    }

    #[must_use]
    pub fn default_model(&self) -> &str {
        &self.config.model
    }

    /// Build from a raw access token (registry path). Loads account id / headers from env.
    pub fn from_access_token(token: String) -> Result<Self, ModelError> {
        let account_id = super::auth::load_secret_pair("CHATGPT_ACCOUNT_ID")
            .map_err(ModelError::new)?
            .or_else(|| extract_account_id_from_jwt(&token));

        let mut extra =
            super::auth::read_headers_file("CHATGPT_WEB_HEADERS_FILE").map_err(ModelError::new)?;
        if let Some(acct) = account_id {
            extra
                .entry("chatgpt-account-id".into())
                .or_insert_with(|| acct.clone());
            extra.entry("openai-account-id".into()).or_insert(acct);
        }
        extra
            .entry("openai-beta".into())
            .or_insert_with(|| "responses=experimental".into());
        extra
            .entry("originator".into())
            .or_insert_with(|| "keryx".into());

        let models = std::env::var("CHATGPT_CODEX_MODELS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let auth = super::auth::ConsumerWebAuth {
            cookie_header: None,
            bearer_token: Some(token),
            extra_headers: extra,
        };
        // Default: gpt-5.6-sol on low reasoning (operator may override via env).
        let reasoning_effort = std::env::var("CHATGPT_CODEX_REASONING_EFFORT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| Some("low".into()));
        let config = ConsumerWebConfig {
            provider_name: "openai_codex".into(),
            base_url: std::env::var("CHATGPT_WEB_BASE_URL")
                .unwrap_or_else(|_| "https://chatgpt.com".into()),
            path: std::env::var("CHATGPT_CODEX_PATH")
                .unwrap_or_else(|_| "/backend-api/codex/responses".into()),
            model: std::env::var("CHATGPT_CODEX_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".into()),
            auth,
            user_agent: std::env::var("CHATGPT_WEB_USER_AGENT")
                .unwrap_or_else(|_| "keryx/0.1 (chatgpt-subscription)".into()),
            allowed_models: models,
        };
        Self::new_with_reasoning(config, reasoning_effort)
    }

    /// Build config from environment. Registers only when access token is present.
    pub fn from_env() -> Result<Option<Self>, String> {
        let token = super::auth::load_secret_pair("CHATGPT_CODEX_ACCESS_TOKEN")?
            .or(super::auth::load_secret_pair("CHATGPT_WEB_ACCESS_TOKEN")?)
            .filter(|t| !t.is_empty());
        let Some(token) = token else {
            return Ok(None);
        };
        Self::from_access_token(token)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    fn build_body(&self, request: &ModelRequest, model: &str) -> Value {
        let mut input: Vec<Value> = Vec::new();
        for msg in &request.transcript {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content_type = if role == "assistant" {
                "output_text"
            } else {
                "input_text"
            };
            input.push(json!({
                "type": "message",
                "role": role,
                "content": [{ "type": content_type, "text": msg.content }],
            }));
        }
        if input.is_empty() {
            input.push(json!({
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": request.goal }],
            }));
        }
        let mut body = json!({
            "model": model,
            "input": input,
            "stream": true,
            "store": false,
        });
        if let Some(effort) = &self.reasoning_effort {
            body["reasoning"] = json!({ "effort": effort });
        }
        body
    }
}

fn extract_account_id_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| {
            // Some encoders include padding.
            base64::engine::general_purpose::URL_SAFE.decode(payload)
        })
        .ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    // Claim key contains slashes; do not use JSON Pointer.
    value
        .get("https://api.openai.com/auth")
        .and_then(|v| v.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[async_trait]
impl ModelProvider for ChatGptCodexProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let model = self
            .config
            .resolve_model(request.model.as_deref())
            .map_err(ModelError::new)?;
        let secrets = self.config.auth.secret_values();
        let url = self.config.chat_url();
        let session_id = Uuid::new_v4().to_string();

        let mut builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .header("session-id", &session_id)
            .header("x-client-request-id", &session_id)
            .json(&self.build_body(&request, &model));

        if let Some(token) = &self.config.auth.bearer_token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        for (k, v) in &self.config.auth.extra_headers {
            builder = builder.header(k, v);
        }

        let response = builder.send().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("openai_codex request failed: {e}"),
                &secrets,
            ))
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(map_http_status("openai_codex", status, &secrets));
        }

        let body = response.text().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("openai_codex read body failed: {e}"),
                &secrets,
            ))
        })?;

        let deltas = parse_sse_text_deltas(&body);
        let content = deltas.concat();
        if content.is_empty() {
            return Err(ModelError::new(
                "openai_codex: empty completion (wire format may have changed, or model rejected)",
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_account_id_shape() {
        // Header.payload.sig — payload is {"https://api.openai.com/auth":{"chatgpt_account_id":"acct-1"}}
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct-1"}}"#);
        let jwt = format!("hdr.{payload}.sig");
        assert_eq!(extract_account_id_from_jwt(&jwt).as_deref(), Some("acct-1"));
    }
}
