use crate::{ApprovalId, PrincipalId, RunId};
use serde::{Deserialize, Serialize};

/// Lifecycle of an Approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
}

/// Operator decision required before a high-blast action proceeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub id: ApprovalId,
    pub run_id: RunId,
    /// Tool or action name (e.g. `write_file`, `shell_exec`).
    pub action: String,
    /// Redacted summary safe for control-plane listing and events.
    pub summary: String,
    pub status: ApprovalStatus,
    /// Principal that created the wait (Run initiator).
    pub requested_by: PrincipalId,
    /// Principal that approved/denied (when resolved).
    pub decided_by: Option<PrincipalId>,
}

impl Approval {
    #[must_use]
    pub fn pending(
        run_id: RunId,
        requested_by: PrincipalId,
        action: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: ApprovalId::new(),
            run_id,
            action: action.into(),
            summary: summary.into(),
            status: ApprovalStatus::Pending,
            requested_by,
            decided_by: None,
        }
    }

    pub fn approve(&mut self, principal: PrincipalId) {
        self.status = ApprovalStatus::Approved;
        self.decided_by = Some(principal);
    }

    pub fn deny(&mut self, principal: PrincipalId) {
        self.status = ApprovalStatus::Denied;
        self.decided_by = Some(principal);
    }
}
