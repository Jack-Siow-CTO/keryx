use crate::model::ModelError;
use thiserror::Error;

/// Application-level failures returned by the control-plane service.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("session not found")]
    SessionNotFound,
    #[error("run not found")]
    RunNotFound,
    #[error("store error: {0}")]
    Store(String),
    #[error("model provider error: {0}")]
    Model(#[from] ModelError),
}
