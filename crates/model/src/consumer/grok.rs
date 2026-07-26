use super::auth::ConsumerWebConfig;
use super::error::{map_http_status, redact_secrets};
use super::parse::{parse_json_content, parse_sse_text_deltas};
use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use keryx_domain::MessageRole;
use serde_json::{json, Value};

/// Grok consumer-web Model provider (`grok_web`).
///
/// Wire format is unofficial (ADR 0010). Locked by Seam 2 fixtures; base URL/path overridable.
pub struct GrokWebProvider {
    config: ConsumerWebConfig,
    client: reqwest::Client,
}

impl GrokWebProvider {
    pub fn new(config: ConsumerWebConfig) -> Result<Self, ModelError> {
        let cookie_missing = match config.auth.cookie_header.as_ref() {
            None => true,
            Some(s) => s.is_empty(),
        };
        if cookie_missing {
            return Err(ModelError::new("grok_web: missing session cookie"));
        }
        let client = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .build()
            .map_err(|e| ModelError::new(e.to_string()))?;
        Ok(Self { config, client })
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        let cookie = super::auth::load_secret_pair("GROK_WEB_COOKIE")?;
        let auth = super::auth::ConsumerWebAuth {
            cookie_header: cookie,
            bearer_token: None,
            extra_headers: super::auth::read_headers_file("GROK_WEB_HEADERS_FILE")?,
        };
        if !auth.is_usable() {
            return Ok(None);
        }
        let config = ConsumerWebConfig {
            provider_name: "grok_web".into(),
            base_url: std::env::var("GROK_WEB_BASE_URL")
                .unwrap_or_else(|_| "https://grok.com".into()),
            path: std::env::var("GROK_WEB_PATH")
                .unwrap_or_else(|_| "/rest/app-chat/conversations/new".into()),
            model: std::env::var("GROK_WEB_MODEL").unwrap_or_else(|_| "grok".into()),
            auth,
            user_agent: std::env::var("GROK_WEB_USER_AGENT").unwrap_or_else(|_| {
                "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                    .into()
            }),
        };
        Self::new(config).map(Some).map_err(|e| e.to_string())
    }

    fn build_body(&self, request: &ModelRequest) -> Value {
        // Flatten transcript into a single message payload the unofficial path accepts.
        let mut parts = Vec::new();
        for msg in &request.transcript {
            let label = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            parts.push(format!("{label}: {}", msg.content));
        }
        if parts.is_empty() {
            parts.push(request.goal.clone());
        } else {
            parts.push(format!("user: {}", request.goal));
        }
        json!({
            "message": parts.join("\n"),
            "model": self.config.model,
            "stream": true,
        })
    }
}

#[async_trait]
impl ModelProvider for GrokWebProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let secrets = self.config.auth.secret_values();
        let url = self.config.chat_url();
        let mut builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&self.build_body(&request));

        if let Some(cookie) = &self.config.auth.cookie_header {
            builder = builder.header("cookie", cookie);
        }
        for (k, v) in &self.config.auth.extra_headers {
            builder = builder.header(k, v);
        }

        let response = builder.send().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("grok_web request failed: {e}"),
                &secrets,
            ))
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(map_http_status("grok_web", status, &secrets));
        }

        let body = response.text().await.map_err(|e| {
            ModelError::new(redact_secrets(
                &format!("grok_web read body failed: {e}"),
                &secrets,
            ))
        })?;

        let (content, deltas) = if body.contains("data:") {
            let deltas = parse_sse_text_deltas(&body);
            (deltas.concat(), deltas)
        } else if let Some(content) = parse_json_content(&body) {
            (content.clone(), vec![content])
        } else {
            return Err(ModelError::new(
                "grok_web: could not parse response (wire format may have changed)",
            ));
        };

        if content.is_empty() {
            return Err(ModelError::new("grok_web: empty completion"));
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
