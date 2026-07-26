use crate::tools::ToolCall;
use async_trait::async_trait;
use keryx_domain::TranscriptMessage;
use thiserror::Error;

/// Request sent to a Model provider for one agent-loop step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub goal: String,
    pub transcript: Vec<TranscriptMessage>,
    /// Optional provider key (`openai`, `grok`, `fake`) for multi-provider routing.
    pub provider: Option<String>,
}

/// Completion returned by a Model provider.
///
/// `deltas` are optional token/text chunks for SSE `model.delta` events; `content` is the full answer.
/// `tool_calls` are tools the model wants to invoke before the next reasoning step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub content: String,
    pub deltas: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Estimated tokens consumed (defaults to content character count when unset by adapters).
    pub tokens_used: u64,
}

impl ModelResponse {
    #[must_use]
    pub fn text(content: impl Into<String>) -> Self {
        let content = content.into();
        let tokens_used = content.chars().count() as u64;
        Self {
            content,
            deltas: Vec::new(),
            tool_calls: Vec::new(),
            tokens_used,
        }
    }

    #[must_use]
    pub fn with_deltas(deltas: Vec<String>) -> Self {
        let content = deltas.concat();
        let tokens_used = content.chars().count() as u64;
        Self {
            content,
            deltas,
            tool_calls: Vec::new(),
            tokens_used,
        }
    }

    #[must_use]
    pub fn with_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        let content = content.into();
        let tokens_used = content.chars().count() as u64;
        Self {
            content,
            deltas: Vec::new(),
            tool_calls,
            tokens_used,
        }
    }
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
