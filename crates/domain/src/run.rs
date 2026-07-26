use crate::{PrincipalId, RunId, SessionId};
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
    /// Final model answer or failure reason when terminal.
    pub result: Option<String>,
}

impl Run {
    #[must_use]
    pub fn start(
        session_id: SessionId,
        principal_id: PrincipalId,
        goal: impl Into<String>,
    ) -> Self {
        Self {
            id: RunId::new(),
            session_id,
            principal_id,
            goal: goal.into(),
            status: RunStatus::Active,
            result: None,
        }
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
