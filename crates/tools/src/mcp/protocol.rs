//! MCP naming helpers and JSON-RPC message shapes (no domain types).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MCP_NAMESPACE_PREFIX: &str = "mcp.";
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Build `mcp.<server_id>.<tool_name>`.
#[must_use]
pub fn namespaced_tool_name(server_id: &str, local_name: &str) -> String {
    format!("{MCP_NAMESPACE_PREFIX}{server_id}.{local_name}")
}

/// Parse `mcp.<server_id>.<tool_name>` → (server_id, local_name).
pub fn parse_namespaced_tool(full: &str) -> Option<(&str, &str)> {
    let rest = full.strip_prefix(MCP_NAMESPACE_PREFIX)?;
    let (server_id, tool) = rest.split_once('.')?;
    if server_id.is_empty() || tool.is_empty() {
        return None;
    }
    // tool name may contain dots
    Some((server_id, tool))
}

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    #[must_use]
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC notification (no `id`; fire-and-forget, e.g. `notifications/initialized`).
#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcNotification {
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "inputSchema")]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ToolsCallResult {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default, alias = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub kind: Option<String>,
    pub text: Option<String>,
}

/// Content-Length framed MCP message (stdio).
#[must_use]
pub fn encode_framed(body: &str) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

/// Try to extract one framed message from a buffer; returns (body, consumed_bytes).
pub fn try_decode_framed(buf: &[u8]) -> Result<Option<(String, usize)>, String> {
    let s = String::from_utf8_lossy(buf);
    // Find header/body separator
    let Some(sep) = s.find("\r\n\r\n") else {
        // Also accept \n\n for lenient peers
        let Some(sep) = s.find("\n\n") else {
            return Ok(None);
        };
        let headers = &s[..sep];
        let content_len = parse_content_length(headers)?;
        let header_end = sep + 2;
        if buf.len() < header_end + content_len {
            return Ok(None);
        }
        let body = String::from_utf8_lossy(&buf[header_end..header_end + content_len]).into_owned();
        return Ok(Some((body, header_end + content_len)));
    };
    let headers = &s[..sep];
    let content_len = parse_content_length(headers)?;
    let header_end = sep + 4;
    if buf.len() < header_end + content_len {
        return Ok(None);
    }
    let body = String::from_utf8_lossy(&buf[header_end..header_end + content_len]).into_owned();
    Ok(Some((body, header_end + content_len)))
}

fn parse_content_length(headers: &str) -> Result<usize, String> {
    for line in headers.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            return rest
                .trim()
                .parse()
                .map_err(|e| format!("invalid Content-Length: {e}"));
        }
    }
    Err("MCP frame missing Content-Length".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_roundtrip() {
        let n = namespaced_tool_name("gmail", "search");
        assert_eq!(n, "mcp.gmail.search");
        assert_eq!(parse_namespaced_tool(&n), Some(("gmail", "search")));
        assert_eq!(
            parse_namespaced_tool("mcp.gmail.send_message"),
            Some(("gmail", "send_message"))
        );
    }

    #[test]
    fn frame_roundtrip() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let framed = encode_framed(body);
        let (decoded, n) = try_decode_framed(&framed).unwrap().unwrap();
        assert_eq!(decoded, body);
        assert_eq!(n, framed.len());
    }
}
