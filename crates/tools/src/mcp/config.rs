//! Pure MCP static configuration parse (config → client set).
//!
//! Apply path: edit config → restart Worker. Parsing is pure so hot reload can reuse later.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Restricted charset for stable server ids: lowercase alphanumeric + underscore.
pub const SERVER_ID_RE: &str = r"^[a-z][a-z0-9_]*$";

/// Root MCP configuration (file or structured env).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

/// One operator-declared MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Stable operator-chosen id → tools named `mcp.<server_id>.<tool_name>`.
    pub server_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub transport: McpTransportConfig,
    /// Optional cwd for stdio processes.
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    /// Extra env for stdio child (literal values; prefer secret file refs).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Env var name → path to secret file (mode 600 recommended). Values never logged.
    #[serde(default)]
    pub env_files: BTreeMap<String, PathBuf>,
    /// Optional filter: only register these peer-local tool names (exact).
    #[serde(default)]
    pub tool_filter: Vec<String>,
    /// Peer-local tool names that require Approval (exact; no substring heuristics).
    #[serde(default)]
    pub high_blast: Vec<String>,
    /// Peer-local names to add to control_plane Policy (connect ≠ allow; must be explicit).
    #[serde(default)]
    pub policy_allowlist: Vec<String>,
    /// Optional per-call timeout milliseconds (adapter default when unset).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional max result characters for Transcript/events.
    #[serde(default)]
    pub max_result_chars: Option<usize>,
}

fn default_true() -> bool {
    true
}

/// Transport selectable per server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// Remote HTTP JSON-RPC **POST** only (simple request/response).
    /// Not a full MCP SSE / streamable-HTTP session lifecycle.
    Remote {
        url: String,
        /// Env var name holding Bearer token (optional).
        #[serde(default)]
        auth_token_env: Option<String>,
        /// Path to secret file for Bearer token (optional).
        #[serde(default)]
        auth_token_file: Option<PathBuf>,
    },
}

/// Reject invalid server_id charset.
pub fn validate_server_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("server_id must not be empty".into());
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err("server_id must not be empty".into());
    };
    if !first.is_ascii_lowercase() {
        return Err(format!(
            "server_id '{id}' must start with a lowercase letter (charset: {SERVER_ID_RE})"
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(format!(
            "server_id '{id}' has invalid characters (charset: {SERVER_ID_RE})"
        ));
    }
    Ok(())
}

/// Parse MCP config JSON (pure).
pub fn parse_mcp_config_json(raw: &str) -> Result<McpConfig, String> {
    let cfg: McpConfig =
        serde_json::from_str(raw).map_err(|e| format!("MCP config JSON: {e}"))?;
    validate_mcp_config(&cfg)?;
    Ok(cfg)
}

/// Load MCP config from a file path.
pub fn load_mcp_config(path: &Path) -> Result<McpConfig, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read MCP config {}: {e}", path.display()))?;
    parse_mcp_config_json(&raw)
}

fn validate_mcp_config(cfg: &McpConfig) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for s in &cfg.servers {
        validate_server_id(&s.server_id)?;
        if !seen.insert(s.server_id.clone()) {
            return Err(format!("duplicate MCP server_id '{}'", s.server_id));
        }
        match &s.transport {
            McpTransportConfig::Stdio { command, .. } => {
                if command.trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}': stdio command must not be empty",
                        s.server_id
                    ));
                }
            }
            McpTransportConfig::Remote { url, .. } => {
                if url.trim().is_empty() {
                    return Err(format!(
                        "MCP server '{}': remote url must not be empty",
                        s.server_id
                    ));
                }
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(format!(
                        "MCP server '{}': remote url must be http(s)",
                        s.server_id
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_server_id() {
        assert!(validate_server_id("gmail").is_ok());
        assert!(validate_server_id("home_assistant").is_ok());
        assert!(validate_server_id("Gmail").is_err());
        assert!(validate_server_id("1bad").is_err());
        assert!(validate_server_id("bad-id").is_err());
        assert!(validate_server_id("").is_err());
    }

    #[test]
    fn parse_stdio_and_remote() {
        let raw = r#"{
          "servers": [
            {
              "server_id": "demo",
              "transport": { "type": "stdio", "command": "mcp-demo", "args": ["--stdio"] },
              "policy_allowlist": ["echo"],
              "high_blast": ["send"]
            },
            {
              "server_id": "remote_svc",
              "transport": { "type": "remote", "url": "https://mcp.example/rpc", "auth_token_env": "TOK" }
            }
          ]
        }"#;
        let cfg = parse_mcp_config_json(raw).unwrap();
        assert_eq!(cfg.servers.len(), 2);
        assert_eq!(cfg.servers[0].policy_allowlist, vec!["echo"]);
        assert_eq!(cfg.servers[0].high_blast, vec!["send"]);
    }

    #[test]
    fn duplicate_server_id_rejected() {
        let raw = r#"{
          "servers": [
            { "server_id": "a", "transport": { "type": "stdio", "command": "x" } },
            { "server_id": "a", "transport": { "type": "stdio", "command": "y" } }
          ]
        }"#;
        assert!(parse_mcp_config_json(raw).unwrap_err().contains("duplicate"));
    }
}
