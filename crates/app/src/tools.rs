use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

/// A model-requested tool invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

/// Result of a Tool invocation for Transcript and summarized events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Full content for Session Transcript (agent continuity).
    pub content: String,
    /// Safe, short summary for Run events (no secret dumps).
    pub summary: String,
}

/// Tool policy / execution failures (fail closed).
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool denied by policy: {0}")]
    Denied(String),
    #[error("path jail violation: {0}")]
    PathJail(String),
    #[error("tool error: {0}")]
    Failed(String),
}

/// Port for policy-gated Tool execution used by the agent loop.
#[async_trait]
pub trait ToolRuntime: Send + Sync {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError>;
}

/// Default runtime: deny every tool (fail closed until Workspace tools are wired).
#[derive(Debug, Default)]
pub struct DenyAllTools;

#[async_trait]
impl ToolRuntime for DenyAllTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        Err(ToolError::Denied(format!(
            "unknown or disallowed tool '{}'",
            call.name
        )))
    }
}

/// Summarize tool arguments for Run events: redact secret-like keys, truncate values.
#[must_use]
pub fn summarize_tool_args(args: &Value) -> String {
    match args {
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key_lower = k.to_ascii_lowercase();
                    let redacted = key_lower.contains("secret")
                        || key_lower.contains("token")
                        || key_lower.contains("password")
                        || key_lower.contains("api_key")
                        || key_lower.ends_with("_key");
                    if redacted {
                        format!("{k}=[REDACTED]")
                    } else {
                        format!("{k}={}", truncate_value(v, 80))
                    }
                })
                .collect();
            parts.join(", ")
        }
        other => truncate_value(other, 80),
    }
}

fn truncate_value(value: &Value, max: usize) -> String {
    let s = match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}…")
    } else {
        s
    }
}
