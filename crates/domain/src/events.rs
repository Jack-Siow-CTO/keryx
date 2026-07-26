use crate::RunId;
use serde::{Deserialize, Serialize};

/// Append-only observation emitted while a Run is Active (or at terminal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub run_id: RunId,
    pub seq: u64,
    pub kind: RunEventKind,
}

/// Fixed Run event taxonomy (ADR 0007). Wire name via [`RunEventKind::name`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RunEventKind {
    RunStarted,
    ModelStarted,
    ModelDelta { text: String },
    ModelFinished,
    ToolStarted { name: String },
    ToolFinished { name: String },
    RunBudget { message: String },
    RunCompleted,
    RunFailed { reason: String },
    RunCancelled,
}

impl RunEventKind {
    /// Stable SSE `event:` name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::RunStarted => "run.started",
            Self::ModelStarted => "model.started",
            Self::ModelDelta { .. } => "model.delta",
            Self::ModelFinished => "model.finished",
            Self::ToolStarted { .. } => "tool.started",
            Self::ToolFinished { .. } => "tool.finished",
            Self::RunBudget { .. } => "run.budget",
            Self::RunCompleted => "run.completed",
            Self::RunFailed { .. } => "run.failed",
            Self::RunCancelled => "run.cancelled",
        }
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::RunCompleted | Self::RunFailed { .. } | Self::RunCancelled
        )
    }
}

impl RunEvent {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.kind.is_terminal()
    }
}
