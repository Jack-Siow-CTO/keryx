//! Gateway adapters: messaging platforms → control-plane Sessions/Runs.
//!
//! Gateways never own the agent loop; they call app ports only (Seam 3).
//! Telegram Approvals: notify + in-chat Approve/Deny as operator Principal (#77 / #68).

mod telegram_live;

pub use telegram_live::{run_telegram_long_poll, ChatAllowlist, ChatSessionMap, TelegramBotApi};

use async_trait::async_trait;
use keryx_app::{AppError, ControlPlaneService};
use keryx_domain::{
    Approval, ApprovalId, ApprovalStatus, Principal, Run, RunOrigin, Session, SessionId,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

/// Failures in Gateway secret validation or mapping.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("gateway secrets missing or invalid")]
    SecretsFailClosed,
    #[error("chat not allowlisted (fail closed)")]
    ChatNotAllowlisted,
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
    /// When set, transport attaches Approve/Deny controls for this Approval id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
}

impl OutboundMessage {
    #[must_use]
    pub fn text(chat_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            chat_id: chat_id.into(),
            text: text.into(),
            approval_id: None,
        }
    }

    #[must_use]
    pub fn approval_notice(
        chat_id: impl Into<String>,
        text: impl Into<String>,
        approval_id: impl Into<String>,
    ) -> Self {
        Self {
            chat_id: chat_id.into(),
            text: text.into(),
            approval_id: Some(approval_id.into()),
        }
    }
}

/// In-chat Approve/Deny decision from a Gateway surface (Telegram callback or command).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub chat_id: String,
    pub approval_id: String,
    pub approve: bool,
    /// Telegram `callback_query.id` when decision came from an inline button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_query_id: Option<String>,
}

/// Platform protocol adapter (Telegram Bot API fixtures, Discord wire fixtures).
#[async_trait]
pub trait PlatformTransport: Send + Sync {
    async fn send(&self, msg: OutboundMessage) -> Result<(), GatewayError>;

    /// Acknowledge a Telegram callback query (no-op for non-Telegram transports).
    async fn answer_callback(
        &self,
        _callback_query_id: &str,
        _text: &str,
    ) -> Result<(), GatewayError> {
        Ok(())
    }
}

/// Shared Gateway orchestration over the control plane (no shadow agent loop).
pub struct GatewayRuntime<C, T> {
    control: Arc<C>,
    transport: Arc<T>,
    principal: Principal,
    bot_secret: String,
    /// chat_id → SessionId for multi-turn continuity.
    chat_sessions: Mutex<HashMap<String, SessionId>>,
    /// Approval ids already notified this process (no Telegram-only lifecycle).
    notified_approvals: Mutex<HashSet<String>>,
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
            notified_approvals: Mutex::new(HashSet::new()),
        })
    }

    /// Operator Principal used for control-plane acts (Session, Run, Approval decide).
    #[must_use]
    pub fn principal(&self) -> &Principal {
        &self.principal
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
            .send(OutboundMessage::text(chat_id, text))
            .await
    }

    /// Notify `target_chats` about operator-wide pending Approvals not yet notified.
    ///
    /// Surfaces **all** pending Approvals (not only chat-mapped Sessions). Control plane remains
    /// system of record; this is a Gateway notify surface only. Does not add Telegram-only
    /// Approval timeouts.
    ///
    /// Returns how many notify messages were sent.
    pub async fn notify_pending_approvals(
        &self,
        target_chats: &[String],
    ) -> Result<usize, GatewayError> {
        if target_chats.is_empty() {
            return Ok(0);
        }
        let pending = self.control.list_approvals(true).await?;
        let mut sent = 0usize;
        for approval in pending {
            if approval.status != ApprovalStatus::Pending {
                continue;
            }
            let id = approval.id.to_string();
            {
                let mut notified = self.notified_approvals.lock().await;
                if !notified.insert(id.clone()) {
                    continue;
                }
            }
            let text = format_approval_notice(&approval);
            for chat_id in target_chats {
                self.transport
                    .send(OutboundMessage::approval_notice(
                        chat_id.clone(),
                        text.clone(),
                        id.clone(),
                    ))
                    .await?;
                sent += 1;
            }
        }
        Ok(sent)
    }

    /// Record Approve/Deny via control plane as the operator Principal.
    ///
    /// Fail closed for non-allowlisted chats. Does not escalate Policy: `approve` only resolves
    /// the pending Approval; Run origin / reduced Policy stay on the control plane.
    ///
    /// Always answers the Telegram callback (when present) and sends a short chat reply so a
    /// button tap never looks like a no-op — including already-decided Approvals.
    pub async fn handle_approval_decision(
        &self,
        decision: ApprovalDecision,
        allowlist: &ChatAllowlist,
        provided_secret: &str,
    ) -> Result<Approval, GatewayError> {
        if provided_secret != self.bot_secret {
            return Err(GatewayError::SecretsFailClosed);
        }
        if !allowlist.allows(&decision.chat_id) {
            let _ = feedback_decision(
                self.transport.as_ref(),
                &decision,
                "Not allowed",
                "this bot is private (chat not allowlisted)",
            )
            .await;
            return Err(GatewayError::ChatNotAllowlisted);
        }
        let approval_id = ApprovalId::from_str(&decision.approval_id)
            .map_err(|e| GatewayError::Other(format!("invalid approval id: {e}")))?;

        let result = if decision.approve {
            self.control
                .approve(self.principal.clone(), approval_id)
                .await
        } else {
            self.control.deny(self.principal.clone(), approval_id).await
        };

        match result {
            Ok(approval) => {
                let status = if decision.approve {
                    "approved"
                } else {
                    "denied"
                };
                let toast = if decision.approve {
                    "Approved"
                } else {
                    "Denied"
                };
                let _ = feedback_decision(
                    self.transport.as_ref(),
                    &decision,
                    toast,
                    &format!("Approval {status}: {} ({})", approval.action, approval.id),
                )
                .await;
                Ok(approval)
            }
            Err(AppError::ApprovalNotPending) => {
                let status = self
                    .control
                    .get_approval(approval_id)
                    .await
                    .ok()
                    .map(|a| match a.status {
                        ApprovalStatus::Approved => "already approved",
                        ApprovalStatus::Denied => "already denied",
                        ApprovalStatus::Pending => "not pending",
                    })
                    .unwrap_or("already decided or missing");
                let _ = feedback_decision(
                    self.transport.as_ref(),
                    &decision,
                    "Already decided",
                    &format!(
                        "Approval {} — {} ({})",
                        status,
                        decision.approval_id,
                        if decision.approve { "Approve" } else { "Deny" }
                    ),
                )
                .await;
                Err(GatewayError::Control(AppError::ApprovalNotPending))
            }
            Err(AppError::ApprovalNotFound) => {
                let _ = feedback_decision(
                    self.transport.as_ref(),
                    &decision,
                    "Not found",
                    &format!("Approval not found ({})", decision.approval_id),
                )
                .await;
                Err(GatewayError::Control(AppError::ApprovalNotFound))
            }
            Err(e) => {
                let _ = feedback_decision(
                    self.transport.as_ref(),
                    &decision,
                    "Failed",
                    &format!("Could not apply Approval: {e}"),
                )
                .await;
                Err(e.into())
            }
        }
    }

    /// Known chat ids with a Session mapping (for open allowlist notify fan-out).
    pub async fn known_chat_ids(&self) -> Vec<String> {
        self.chat_sessions.lock().await.keys().cloned().collect()
    }
}

/// Redacted notice body for a pending Approval (safe for chat).
#[must_use]
pub fn format_approval_notice(approval: &Approval) -> String {
    format!(
        "Needs you — Approval pending\n\
         action: {}\n\
         summary: {}\n\
         id: {}\n\
         run: {}\n\
         Approve or Deny below.",
        approval.action, approval.summary, approval.id, approval.run_id
    )
}

/// Always ack a callback (if any) and post a chat line — button taps must not be silent.
async fn feedback_decision<T: PlatformTransport + ?Sized>(
    transport: &T,
    decision: &ApprovalDecision,
    toast: &str,
    chat_text: &str,
) {
    if let Some(cb) = decision.callback_query_id.as_deref() {
        let _ = transport.answer_callback(cb, toast).await;
    }
    let _ = transport
        .send(OutboundMessage::text(
            decision.chat_id.clone(),
            chat_text.to_string(),
        ))
        .await;
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

    /// Parse inline-button callback_query into an [`ApprovalDecision`].
    ///
    /// Callback data wire: `a:<approval_id>` or `d:<approval_id>` (≤64 bytes).
    pub fn parse_callback_query(update: &Value) -> Result<ApprovalDecision, GatewayError> {
        let cq = update
            .get("callback_query")
            .ok_or_else(|| GatewayError::Other("missing callback_query".into()))?;
        let chat_id = cq
            .get("message")
            .and_then(|m| m.get("chat"))
            .and_then(|c| c.get("id"))
            .map(|id| id.to_string())
            .ok_or_else(|| GatewayError::Other("missing callback chat.id".into()))?;
        let data = cq
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::Other("missing callback data".into()))?;
        let callback_query_id = cq.get("id").and_then(Value::as_str).map(str::to_string);
        parse_decision_data(data, chat_id, callback_query_id)
    }

    /// Parse text command `/approve <id>` or `/deny <id>` (and bare `approve`/`deny` forms).
    pub fn parse_decision_command(msg: &InboundMessage) -> Option<ApprovalDecision> {
        let text = msg.text.trim();
        let (approve, rest) = if let Some(r) = text.strip_prefix("/approve") {
            (true, r)
        } else {
            let r = text.strip_prefix("/deny")?;
            (false, r)
        };
        let id = rest.trim();
        if id.is_empty() || ApprovalId::from_str(id).is_err() {
            return None;
        }
        Some(ApprovalDecision {
            chat_id: msg.chat_id.clone(),
            approval_id: id.to_string(),
            approve,
            callback_query_id: None,
        })
    }

    fn parse_decision_data(
        data: &str,
        chat_id: String,
        callback_query_id: Option<String>,
    ) -> Result<ApprovalDecision, GatewayError> {
        let (approve, id) = if let Some(id) = data.strip_prefix("a:") {
            (true, id)
        } else if let Some(id) = data.strip_prefix("d:") {
            (false, id)
        } else {
            return Err(GatewayError::Other(format!(
                "unknown callback data: {data}"
            )));
        };
        if ApprovalId::from_str(id).is_err() {
            return Err(GatewayError::Other(format!("invalid approval id: {id}")));
        }
        Ok(ApprovalDecision {
            chat_id,
            approval_id: id.to_string(),
            approve,
            callback_query_id,
        })
    }

    /// Inline keyboard JSON for Telegram `reply_markup` (Approve / Deny).
    #[must_use]
    pub fn approval_reply_markup(approval_id: &str) -> Value {
        serde_json::json!({
            "inline_keyboard": [[
                { "text": "Approve", "callback_data": format!("a:{approval_id}") },
                { "text": "Deny", "callback_data": format!("d:{approval_id}") }
            ]]
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
    pub callbacks: std::sync::Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl PlatformTransport for RecordingTransport {
    async fn send(&self, msg: OutboundMessage) -> Result<(), GatewayError> {
        if let Ok(mut s) = self.sent.lock() {
            s.push(msg);
        }
        Ok(())
    }

    async fn answer_callback(
        &self,
        callback_query_id: &str,
        text: &str,
    ) -> Result<(), GatewayError> {
        if let Ok(mut c) = self.callbacks.lock() {
            c.push((callback_query_id.to_string(), text.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keryx_app::{ControlPlane, SessionStore};
    use keryx_domain::{PrincipalId, RunId};
    use keryx_model::FakeModelProvider;
    use keryx_storage::InMemorySessionStore;
    use serde_json::json;
    use std::sync::Arc;

    fn operator() -> Principal {
        Principal {
            id: PrincipalId::new("operator"),
        }
    }

    type TestGw =
        GatewayRuntime<ControlPlane<InMemorySessionStore, FakeModelProvider>, RecordingTransport>;
    type TestControl = ControlPlane<InMemorySessionStore, FakeModelProvider>;

    fn harness() -> (
        Arc<InMemorySessionStore>,
        Arc<TestControl>,
        Arc<RecordingTransport>,
        TestGw,
    ) {
        let store = Arc::new(InMemorySessionStore::new());
        let model = Arc::new(FakeModelProvider::with_fixed_content("tg reply"));
        let control = Arc::new(ControlPlane::new(store.clone(), model));
        let transport = Arc::new(RecordingTransport::default());
        let gw = GatewayRuntime::new(
            control.clone(),
            transport.clone(),
            operator(),
            "secret-bot-token",
        )
        .unwrap();
        (store, control, transport, gw)
    }

    async fn seed_pending(store: &InMemorySessionStore, action: &str, summary: &str) -> Approval {
        let approval =
            Approval::pending(RunId::new(), PrincipalId::new("operator"), action, summary);
        store.create_approval(approval.clone()).await.unwrap();
        approval
    }

    #[tokio::test]
    async fn telegram_inbound_maps_to_gateway_origin() {
        let (_store, _control, transport, gw) = harness();

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
        let (_store, _control, _transport, gw) = {
            let store = Arc::new(InMemorySessionStore::new());
            let model = Arc::new(FakeModelProvider::with_fixed_content("discord ok"));
            let control = Arc::new(ControlPlane::new(store.clone(), model));
            let transport = Arc::new(RecordingTransport::default());
            let gw = GatewayRuntime::new(
                control.clone(),
                transport.clone(),
                Principal {
                    id: PrincipalId::new("op"),
                },
                "discord-token",
            )
            .unwrap();
            (store, control, transport, gw)
        };
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

    #[tokio::test]
    async fn notify_pending_sends_approve_deny_to_allowlisted_chats() {
        let (store, _control, transport, gw) = harness();
        let a = seed_pending(&store, "write_file", "path=SOUL.md").await;

        let n = gw
            .notify_pending_approvals(&["42".into(), "99".into()])
            .await
            .unwrap();
        assert_eq!(n, 2);

        {
            let sent = transport.sent.lock().unwrap();
            assert_eq!(sent.len(), 2);
            for msg in sent.iter() {
                assert_eq!(msg.approval_id.as_deref(), Some(a.id.to_string().as_str()));
                assert!(msg.text.contains("Needs you"));
                assert!(msg.text.contains("write_file"));
                assert!(msg.text.contains(&a.id.to_string()));
            }
        }
        // Second poll does not re-notify the same Approval.
        let n2 = gw.notify_pending_approvals(&["42".into()]).await.unwrap();
        assert_eq!(n2, 0);
    }

    #[tokio::test]
    async fn operator_wide_queue_not_only_chat_mapped_sessions() {
        let (store, _control, transport, gw) = harness();
        // Pending Approvals with no Session/chat mapping still surface.
        let a1 = seed_pending(&store, "shell_exec", "rm -rf").await;
        let a2 = seed_pending(&store, "skill_manage", "create skill").await;

        let n = gw.notify_pending_approvals(&["42".into()]).await.unwrap();
        assert_eq!(n, 2);
        let sent = transport.sent.lock().unwrap();
        let ids: HashSet<_> = sent.iter().filter_map(|m| m.approval_id.clone()).collect();
        assert!(ids.contains(&a1.id.to_string()));
        assert!(ids.contains(&a2.id.to_string()));
    }

    #[tokio::test]
    async fn approve_via_callback_records_operator_principal() {
        let (store, control, transport, gw) = harness();
        let a = seed_pending(&store, "write_file", "path=x").await;
        let allow = ChatAllowlist::from_ids(["42"]);

        let update = json!({
            "callback_query": {
                "id": "cq-1",
                "data": format!("a:{}", a.id),
                "message": { "chat": { "id": 42 } }
            }
        });
        let decision = telegram::parse_callback_query(&update).unwrap();
        let resolved = gw
            .handle_approval_decision(decision, &allow, "secret-bot-token")
            .await
            .unwrap();

        assert_eq!(resolved.status, ApprovalStatus::Approved);
        assert_eq!(
            resolved.decided_by.as_ref().map(|p| p.to_string()),
            Some("operator".into())
        );
        // Control plane is system of record — re-read via service.
        let listed = control.get_approval(a.id).await.unwrap();
        assert_eq!(listed.status, ApprovalStatus::Approved);
        assert_eq!(
            listed.decided_by.as_ref().map(|p| p.to_string()),
            Some("operator".into())
        );
        assert_eq!(transport.callbacks.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn deny_via_command_fails_closed() {
        let (store, control, _transport, gw) = harness();
        let a = seed_pending(&store, "write_file", "path=x").await;
        let allow = ChatAllowlist::from_ids(["42"]);

        let inbound = InboundMessage {
            platform: "telegram".into(),
            chat_id: "42".into(),
            text: format!("/deny {}", a.id),
            external_user: "7".into(),
        };
        let decision = telegram::parse_decision_command(&inbound).unwrap();
        let resolved = gw
            .handle_approval_decision(decision, &allow, "secret-bot-token")
            .await
            .unwrap();
        assert_eq!(resolved.status, ApprovalStatus::Denied);
        assert_eq!(
            control.get_approval(a.id).await.unwrap().status,
            ApprovalStatus::Denied
        );
    }

    #[tokio::test]
    async fn non_allowlisted_decide_fail_closed() {
        let (store, control, transport, gw) = harness();
        let a = seed_pending(&store, "write_file", "path=x").await;
        let allow = ChatAllowlist::from_ids(["42"]);

        let decision = ApprovalDecision {
            chat_id: "evil".into(),
            approval_id: a.id.to_string(),
            approve: true,
            callback_query_id: None,
        };
        let err = gw
            .handle_approval_decision(decision, &allow, "secret-bot-token")
            .await
            .unwrap_err();
        assert!(matches!(err, GatewayError::ChatNotAllowlisted));
        // Still pending — control plane unchanged.
        assert_eq!(
            control.get_approval(a.id).await.unwrap().status,
            ApprovalStatus::Pending
        );
        // Fail-closed still posts feedback so the button is not silent.
        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].text.contains("not allowlisted"));
    }

    #[tokio::test]
    async fn already_decided_callback_answers_and_explains() {
        let (store, control, transport, gw) = harness();
        let a = seed_pending(&store, "write_file", "path=x").await;
        let allow = ChatAllowlist::from_ids(["42"]);
        // First decide succeeds.
        control
            .approve(operator(), a.id)
            .await
            .expect("first approve");

        let update = json!({
            "callback_query": {
                "id": "cq-stale",
                "data": format!("a:{}", a.id),
                "message": { "chat": { "id": 42 } }
            }
        });
        let decision = telegram::parse_callback_query(&update).unwrap();
        let err = gw
            .handle_approval_decision(decision, &allow, "secret-bot-token")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            GatewayError::Control(AppError::ApprovalNotPending)
        ));
        // Toast + chat line so the tap is not silent.
        assert_eq!(transport.callbacks.lock().unwrap().len(), 1);
        assert_eq!(transport.callbacks.lock().unwrap()[0].1, "Already decided");
        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].text.contains("already approved") || sent[0].text.contains("already decided"),
            "got {}",
            sent[0].text
        );
    }

    #[tokio::test]
    async fn approve_does_not_change_gateway_run_origin() {
        // Approve resolves the Approval only; Run origin stays reduced-trust gateway.
        let (store, control, _transport, gw) = harness();
        let run = gw
            .handle_inbound(
                InboundMessage {
                    platform: "telegram".into(),
                    chat_id: "42".into(),
                    text: "goal".into(),
                    external_user: "7".into(),
                },
                "secret-bot-token",
            )
            .await
            .unwrap();
        assert!(run.origin.is_reduced_trust());

        let approval =
            Approval::pending(run.id, PrincipalId::new("operator"), "write_file", "path=x");
        store.create_approval(approval.clone()).await.unwrap();
        let allow = ChatAllowlist::from_ids(["42"]);
        let decision = ApprovalDecision {
            chat_id: "42".into(),
            approval_id: approval.id.to_string(),
            approve: true,
            callback_query_id: None,
        };
        gw.handle_approval_decision(decision, &allow, "secret-bot-token")
            .await
            .unwrap();

        let after = control.get_run(run.id).await.unwrap();
        assert_eq!(after.origin.as_str(), "gateway:telegram");
        assert!(after.origin.is_reduced_trust());
    }

    #[tokio::test]
    async fn callback_data_and_markup_round_trip() {
        let id = ApprovalId::new();
        let markup = telegram::approval_reply_markup(&id.to_string());
        let row = &markup["inline_keyboard"][0];
        assert_eq!(row[0]["callback_data"], format!("a:{id}"));
        assert_eq!(row[1]["callback_data"], format!("d:{id}"));

        let update = json!({
            "callback_query": {
                "id": "cq",
                "data": format!("d:{id}"),
                "message": { "chat": { "id": 1 } }
            }
        });
        let d = telegram::parse_callback_query(&update).unwrap();
        assert!(!d.approve);
        assert_eq!(d.approval_id, id.to_string());
    }
}
