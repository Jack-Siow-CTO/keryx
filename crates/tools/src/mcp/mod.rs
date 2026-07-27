//! MCP client Tools: static config, discovery, namespaced invoke, mock peer (Seam 1).
//!
//! Domain has no MCP protocol types. Protocol lives here; app sees ToolRuntime + ToolSpec.

mod client;
mod config;
mod mock;
mod protocol;

pub use client::{
    build_mcp_runtimes, McpClientRegistry, McpDoctorReport, McpRuntimeBundle, McpServerHealth,
    McpSession,
};
pub use config::{
    load_mcp_config, parse_mcp_config_json, validate_server_id, McpConfig, McpServerConfig,
    McpTransportConfig,
};
pub use mock::{McpClientTools, McpServerExport, MockMcpPeer};
pub use protocol::{namespaced_tool_name, parse_namespaced_tool, MCP_NAMESPACE_PREFIX};

/// Build a mock-based registry for Seam 1 tests (config-driven shape without live peers).
#[must_use]
pub fn mock_registry_from_peer(
    server_id: &str,
    peer: std::sync::Arc<MockMcpPeer>,
    policy_allowlist_local: &[String],
    high_blast_local: &[String],
) -> McpClientRegistry {
    McpClientRegistry::from_mock(server_id, peer, policy_allowlist_local, high_blast_local)
}
