use crate::{PrincipalId, SessionId};
use serde::{Deserialize, Serialize};

/// Durable conversational and policy context that may span multiple Runs.
///
/// Operator-facing list fields (`title`, timestamps) are durable on the Worker
/// (ADR 0027) — not client-only nicknames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub principal_id: PrincipalId,
    /// Operator-set title. When `None`, list projection derives a default
    /// (first user goal / placeholder).
    pub title: Option<String>,
    /// Unix seconds (UTC) when the Session was created.
    pub created_at: i64,
    /// Unix seconds (UTC) of last meaningful activity (create, rename, Run, transcript).
    pub updated_at: i64,
}

impl Session {
    #[must_use]
    pub fn new(principal_id: PrincipalId) -> Self {
        let now = unix_now();
        Self {
            id: SessionId::new(),
            principal_id,
            title: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Display title for list/detail: operator override, else placeholder.
    ///
    /// Callers that know the first user goal should prefer
    /// [`Session::display_title_with_goal`].
    #[must_use]
    pub fn display_title(&self) -> String {
        self.display_title_with_goal(None)
    }

    #[must_use]
    pub fn display_title_with_goal(&self, first_user_goal: Option<&str>) -> String {
        if let Some(t) = self
            .title
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return t.to_string();
        }
        if let Some(goal) = first_user_goal.map(str::trim).filter(|s| !s.is_empty()) {
            return truncate_title(goal, 80);
        }
        "New Session".to_string()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        let t = title.into();
        let trimmed = t.trim();
        self.title = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        self.updated_at = unix_now();
    }

    pub fn touch(&mut self) {
        self.updated_at = unix_now();
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn truncate_title(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Compact Active root Run summary for Session list/detail chips (ADR 0027).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRootRunSummary {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub origin: String,
}

/// Operator Session list projection (not bare ids).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub principal_id: String,
    /// Resolved display title (override or default).
    pub title: String,
    /// True when `title` is an operator override (not derived default).
    pub title_is_custom: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_message_preview: Option<String>,
    pub active_root_run: Option<ActiveRootRunSummary>,
    pub pending_approval_count: u32,
}
