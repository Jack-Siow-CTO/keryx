use async_trait::async_trait;
use serde_json::{json, Value};
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

/// One entry in the invocable tool catalog advertised to Model providers.
///
/// Catalog for a Run = registered tools ∩ Policy allowlist (name, description, JSON schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema object for parameters (OpenAI-style `parameters`).
    pub parameters: Value,
}

impl ToolSpec {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// Minimal empty-object schema (no required args).
    #[must_use]
    pub fn empty_params(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self::new(
            name,
            description,
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
            }),
        )
    }
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

    /// Registered tool specs this runtime can execute (before Policy intersection).
    ///
    /// Default: empty (deny-all / opaque runtimes). Adapters override to feed the model catalog.
    fn catalog(&self) -> Vec<ToolSpec> {
        Vec::new()
    }
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

/// Intersect a registered catalog with a Policy allowlist (exact names).
#[must_use]
pub fn catalog_for_policy(registered: &[ToolSpec], allows: impl Fn(&str) -> bool) -> Vec<ToolSpec> {
    registered
        .iter()
        .filter(|t| allows(&t.name))
        .cloned()
        .collect()
}

/// Summarize tool arguments for Run events: redact secret-like keys, strip URL userinfo, truncate.
///
/// Key matching normalizes to lowercase and strips `_`/`-` so `apiKey`, `api_key`, and `API-KEY`
/// all match. Nested objects are redacted recursively (one level is enough for typical payloads;
/// full recursion is used for safety).
#[must_use]
pub fn summarize_tool_args(args: &Value) -> String {
    summarize_tool_args_depth(args, 0)
}

fn summarize_tool_args_depth(args: &Value, depth: usize) -> String {
    const MAX_DEPTH: usize = 4;
    match args {
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    if is_secret_like_key(k) {
                        format!("{k}=[REDACTED]")
                    } else if let Value::Object(_) = v {
                        if depth < MAX_DEPTH {
                            let inner = summarize_tool_args_depth(v, depth + 1);
                            format!("{k}={{{inner}}}")
                        } else {
                            format!("{k}=[nested]")
                        }
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

/// True when a key name looks like a credential after normalization.
fn is_secret_like_key(key: &str) -> bool {
    let norm: String = key
        .chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(|c| c.to_lowercase())
        .collect();
    // Substring matches on normalized form (apiKey → apikey, api_key → apikey).
    if norm.contains("secret")
        || norm.contains("password")
        || norm.contains("passwd")
        || norm.contains("token")
        || norm.contains("authorization")
        || norm.contains("bearer")
        || norm.contains("apikey")
        || (norm.ends_with("key") && norm.len() > 3)
    {
        return true;
    }
    // Exact normalized credential field names (covers access_token / refresh_token / client_secret).
    matches!(
        norm.as_str(),
        "accesstoken" | "refreshtoken" | "clientsecret" | "privatekey" | "auth" | "credential" | "credentials"
    )
}

fn truncate_value(value: &Value, max: usize) -> String {
    let s = match value {
        Value::String(s) => sanitize_string_for_event(s),
        other => other.to_string(),
    };
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}…")
    } else {
        s
    }
}

/// Strip URL userinfo (credentials) and leave other strings unchanged.
fn sanitize_string_for_event(s: &str) -> String {
    if let Ok(mut parsed) = url::Url::parse(s) {
        if parsed.scheme() == "http" || parsed.scheme() == "https" {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            return parsed.to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_normalized_secret_keys_and_nested() {
        let s = summarize_tool_args(&json!({
            "message": "hi",
            "apiKey": "camel-secret",
            "API-KEY": "dash-secret",
            "api_key": "snake-secret",
            "authorization": "Bearer x",
            "nested": {
                "access_token": "nested-secret",
                "q": "ok"
            }
        }));
        assert!(s.contains("message=hi"), "{s}");
        assert!(s.contains("apiKey=[REDACTED]"), "{s}");
        assert!(s.contains("API-KEY=[REDACTED]"), "{s}");
        assert!(s.contains("api_key=[REDACTED]"), "{s}");
        assert!(s.contains("authorization=[REDACTED]"), "{s}");
        assert!(s.contains("access_token=[REDACTED]"), "{s}");
        assert!(!s.contains("camel-secret"), "{s}");
        assert!(!s.contains("nested-secret"), "{s}");
        assert!(s.contains("q=ok") || s.contains("q=\"ok\""), "{s}");
    }
}
