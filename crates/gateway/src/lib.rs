//! Gateway adapters: messaging platforms → control-plane Sessions/Runs.
//!
//! Gateways never own the agent loop; they call app ports only (Seam 3).

mod telegram_live;

pub use telegram_live::{run_telegram_long_poll, ChatAllowlist, ChatSessionMap, TelegramBotApi};

use async_trait::async_trait;
use keryx_app::{AppError, ControlPlaneService};
use keryx_domain::{Principal, Run, RunOrigin, Session, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

/// Failures in Gateway secret validation or mapping.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("gateway secrets missing or invalid")]
    SecretsFailClosed,
    #[error("control plane: {0}")]
    Control(#[from] AppError),
    #[error("gateway: {0}")]
    Other(String),
}

/// Inbound message normalized from a platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub platform: String,
    pub chat_id: String,
    pub text: String,
    pub external_user: String,
}

/// Outbound reply to a platform chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub text: String,
}

/// Platform protocol adapter (Telegram Bot API fixtures, Discord wire fixtures).
#[async_trait]
pub trait PlatformTransport: Send + Sync {
    async fn send(&self, msg: OutboundMessage) -> Result<(), GatewayError>;
}

/// Shared Gateway orchestration over the control plane (no shadow agent loop).
pub struct GatewayRuntime<C, T> {
    control: Arc<C>,
    transport: Arc<T>,
    principal: Principal,
    bot_secret: String,
    /// chat_id → SessionId for multi-turn continuity.
    chat_sessions: Mutex<HashMap<String, SessionId>>,
}

impl<C, T> GatewayRuntime<C, T>
where
    C: ControlPlaneService,
    T: PlatformTransport,
{
    /// Fail closed when bot secret is empty/invalid.
    pub fn new(
        control: Arc<C>,
        transport: Arc<T>,
        principal: Principal,
        bot_secret: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        let bot_secret = bot_secret.into();
        if bot_secret.trim().is_empty() || bot_secret == "invalid" {
            return Err(GatewayError::SecretsFailClosed);
        }
        Ok(Self {
            control,
            transport,
            principal,
            bot_secret,
            chat_sessions: Mutex::new(HashMap::new()),
        })
    }

    /// Handle one inbound update: create/continue Session, start Run with gateway origin.
    pub async fn handle_inbound(
        &self,
        msg: InboundMessage,
        provided_secret: &str,
    ) -> Result<Run, GatewayError> {
        if provided_secret != self.bot_secret {
            return Err(GatewayError::SecretsFailClosed);
        }
        let origin = RunOrigin::gateway(&msg.platform);
        let session_id = {
            let mut map = self.chat_sessions.lock().await;
            if let Some(id) = map.get(&msg.chat_id) {
                *id
            } else {
                let session: Session = self.control.create_session(self.principal.clone()).await?;
                map.insert(msg.chat_id.clone(), session.id);
                session.id
            }
        };
        let run = self
            .control
            .start_run_with_origin(
                self.principal.clone(),
                session_id,
                msg.text,
                origin,
                None,
                None,
            )
            .await?;
        Ok(run)
    }

    /// Deliver a Run outcome back to the originating chat.
    pub async fn deliver_outcome(
        &self,
        chat_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<(), GatewayError> {
        self.transport
            .send(OutboundMessage {
                chat_id: chat_id.into(),
                text: text.into(),
            })
            .await
    }
}

/// Telegram wire mapping helpers (fixture payloads; no live Bot API).
pub mod telegram {
    use super::*;
    use serde_json::Value;

    /// Parse a Telegram update fixture into [`InboundMessage`].
    pub fn parse_update(update: &Value) -> Result<InboundMessage, GatewayError> {
        let message = update
            .get("message")
            .ok_or_else(|| GatewayError::Other("missing message".into()))?;
        let chat_id = message
            .get("chat")
            .and_then(|c| c.get("id"))
            .map(|id| id.to_string())
            .ok_or_else(|| GatewayError::Other("missing chat.id".into()))?;
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let external_user = message
            .get("from")
            .and_then(|f| f.get("id"))
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".into());
        Ok(InboundMessage {
            platform: "telegram".into(),
            chat_id,
            text,
            external_user,
        })
    }
}

/// Discord wire mapping helpers (fixture payloads).
pub mod discord {
    use super::*;
    use serde_json::Value;

    pub fn parse_message_create(event: &Value) -> Result<InboundMessage, GatewayError> {
        let d = event
            .get("d")
            .ok_or_else(|| GatewayError::Other("missing d".into()))?;
        let chat_id = d
            .get("channel_id")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::Other("missing channel_id".into()))?
            .to_string();
        let text = d
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let external_user = d
            .get("author")
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        Ok(InboundMessage {
            platform: "discord".into(),
            chat_id,
            text,
            external_user,
        })
    }
}

/// Recording transport for Seam 3 fixtures.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    pub sent: std::sync::Mutex<Vec<OutboundMessage>>,
}

#[async_trait]
impl PlatformTransport for RecordingTransport {
    async fn send(&self, msg: OutboundMessage) -> Result<(), GatewayError> {
        if let Ok(mut s) = self.sent.lock() {
            s.push(msg);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keryx_app::ControlPlane;
    use keryx_domain::PrincipalId;
    use keryx_model::FakeModelProvider;
    use keryx_storage::InMemorySessionStore;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn telegram_inbound_maps_to_gateway_origin() {
        let store = Arc::new(InMemorySessionStore::new());
        let model = Arc::new(FakeModelProvider::with_fixed_content("tg reply"));
        let control = Arc::new(ControlPlane::new(store, model));
        let transport = Arc::new(RecordingTransport::default());
        let gw = GatewayRuntime::new(
            control,
            transport.clone(),
            Principal {
                id: PrincipalId::new("operator"),
            },
            "secret-bot-token",
        )
        .unwrap();

        let update = json!({
            "message": {
                "chat": { "id": 42 },
                "from": { "id": 7 },
                "text": "hello from phone"
            }
        });
        let inbound = telegram::parse_update(&update).unwrap();
        let run = gw
            .handle_inbound(inbound, "secret-bot-token")
            .await
            .unwrap();
        assert_eq!(run.origin.as_str(), "gateway:telegram");
        assert!(run.origin.is_reduced_trust());

        gw.deliver_outcome("42", "done").await.unwrap();
        assert_eq!(transport.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn bad_secret_fail_closed() {
        let store = Arc::new(InMemorySessionStore::new());
        let model = Arc::new(FakeModelProvider::with_fixed_content("x"));
        let control = Arc::new(ControlPlane::new(store, model));
        let transport = Arc::new(RecordingTransport::default());
        assert!(GatewayRuntime::new(
            control.clone(),
            transport.clone(),
            Principal {
                id: PrincipalId::new("op"),
            },
            "",
        )
        .is_err());
        let gw = GatewayRuntime::new(
            control,
            transport,
            Principal {
                id: PrincipalId::new("op"),
            },
            "good",
        )
        .unwrap();
        let msg = InboundMessage {
            platform: "telegram".into(),
            chat_id: "1".into(),
            text: "x".into(),
            external_user: "u".into(),
        };
        assert!(gw.handle_inbound(msg, "bad").await.is_err());
    }

    #[tokio::test]
    async fn discord_inbound_maps_origin() {
        let store = Arc::new(InMemorySessionStore::new());
        let model = Arc::new(FakeModelProvider::with_fixed_content("discord ok"));
        let control = Arc::new(ControlPlane::new(store, model));
        let transport = Arc::new(RecordingTransport::default());
        let gw = GatewayRuntime::new(
            control,
            transport,
            Principal {
                id: PrincipalId::new("op"),
            },
            "discord-token",
        )
        .unwrap();
        let event = json!({
            "t": "MESSAGE_CREATE",
            "d": {
                "channel_id": "chan-1",
                "content": "hi discord",
                "author": { "id": "user-9" }
            }
        });
        let inbound = discord::parse_message_create(&event).unwrap();
        let run = gw.handle_inbound(inbound, "discord-token").await.unwrap();
        assert_eq!(run.origin.as_str(), "gateway:discord");
    }
}
