//! Live Telegram Bot API transport + long-poll loop (not used in default CI).

use crate::{telegram, GatewayError, InboundMessage, OutboundMessage, PlatformTransport};
use async_trait::async_trait;
use keryx_app::ControlPlaneService;
use keryx_domain::{Principal, RunStatus, SessionId};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// HTTP Bot API client implementing [`PlatformTransport`].
pub struct TelegramBotApi {
    token: String,
    client: reqwest::Client,
    api_base: String,
}

impl TelegramBotApi {
    pub fn new(token: impl Into<String>) -> Result<Self, GatewayError> {
        let token = token.into();
        if token.trim().is_empty() {
            return Err(GatewayError::SecretsFailClosed);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| GatewayError::Other(e.to_string()))?;
        Ok(Self {
            token,
            client,
            api_base: "https://api.telegram.org".into(),
        })
    }

    fn method_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.api_base, self.token, method)
    }

    /// `getMe` — validates token without sending chat traffic.
    pub async fn get_me(&self) -> Result<Value, GatewayError> {
        let resp = self
            .client
            .get(self.method_url("getMe"))
            .send()
            .await
            .map_err(|e| GatewayError::Other(format!("getMe: {e}")))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Other(format!("getMe json: {e}")))?;
        if !status.is_success() || body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Other(format!("getMe failed: {body}")));
        }
        Ok(body["result"].clone())
    }

    /// Long-poll `getUpdates`.
    pub async fn get_updates(
        &self,
        offset: i64,
        timeout_secs: u64,
    ) -> Result<Vec<Value>, GatewayError> {
        let resp = self
            .client
            .get(self.method_url("getUpdates"))
            .query(&[
                ("offset", offset.to_string()),
                ("timeout", timeout_secs.to_string()),
                ("allowed_updates", r#"["message"]"#.to_string()),
            ])
            .send()
            .await
            .map_err(|e| GatewayError::Other(format!("getUpdates: {e}")))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Other(format!("getUpdates json: {e}")))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Other(format!("getUpdates not ok: {body}")));
        }
        let arr = body
            .get("result")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(arr)
    }
}

#[async_trait]
impl PlatformTransport for TelegramBotApi {
    async fn send(&self, msg: OutboundMessage) -> Result<(), GatewayError> {
        // Telegram limit ~4096 chars; truncate safely.
        let text = if msg.text.chars().count() > 4000 {
            let t: String = msg.text.chars().take(3990).collect();
            format!("{t}\n…[truncated]")
        } else {
            msg.text
        };
        let resp = self
            .client
            .post(self.method_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": msg.chat_id,
                "text": text,
            }))
            .send()
            .await
            .map_err(|e| GatewayError::Other(format!("sendMessage: {e}")))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| GatewayError::Other(format!("sendMessage json: {e}")))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(GatewayError::Other(format!("sendMessage failed: {body}")));
        }
        Ok(())
    }
}

/// Chat allowlist: empty means allow-all (personal bot default); non-empty fail-closed.
#[derive(Debug, Clone, Default)]
pub struct ChatAllowlist {
    ids: HashSet<String>,
}

impl ChatAllowlist {
    pub fn from_env_csv(raw: Option<String>) -> Self {
        let ids = raw
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self { ids }
    }

    pub fn allows(&self, chat_id: &str) -> bool {
        self.ids.is_empty() || self.ids.contains(chat_id)
    }

    pub fn is_open(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Durable-enough chat → Session mapping for the process lifetime.
#[derive(Debug, Default)]
pub struct ChatSessionMap {
    inner: Mutex<HashMap<String, SessionId>>,
}

impl ChatSessionMap {
    pub async fn get_or_insert<F, Fut>(
        &self,
        chat_id: &str,
        create: F,
    ) -> Result<SessionId, GatewayError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<SessionId, GatewayError>>,
    {
        {
            let guard = self.inner.lock().await;
            if let Some(id) = guard.get(chat_id) {
                return Ok(*id);
            }
        }
        let id = create().await?;
        let mut guard = self.inner.lock().await;
        // Another task may have raced; prefer existing.
        Ok(*guard.entry(chat_id.to_string()).or_insert(id))
    }
}

/// Process one inbound Telegram message through control plane and reply with Run result.
pub async fn handle_message_e2e<C: ControlPlaneService + 'static>(
    control: Arc<C>,
    transport: Arc<TelegramBotApi>,
    principal: Principal,
    sessions: Arc<ChatSessionMap>,
    msg: InboundMessage,
    max_wait: Duration,
) -> Result<(), GatewayError> {
    let chat_id = msg.chat_id.clone();
    let text = msg.text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    // Typing / ack
    let _ = transport
        .send(OutboundMessage {
            chat_id: chat_id.clone(),
            text: "… working".into(),
        })
        .await;

    let session_id = sessions
        .get_or_insert(&chat_id, || {
            let control = Arc::clone(&control);
            let principal = principal.clone();
            async move {
                let s = control.create_session(principal).await?;
                Ok(s.id)
            }
        })
        .await?;

    let run = control
        .start_run_with_origin(
            principal,
            session_id,
            text,
            keryx_domain::RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await?;

    info!(run_id = %run.id, chat_id = %chat_id, "telegram gateway started Run");

    let deadline = tokio::time::Instant::now() + max_wait;
    let outcome = loop {
        let r = control.get_run(run.id).await?;
        if r.status.is_terminal() {
            break r;
        }
        if tokio::time::Instant::now() > deadline {
            let _ = control.cancel_run(run.id).await;
            let _ = transport
                .send(OutboundMessage {
                    chat_id: chat_id.clone(),
                    text: "timed out waiting for the agent Run".into(),
                })
                .await;
            return Err(GatewayError::Other("run wait timeout".into()));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };

    let reply = match outcome.status {
        RunStatus::Completed => outcome
            .result
            .unwrap_or_else(|| "(completed with empty result)".into()),
        RunStatus::Failed => format!(
            "failed: {}",
            outcome.result.unwrap_or_else(|| "unknown".into())
        ),
        RunStatus::Cancelled => "cancelled".into(),
        RunStatus::Interrupted => "interrupted (worker restart)".into(),
        RunStatus::Active => "still active".into(),
    };

    transport
        .send(OutboundMessage {
            chat_id,
            text: reply,
        })
        .await?;
    Ok(())
}

/// Background long-poll loop. Spawns per-message tasks so one slow Run does not block polling.
pub async fn run_telegram_long_poll<C: ControlPlaneService + 'static>(
    control: Arc<C>,
    token: String,
    principal: Principal,
    allowlist: ChatAllowlist,
    max_wait: Duration,
) -> Result<(), GatewayError> {
    let api = Arc::new(TelegramBotApi::new(token)?);
    let me = api.get_me().await?;
    let bot_username = me
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string();
    info!(%bot_username, allow_open = allowlist.is_open(), "telegram gateway long-poll starting");
    if allowlist.is_open() {
        warn!("KERYX_TELEGRAM_ALLOWED_CHAT_IDS empty — any chat can talk to this bot");
    }

    // Drop any pending webhook so getUpdates works.
    let _ = api
        .client
        .get(api.method_url("deleteWebhook"))
        .query(&[("drop_pending_updates", "false")])
        .send()
        .await;

    let sessions = Arc::new(ChatSessionMap::default());
    let mut offset: i64 = 0;

    loop {
        let updates = match api.get_updates(offset, 25).await {
            Ok(u) => u,
            Err(e) => {
                warn!(error = %e, "telegram getUpdates failed; retrying");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        for update in updates {
            let update_id = update.get("update_id").and_then(Value::as_i64).unwrap_or(0);
            offset = offset.max(update_id + 1);

            let inbound = match telegram::parse_update(&update) {
                Ok(m) => m,
                Err(_) => continue, // non-text / no message
            };
            if inbound.text.trim().is_empty() {
                continue;
            }
            if !allowlist.allows(&inbound.chat_id) {
                warn!(chat_id = %inbound.chat_id, "telegram chat not allowlisted; ignored");
                let _ = api
                    .send(OutboundMessage {
                        chat_id: inbound.chat_id.clone(),
                        text: "this bot is private (chat not allowlisted)".into(),
                    })
                    .await;
                continue;
            }

            let control = Arc::clone(&control);
            let api = Arc::clone(&api);
            let principal = principal.clone();
            let sessions = Arc::clone(&sessions);
            tokio::spawn(async move {
                if let Err(e) =
                    handle_message_e2e(control, api, principal, sessions, inbound, max_wait).await
                {
                    warn!(error = %e, "telegram handle_message_e2e failed");
                }
            });
        }
    }
}

// Re-export for GatewayRuntime secret check compatibility tests.
impl TelegramBotApi {
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Helper used by unit tests with a custom base URL (mock HTTP).
#[cfg(test)]
impl TelegramBotApi {
    pub fn with_base(
        token: impl Into<String>,
        api_base: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        let mut s = Self::new(token)?;
        s.api_base = api_base.into();
        Ok(s)
    }
}
