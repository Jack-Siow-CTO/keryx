//! Fenced in-process execute_code: scripts only reach the world via Tool RPC.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use keryx_domain::RunOrigin;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hard-fenced mini interpreter: only `tool(name, json_args)` calls allowed.
///
/// No raw network, subprocess, env, or arbitrary filesystem access.
pub struct ExecuteCodeTools {
    allowed: HashSet<String>,
    origin: RunOrigin,
    /// Host-mediated tool RPC (Policy already applied by outer ControlPlane before invoke,
    /// and again here for nested tool names via the shared runtime).
    host_tools: Arc<dyn ToolRuntime>,
    max_duration: Duration,
    max_rpc_calls: u32,
}

impl ExecuteCodeTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        origin: RunOrigin,
        host_tools: Arc<dyn ToolRuntime>,
    ) -> Self {
        Self {
            allowed,
            origin,
            host_tools,
            max_duration: Duration::from_secs(5),
            max_rpc_calls: 16,
        }
    }
}

#[async_trait]
impl ToolRuntime for ExecuteCodeTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        if call.name != "execute_code" {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        // Reduced origin: deny by default.
        if self.origin.is_reduced_trust() {
            return Err(ToolError::Denied(
                "execute_code denied for reduced Run origin".into(),
            ));
        }
        let code = call
            .arguments
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing code".into()))?;

        // Fence: reject banned constructs before any execution.
        fence_check(code)?;

        let started = Instant::now();
        let mut rpc_count = 0u32;
        let mut outputs = Vec::new();

        // Script language: lines of `tool NAME {json}` or comments `#`.
        for (lineno, raw) in code.lines().enumerate() {
            if started.elapsed() > self.max_duration {
                return Err(ToolError::Failed(
                    "execute_code: time quota exceeded".into(),
                ));
            }
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("tool ") {
                rpc_count += 1;
                if rpc_count > self.max_rpc_calls {
                    return Err(ToolError::Failed(
                        "execute_code: RPC call quota exceeded".into(),
                    ));
                }
                let (name, args_json) = parse_tool_line(rest)
                    .map_err(|e| ToolError::Failed(format!("line {}: {e}", lineno + 1)))?;
                let nested = ToolCall {
                    name: name.clone(),
                    arguments: args_json,
                };
                match self.host_tools.invoke(nested).await {
                    Ok(r) => outputs.push(format!("{name}: {}", r.summary)),
                    Err(e) => outputs.push(format!("{name}: error={e}")),
                }
            } else if line.starts_with("print ") {
                outputs.push(line.trim_start_matches("print ").to_string());
            } else {
                return Err(ToolError::Failed(format!(
                    "line {}: only 'tool NAME {{json}}' or 'print …' allowed (fence)",
                    lineno + 1
                )));
            }
        }

        let content = if outputs.is_empty() {
            "execute_code: no ops".into()
        } else {
            outputs.join("\n")
        };
        Ok(ToolResult {
            summary: format!("execute_code rpc_calls={rpc_count}"),
            content,
        })
    }
}

fn fence_check(code: &str) -> Result<(), ToolError> {
    let lower = code.to_ascii_lowercase();
    for banned in [
        "std::",
        "tokio::",
        "reqwest",
        "std::process",
        "std::fs",
        "std::net",
        "std::env",
        "include!",
        "command::",
        "tcpstream",
        "udp",
        "file::",
        "/etc/",
        "open(",
        "eval(",
        "import ",
        "require(",
        "__import__",
        "subprocess",
        "os.system",
        "socket.",
    ] {
        if lower.contains(banned) {
            return Err(ToolError::Denied(format!(
                "execute_code fence: banned construct '{banned}'"
            )));
        }
    }
    Ok(())
}

fn parse_tool_line(rest: &str) -> Result<(String, Value), String> {
    let rest = rest.trim();
    let brace = rest.find('{').unwrap_or(rest.len());
    let name = rest[..brace].trim().to_string();
    if name.is_empty() {
        return Err("empty tool name".into());
    }
    let args = if brace < rest.len() {
        serde_json::from_str(&rest[brace..]).map_err(|e| e.to_string())?
    } else {
        Value::Object(Default::default())
    };
    Ok((name, args))
}
