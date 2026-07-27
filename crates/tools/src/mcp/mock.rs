//! Mock MCP peer for Seam 1 (no live third-party MCP).

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use super::protocol::namespaced_tool_name;

/// Mock MCP server peer for CI (no live third-party MCP).
#[derive(Debug, Default)]
pub struct MockMcpPeer {
    /// local_name → result content
    tools: HashMap<String, String>,
    /// local_name → description
    descriptions: HashMap<String, String>,
    /// local_name → JSON schema
    schemas: HashMap<String, Value>,
    disconnected: Mutex<bool>,
}

impl MockMcpPeer {
    #[must_use]
    pub fn with_tool(mut self, name: impl Into<String>, result: impl Into<String>) -> Self {
        let name = name.into();
        self.tools.insert(name.clone(), result.into());
        self.descriptions
            .entry(name)
            .or_insert_with(|| "mock MCP tool".into());
        self
    }

    #[must_use]
    pub fn with_tool_schema(
        mut self,
        name: impl Into<String>,
        result: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        let name = name.into();
        self.tools.insert(name.clone(), result.into());
        self.descriptions.insert(name.clone(), description.into());
        self.schemas.insert(name, schema);
        self
    }

    pub fn disconnect(&self) {
        if let Ok(mut d) = self.disconnected.lock() {
            *d = true;
        }
    }

    pub fn is_disconnected(&self) -> bool {
        self.disconnected.lock().map(|d| *d).unwrap_or(true)
    }

    /// Look up mock result content for a peer-local tool name.
    #[must_use]
    pub fn tool_result(&self, local_name: &str) -> Option<String> {
        self.tools.get(local_name).cloned()
    }

    #[must_use]
    pub fn local_tools(&self) -> Vec<(String, String, Value)> {
        self.tools
            .keys()
            .map(|k| {
                let desc = self
                    .descriptions
                    .get(k)
                    .cloned()
                    .unwrap_or_else(|| "mock MCP tool".into());
                let schema = self.schemas.get(k).cloned().unwrap_or_else(|| {
                    json!({
                        "type": "object",
                        "properties": {},
                        "additionalProperties": true
                    })
                });
                (k.clone(), desc, schema)
            })
            .collect()
    }
}

/// MCP client: namespaced tools from a mock peer (Seam 1).
pub struct McpClientTools {
    allowed: HashSet<String>,
    peer: std::sync::Arc<MockMcpPeer>,
    /// Namespace prefix e.g. `mcp.demo.`
    namespace: String,
    server_id: String,
}

impl McpClientTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        peer: std::sync::Arc<MockMcpPeer>,
        namespace: impl Into<String>,
    ) -> Self {
        let namespace = namespace.into();
        let server_id = namespace
            .strip_prefix("mcp.")
            .unwrap_or(namespace.as_str())
            .trim_end_matches('.')
            .to_string();
        Self {
            allowed,
            peer,
            namespace,
            server_id,
        }
    }

    /// Construct from server_id (namespace `mcp.<id>.`).
    #[must_use]
    pub fn for_server(
        server_id: impl Into<String>,
        allowed: HashSet<String>,
        peer: std::sync::Arc<MockMcpPeer>,
    ) -> Self {
        let server_id = server_id.into();
        let namespace = format!("mcp.{server_id}.");
        Self {
            allowed,
            peer,
            namespace,
            server_id,
        }
    }

    #[must_use]
    pub fn registered_names(&self) -> Vec<String> {
        self.peer
            .tools
            .keys()
            .map(|k| namespaced_tool_name(&self.server_id, k))
            .filter(|n| self.allowed.is_empty() || self.allowed.contains(n))
            .collect()
    }

    #[must_use]
    pub fn server_id(&self) -> &str {
        &self.server_id
    }
}

#[async_trait]
impl ToolRuntime for McpClientTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.is_empty() && !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        if self.peer.is_disconnected() {
            return Err(ToolError::Failed(
                "MCP client disconnect: fail closed for in-flight invocation".into(),
            ));
        }
        let local = call
            .name
            .strip_prefix(&self.namespace)
            .ok_or_else(|| ToolError::Denied(format!("not an MCP namespaced tool {}", call.name)))?;
        let result = self
            .peer
            .tools
            .get(local)
            .ok_or_else(|| ToolError::Denied(format!("MCP tool '{local}' not registered")))?;
        Ok(ToolResult {
            content: result.clone(),
            summary: format!("mcp_client tool={}", call.name),
        })
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        self.peer
            .local_tools()
            .into_iter()
            .filter_map(|(local, desc, schema)| {
                let name = namespaced_tool_name(&self.server_id, &local);
                if !self.allowed.is_empty() && !self.allowed.contains(&name) {
                    return None;
                }
                Some(ToolSpec::new(name, desc, schema))
            })
            .collect()
    }
}

/// MCP server export: selected tools only with authenticated Principal path.
///
/// Out of ship bar for client-ingest slice; kept fail-closed for Seam 1.
pub struct McpServerExport {
    /// Requires operator token / Principal; unauthenticated serve impossible by default.
    require_auth: bool,
    exported: HashSet<String>,
    peer: std::sync::Arc<MockMcpPeer>,
}

impl McpServerExport {
    #[must_use]
    pub fn new(exported: HashSet<String>, peer: std::sync::Arc<MockMcpPeer>) -> Self {
        Self {
            require_auth: true,
            exported,
            peer,
        }
    }

    /// Fail closed without Principal/operator auth.
    pub fn invoke_exported(
        &self,
        authenticated: bool,
        tool: &str,
        _args: &Value,
    ) -> Result<String, ToolError> {
        if self.require_auth && !authenticated {
            return Err(ToolError::Denied(
                "MCP serve requires authenticated Principal/operator path".into(),
            ));
        }
        if !self.exported.contains(tool) {
            return Err(ToolError::Denied(format!(
                "tool '{tool}' not exported by MCP server"
            )));
        }
        if self.peer.is_disconnected() {
            return Err(ToolError::Failed(
                "MCP server disconnect: fail closed".into(),
            ));
        }
        Ok(format!("exported:{tool}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keryx_app::ToolRuntime;
    use std::sync::Arc;

    #[tokio::test]
    async fn mcp_client_namespaced_and_disconnect_fail_closed() {
        let peer = std::sync::Arc::new(MockMcpPeer::default().with_tool("echo", "pong"));
        let tools = McpClientTools::new(
            HashSet::from(["mcp.demo.echo".into()]),
            Arc::clone(&peer),
            "mcp.demo.",
        );
        let ok = tools
            .invoke(ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: Value::Object(Default::default()),
            })
            .await
            .unwrap();
        assert!(ok.content.contains("pong"));
        assert!(tools
            .catalog()
            .iter()
            .any(|t| t.name == "mcp.demo.echo"));
        peer.disconnect();
        let err = tools
            .invoke(ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: Value::Object(Default::default()),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("fail closed"));
    }

    #[test]
    fn mcp_serve_requires_auth() {
        let peer = std::sync::Arc::new(MockMcpPeer::default());
        let serve = McpServerExport::new(HashSet::from(["read_file".into()]), peer);
        assert!(serve
            .invoke_exported(false, "read_file", &Value::Null)
            .unwrap_err()
            .to_string()
            .contains("authenticated"));
        assert!(serve
            .invoke_exported(true, "read_file", &Value::Null)
            .unwrap()
            .contains("exported"));
    }
}
