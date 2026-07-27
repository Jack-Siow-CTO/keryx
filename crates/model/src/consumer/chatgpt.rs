use super::auth::ConsumerWebConfig;
use super::error::{map_http_status, redact_secrets};
use super::parse::{parse_json_content, parse_sse_text_deltas};
use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use keryx_domain::MessageRole;
use serde_json::{json, Value};
use uuid::Uuid;

/// ChatGPT consumer-web Model provider (`openai_web`).
///
/// Wire format is unofficial (ADR 0010). Locked by Seam 2 fixtures; base URL/path overridable.
pub struct ChatGptWebProvider {
    config: ConsumerWebConfig,
    client: reqwest::Client,
}

impl ChatGptWebProvider {
    pub fn new(config: ConsumerWebConfig) -> Result<Self, ModelError> {
        if !config.auth.is_usable() {
            return Err(ModelError::new(
                "openai_web: missing session cookie or access token",
            ));
        }
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .build()
            .map_err(|e| ModelError::new(e.to_string()))?;
        Ok(Self { config, client })
    }

    /// Build config from environment (registers only when secrets present).
    pub fn from_env() -> Result<Option<Self>, String> {
        let token = super::auth::load_secret_pair("CHATGPT_WEB_ACCESS_TOKEN")?;
        let cookie = super::auth::load_secret_pair("CHATGPT_WEB_COOKIE")?;
        let auth = super::auth::ConsumerWebAuth {
            cookie_header: cookie,
            bearer_token: token,
            extra_headers: super::auth::read_headers_file("CHATGPT_WEB_HEADERS_FILE")?,
        };
        if !auth.is_usable() {
            return Ok(None);
        }
        let config = ConsumerWebConfig {
            provider_name: "openai_web".into(),
            base_url: std::env::var("CHATGPT_WEB_BASE_URL")
                .unwrap_or_else(|_| "https://chatgpt.com".into()),
            path: std::env::var("CHATGPT_WEB_PATH")
                .unwrap_or_else(|_| "/backend-api/conversation".into()),
            model: std::env::var("CHATGPT_WEB_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".into()),
            auth,
            user_agent: std::env::var("CHATGPT_WEB_USER_AGENT").unwrap_or_else(|_| {
                "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                    .into()
            }),
            allowed_models: Vec::new(),
        };
        Self::new(config).map(Some).map_err(|e| e.to_string())
    }

    fn build_body(&self, request: &ModelRequest, model: &str) -> Value {
        let mut messages = Vec::new();
        for msg in &request.transcript {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            messages.push(json!({
                "id": Uuid::new_v4().to_string(),
                "author": { "role": role },
                "content": {
                    "content_type": "text",
                    "parts": [msg.content],
                }
            }));
        }
        if messages.is_empty() {
            messages.push(json!({
                "id": Uuid::new_v4().to_string(),
                "author": { "role": "user" },
                "content": {
                    "content_type": "text",
                    "parts": [request.goal],
                }
            }));
        }
        json!({
            "action": "next",
            "messages": messages,
            "model": model,
            "parent_message_id": Uuid::new_v4().to_string(),
        })
    }
}

#[async_trait]
impl ModelProvider for ChatGptWebProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let model = self
            .config
            .resolve_model(request.model.as_deref())
            .map_err(ModelError::new)?;
        let secrets = self.config.auth.secret_values();
        let url = self.config.chat_url();
        let mut builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&self.build_body(&request, &model));

        if let Some(token) = &self.config.auth.bearer_token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        if let Some(cookie) = &self.config.auth.cookie_header {
            builder = builder.header("cookie", cookie);
        }
        for (k, v) in &self.config.auth.extra_headers {
            builder = builder.header(k, v);
        }

        let response = builder.send().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("openai_web request failed: {e}"),
                &secrets,
            ))
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(map_http_status("openai_web", status, &secrets));
        }

        let body = response.text().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("openai_web read body failed: {e}"),
                &secrets,
            ))
        })?;

        let content_type_stream = body.contains("data:");
        let (content, deltas) = if content_type_stream {
            let deltas = parse_sse_text_deltas(&body);
            let content = deltas.concat();
            (content, deltas)
        } else if let Some(content) = parse_json_content(&body) {
            (content.clone(), vec![content])
        } else {
            return Err(ModelError::new(
                "openai_web: could not parse response (wire format may have changed)",
            ));
        };

        if content.is_empty() {
            return Err(ModelError::new("openai_web: empty completion"));
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
