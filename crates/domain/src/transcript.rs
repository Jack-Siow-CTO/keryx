use crate::SessionId;
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

/// One message in a Session Transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub role: MessageRole,
    pub content: String,
}

impl TranscriptMessage {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }
}

/// Ordered Session history available to subsequent Runs.
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
