use crate::{PrincipalId, RunId, RunOrigin, SessionId};
use serde::{Deserialize, Serialize};

/// Lifecycle status of a Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

/// One bounded execution of the agent loop toward a goal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub session_id: SessionId,
    pub principal_id: PrincipalId,
    pub goal: String,
    pub status: RunStatus,
    /// Channel that initiated this Run (Policy templates depend on origin).
    pub origin: RunOrigin,
    /// Parent Run when this is a Child Run (None for Session-level root Runs).
    pub parent_run_id: Option<RunId>,
    /// Final model answer or failure reason when terminal.
    pub result: Option<String>,
}

impl Run {
    /// Start a root Run with control-plane origin (trusted control plane API).
    #[must_use]
    pub fn start(
        session_id: SessionId,
        principal_id: PrincipalId,
        goal: impl Into<String>,
    ) -> Self {
        Self::start_with_origin(session_id, principal_id, goal, RunOrigin::ControlPlane)
    }

    /// Start a root Run with an explicit Run origin (for Gateways, Schedules, tests).
    #[must_use]
    pub fn start_with_origin(
        session_id: SessionId,
        principal_id: PrincipalId,
        goal: impl Into<String>,
        origin: RunOrigin,
    ) -> Self {
        Self {
            id: RunId::new(),
            session_id,
            principal_id,
            goal: goal.into(),
            status: RunStatus::Active,
            origin,
            parent_run_id: None,
            result: None,
        }
    }

    /// Start a Child Run under a parent (inherits Session + Principal; isolated goal).
    #[must_use]
    pub fn start_child(
        session_id: SessionId,
        principal_id: PrincipalId,
        parent_run_id: RunId,
        goal: impl Into<String>,
        origin: RunOrigin,
    ) -> Self {
        Self {
            id: RunId::new(),
            session_id,
            principal_id,
            goal: goal.into(),
            status: RunStatus::Active,
            origin,
            parent_run_id: Some(parent_run_id),
            result: None,
        }
    }

    /// True when this Run is a Session-level root (not a Child Run).
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.parent_run_id.is_none()
    }

    pub fn complete(&mut self, result: impl Into<String>) {
        self.status = RunStatus::Completed;
        self.result = Some(result.into());
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.status = RunStatus::Failed;
        self.result = Some(reason.into());
    }

    pub fn cancel(&mut self) {
        self.status = RunStatus::Cancelled;
        self.result = Some("cancelled".into());
    }

    pub fn interrupt(&mut self) {
        self.status = RunStatus::Interrupted;
        self.result = Some("interrupted".into());
    }
}
