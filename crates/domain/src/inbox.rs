use serde::{Deserialize, Serialize};

/// Kind of Inbox attention item (read projection, not a durable Notification aggregate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxItemKind {
    ApprovalPending,
    RunFailed,
    RunInterrupted,
}

/// Control-plane Inbox projection row (ADR 0028).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub kind: InboxItemKind,
    pub session_id: Option<String>,
    pub run_id: Option<String>,
    pub approval_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub created_at: i64,
}
