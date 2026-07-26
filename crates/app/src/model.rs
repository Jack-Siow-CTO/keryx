use async_trait::async_trait;
use thiserror::Error;

/// Request sent to a Model provider for one agent-loop step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub goal: String,
}

/// Completion returned by a Model provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub content: String,
}

/// Failures from a Model provider adapter.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ModelError(pub String);

impl ModelError {
    #[must_use]
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Port for model completions used by the agent loop.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;
}
