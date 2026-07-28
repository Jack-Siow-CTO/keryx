use crate::model::ModelError;
use keryx_domain::{RunId, SessionId};
use thiserror::Error;

/// Application-level failures returned by the control-plane service.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("session not found")]
    SessionNotFound,
    #[error("run not found")]
    RunNotFound,
    #[error("session {session_id} already has active run {run_id}")]
    ActiveRunExists {
        session_id: SessionId,
        run_id: RunId,
    },
    #[error("global active run cap exceeded (cap={cap})")]
    GlobalCapExceeded { cap: usize },
    #[error("run is not active")]
    RunNotActive,
    #[error("approval not found")]
    ApprovalNotFound,
    #[error("approval is not pending")]
    ApprovalNotPending,
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("memory not found")]
    MemoryNotFound,
    #[error("artifact not found")]
    ArtifactNotFound,
    #[error("store error: {0}")]
    Store(String),
    #[error("model provider error: {0}")]
    Model(#[from] ModelError),
}
