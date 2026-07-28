use crate::{RunId, SessionId};
use serde::{Deserialize, Serialize};

/// Role of a message in a Session Transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// Operator Soul / workspace Context files / standing instructions (not Memory, not Skill).
    System,
    User,
    Assistant,
    Tool,
}

/// Compact tool participation in Transcript (ADR 0025) — not unbounded dumps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolCompact {
    pub name: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
}

/// One message in a Session Transcript (structured for Console).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptMessage {
    /// Stable identity for paging / UI keys.
    pub id: String,
    /// Run that produced this message, when known.
    pub run_id: Option<RunId>,
    /// Unix seconds (UTC).
    pub created_at: i64,
    pub role: MessageRole,
    /// User/assistant prose, or short tool observation text.
    pub content: String,
    /// Present when `role == Tool` (or tool-linked rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolCompact>,
}

impl TranscriptMessage {
    fn stamp(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: new_message_id(),
            run_id: None,
            created_at: unix_now(),
            role,
            content: content.into(),
            tool: None,
        }
    }

    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::stamp(MessageRole::System, content)
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::stamp(MessageRole::User, content)
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::stamp(MessageRole::Assistant, content)
    }

    /// Compact Console fields (`tool`) plus full observation body in `content`
    /// for agent-loop continuity (ADR 0025: compact UI, not truncated model SoR).
    #[must_use]
    pub fn tool_compact(
        name: impl Into<String>,
        status: impl Into<String>,
        summary: impl Into<String>,
        full_content: impl Into<String>,
        artifact_refs: Vec<String>,
    ) -> Self {
        let name = name.into();
        let status = status.into();
        let summary = summary.into();
        let full_content = full_content.into();
        Self {
            id: new_message_id(),
            run_id: None,
            created_at: unix_now(),
            role: MessageRole::Tool,
            // Full tool observation for subsequent model turns / Seam 1 assertions.
            content: format!("{name}: {full_content}"),
            tool: Some(ToolCompact {
                name,
                status,
                summary,
                artifact_refs,
            }),
        }
    }

    #[must_use]
    pub fn with_run_id(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }
}

/// Ordered Session history available to subsequent Runs and Console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Transcript {
    pub session_id: Option<SessionId>,
    pub messages: Vec<TranscriptMessage>,
}

impl Transcript {
    #[must_use]
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id: Some(session_id),
            messages: Vec::new(),
        }
    }
}

/// Reverse-chronological page for Console (latest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptPage {
    pub session_id: String,
    /// Messages newest-first for this page.
    pub messages: Vec<TranscriptMessage>,
    /// Pass as `before` to load older history; null when no more.
    pub next_before: Option<String>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn new_message_id() -> String {
    // Time-sortable-ish id without extra deps: unix_ms-random.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let r: u32 = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        ms.hash(&mut h);
        std::thread::current().id().hash(&mut h);
        (h.finish() as u32) ^ (ms as u32)
    };
    format!("tm-{ms:x}-{r:08x}")
}
