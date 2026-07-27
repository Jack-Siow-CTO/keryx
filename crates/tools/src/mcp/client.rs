//! MCP client sessions, registry, composition, and doctor reporting.
//!
//! Domain has no MCP SDK types. Fail closed on disconnect / transport errors.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::timeout;

use super::config::{McpConfig, McpServerConfig, McpTransportConfig};
use super::mock::MockMcpPeer;
use super::protocol::{
    encode_framed, namespaced_tool_name, parse_namespaced_tool, try_decode_framed,
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpToolDef, ToolsCallResult,
    ToolsListResult, PROTOCOL_VERSION,
};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_RESULT_CHARS: usize = 50_000;
const MAX_FRAME_BUF: usize = 4 * 1024 * 1024;

/// Health of one configured MCP server (doctor; never secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerHealth {
    pub server_id: String,
    pub configured: bool,
    pub connected: bool,
    pub error: Option<String>,
    /// Namespaced tool names discovered/registered.
    pub discovered_tools: Vec<String>,
    /// Subset on control_plane Policy allowlist (from config `policy_allowlist`).
    pub allowlisted_tools: Vec<String>,
    /// Config-declared high-blast namespaced names.
    pub high_blast_tools: Vec<String>,
}

/// Aggregate doctor report for all MCP servers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpDoctorReport {
    pub servers: Vec<McpServerHealth>,
}

impl McpDoctorReport {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }

    /// Pretty-print doctor lines (no secrets).
    pub fn print_lines(&self) {
        if self.servers.is_empty() {
            println!("info MCP: no servers configured (set KERYX_MCP_CONFIG)");
            return;
        }
        for s in &self.servers {
            let status = if s.connected {
                "connected"
            } else if s.configured {
                "error"
            } else {
                "not_configured"
            };
            println!(
                "{} MCP server '{}' status={status} discovered={} allowlisted={} high_blast={}",
                if s.connected { "ok  " } else { "warn" },
                s.server_id,
                s.discovered_tools.len(),
                s.allowlisted_tools.len(),
                s.high_blast_tools.len()
            );
            if let Some(err) = &s.error {
                println!("     error: {err}");
            }
            if !s.discovered_tools.is_empty() {
                println!("     discovered: {:?}", s.discovered_tools);
            }
            if !s.allowlisted_tools.is_empty() {
                println!("     control_plane allowlist: {:?}", s.allowlisted_tools);
            }
            if !s.high_blast_tools.is_empty() {
                println!("     high_blast: {:?}", s.high_blast_tools);
            }
        }
    }
}

/// Result of pure-ish composition: config → runtime + policy extras + doctor.
pub struct McpRuntimeBundle {
    pub runtime: Option<Arc<McpClientRegistry>>,
    /// Exact namespaced tool names for control_plane Policy extras.
    pub control_plane_extra: Vec<String>,
    /// Exact namespaced high-blast tool names.
    pub high_blast: Vec<String>,
    pub doctor: McpDoctorReport,
}

/// One live or mock MCP session (transport + discovered tools).
pub struct McpSession {
    server_id: String,
    transport: Arc<dyn McpTransport>,
    tools: Vec<RegisteredMcpTool>,
    timeout: Duration,
    max_result_chars: usize,
}

#[derive(Debug, Clone)]
struct RegisteredMcpTool {
    local_name: String,
    namespaced: String,
    description: String,
    parameters: Value,
}

impl McpSession {
    #[must_use]
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    #[must_use]
    pub fn registered_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.namespaced.clone()).collect()
    }

    #[must_use]
    pub fn catalog(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .map(|t| {
                ToolSpec::new(
                    t.namespaced.clone(),
                    t.description.clone(),
                    t.parameters.clone(),
                )
            })
            .collect()
    }

    fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    fn disconnect(&self) {
        self.transport.disconnect();
    }

    async fn call_tool(&self, local_name: &str, arguments: Value) -> Result<String, ToolError> {
        if !self.transport.is_connected() {
            return Err(ToolError::Failed(format!(
                "MCP server '{}': disconnected (fail closed)",
                self.server_id
            )));
        }
        let params = json!({
            "name": local_name,
            "arguments": arguments,
        });
        let result = self
            .transport
            .request_timeout("tools/call", Some(params), self.timeout)
            .await?;
        // Fail closed: malformed tools/call payloads must not become empty success.
        let parsed: ToolsCallResult = serde_json::from_value(result).map_err(|e| {
            ToolError::Failed(format!(
                "MCP tools/call result parse error on '{}': {e}",
                self.server_id
            ))
        })?;
        if parsed.is_error {
            let msg = content_to_text(&parsed.content);
            return Err(ToolError::Failed(format!(
                "MCP tools/call error on '{}': {msg}",
                self.server_id
            )));
        }
        let mut text = content_to_text(&parsed.content);
        if text.chars().count() > self.max_result_chars {
            text = text.chars().take(self.max_result_chars).collect::<String>() + "…[truncated]";
        }
        Ok(text)
    }
}

fn content_to_text(blocks: &[super::protocol::ContentBlock]) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    blocks
        .iter()
        .filter_map(|b| b.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}

fn empty_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": true
    })
}

// --- Transport -------------------------------------------------------------

#[async_trait]
trait McpTransport: Send + Sync {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, ToolError>;

    /// Fire-and-forget JSON-RPC notification (no response wait).
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), ToolError>;

    async fn request_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        limit: Duration,
    ) -> Result<Value, ToolError> {
        match timeout(limit, self.request(method, params)).await {
            Ok(inner) => inner,
            Err(_) => Err(ToolError::Failed(format!(
                "MCP request '{method}' timed out after {}ms",
                limit.as_millis()
            ))),
        }
    }

    fn disconnect(&self);
    fn is_connected(&self) -> bool;
}

/// Mock transport backed by [`MockMcpPeer`] (Seam 1; no live process).
struct MockTransport {
    peer: Arc<MockMcpPeer>,
    connected: AtomicBool,
}

#[async_trait]
impl McpTransport for MockTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, ToolError> {
        if !self.is_connected() || self.peer.is_disconnected() {
            self.connected.store(false, Ordering::SeqCst);
            return Err(ToolError::Failed(
                "MCP client disconnect: fail closed for in-flight invocation".into(),
            ));
        }
        match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "mock", "version": "0" }
            })),
            "tools/list" => {
                let tools: Vec<Value> = self
                    .peer
                    .local_tools()
                    .into_iter()
                    .map(|(name, desc, schema)| {
                        json!({
                            "name": name,
                            "description": desc,
                            "inputSchema": schema,
                        })
                    })
                    .collect();
                Ok(json!({ "tools": tools }))
            }
            "tools/call" => {
                let local = params
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Failed("tools/call missing name".into()))?;
                let content = self.peer.tool_result(local).ok_or_else(|| {
                    ToolError::Denied(format!("MCP tool '{local}' not registered"))
                })?;
                Ok(json!({
                    "content": [{ "type": "text", "text": content }],
                    "isError": false
                }))
            }
            other => Err(ToolError::Failed(format!(
                "mock MCP: unsupported method '{other}'"
            ))),
        }
    }

    async fn notify(&self, method: &str, _params: Option<Value>) -> Result<(), ToolError> {
        if !self.is_connected() || self.peer.is_disconnected() {
            return Err(ToolError::Failed(
                "MCP client disconnect: fail closed for notification".into(),
            ));
        }
        // Mock peers accept initialized (and other) notifications without side effects.
        let _ = method;
        Ok(())
    }

    fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
        self.peer.disconnect();
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst) && !self.peer.is_disconnected()
    }
}

/// Remote HTTP JSON-RPC POST transport.
struct RemoteHttpTransport {
    url: String,
    auth_token: Option<String>,
    client: reqwest::Client,
    connected: AtomicBool,
    next_id: AtomicU64,
}

#[async_trait]
impl McpTransport for RemoteHttpTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, ToolError> {
        if !self.is_connected() {
            return Err(ToolError::Failed(
                "MCP remote disconnected (fail closed)".into(),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);
        let mut builder = self
            .client
            .post(&self.url)
            .header("content-type", "application/json")
            .json(&req);
        if let Some(tok) = &self.auth_token {
            builder = builder.header("authorization", format!("Bearer {tok}"));
        }
        let response = builder.send().await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP remote request failed: {e}"))
        })?;
        if !response.status().is_success() {
            self.connected.store(false, Ordering::SeqCst);
            return Err(ToolError::Failed(format!(
                "MCP remote HTTP {}",
                response.status()
            )));
        }
        let rpc: JsonRpcResponse = response.json().await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP remote JSON: {e}"))
        })?;
        if let Some(err) = rpc.error {
            return Err(ToolError::Failed(format!(
                "MCP JSON-RPC error {}: {}",
                err.code, err.message
            )));
        }
        rpc.result
            .ok_or_else(|| ToolError::Failed("MCP remote empty result".into()))
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), ToolError> {
        if !self.is_connected() {
            return Err(ToolError::Failed(
                "MCP remote disconnected (fail closed)".into(),
            ));
        }
        let note = JsonRpcNotification::new(method, params);
        let mut builder = self
            .client
            .post(&self.url)
            .header("content-type", "application/json")
            .json(&note);
        if let Some(tok) = &self.auth_token {
            builder = builder.header("authorization", format!("Bearer {tok}"));
        }
        // Fire-and-forget: do not require a JSON-RPC response body for notifications.
        let response = builder.send().await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP remote notify failed: {e}"))
        })?;
        // Accept 2xx; many servers return 202/204 for notifications.
        if !response.status().is_success() {
            // Non-fatal for notify on some peers: treat 4xx/5xx as soft fail closed only
            // when clearly disconnected; otherwise ignore so tools/list can proceed.
            let status = response.status();
            if status.is_server_error() {
                self.connected.store(false, Ordering::SeqCst);
                return Err(ToolError::Failed(format!(
                    "MCP remote notify HTTP {status}"
                )));
            }
        }
        Ok(())
    }

    fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

/// Stdio subprocess with Content-Length framed JSON-RPC.
struct StdioTransport {
    child: AsyncMutex<Option<Child>>,
    stdin: AsyncMutex<Option<tokio::process::ChildStdin>>,
    stdout: AsyncMutex<Option<tokio::process::ChildStdout>>,
    read_buf: AsyncMutex<Vec<u8>>,
    connected: AtomicBool,
    next_id: AtomicU64,
    server_id: String,
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::SeqCst);
        // Best-effort kill of owned child on drop (Worker shutdown).
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, ToolError> {
        if !self.is_connected() {
            return Err(ToolError::Failed(format!(
                "MCP stdio '{}' disconnected (fail closed)",
                self.server_id
            )));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);
        let body = serde_json::to_string(&req)
            .map_err(|e| ToolError::Failed(format!("MCP encode: {e}")))?;
        let framed = encode_framed(&body);

        {
            let mut stdin_guard = self.stdin.lock().await;
            let stdin = stdin_guard.as_mut().ok_or_else(|| {
                self.connected.store(false, Ordering::SeqCst);
                ToolError::Failed(format!("MCP stdio '{}': stdin closed", self.server_id))
            })?;
            stdin.write_all(&framed).await.map_err(|e| {
                self.connected.store(false, Ordering::SeqCst);
                ToolError::Failed(format!("MCP stdio write: {e}"))
            })?;
            stdin.flush().await.map_err(|e| {
                self.connected.store(false, Ordering::SeqCst);
                ToolError::Failed(format!("MCP stdio flush: {e}"))
            })?;
        }

        // Read until we get a response with matching id (skip notifications).
        loop {
            let body = self.read_one_frame().await?;
            // Notifications have no id / are not responses we care about.
            if let Ok(rpc) = serde_json::from_str::<JsonRpcResponse>(&body) {
                // Match by numeric id when present.
                let matches = match &rpc.id {
                    Some(Value::Number(n)) => n.as_u64() == Some(id),
                    Some(Value::String(s)) => s.parse::<u64>().ok() == Some(id),
                    None => false,
                    _ => false,
                };
                if !matches {
                    continue;
                }
                if let Some(err) = rpc.error {
                    return Err(ToolError::Failed(format!(
                        "MCP JSON-RPC error {}: {}",
                        err.code, err.message
                    )));
                }
                return rpc
                    .result
                    .ok_or_else(|| ToolError::Failed("MCP stdio empty result".into()));
            }
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), ToolError> {
        if !self.is_connected() {
            return Err(ToolError::Failed(format!(
                "MCP stdio '{}' disconnected (fail closed)",
                self.server_id
            )));
        }
        let note = JsonRpcNotification::new(method, params);
        let body = serde_json::to_string(&note)
            .map_err(|e| ToolError::Failed(format!("MCP notify encode: {e}")))?;
        let framed = encode_framed(&body);
        let mut stdin_guard = self.stdin.lock().await;
        let stdin = stdin_guard.as_mut().ok_or_else(|| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP stdio '{}': stdin closed", self.server_id))
        })?;
        stdin.write_all(&framed).await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP stdio notify write: {e}"))
        })?;
        stdin.flush().await.map_err(|e| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP stdio notify flush: {e}"))
        })?;
        // Do not wait for a response (notifications are fire-and-forget).
        Ok(())
    }

    fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

impl StdioTransport {
    async fn read_one_frame(&self) -> Result<String, ToolError> {
        let mut buf = self.read_buf.lock().await;
        let mut stdout_guard = self.stdout.lock().await;
        let stdout = stdout_guard.as_mut().ok_or_else(|| {
            self.connected.store(false, Ordering::SeqCst);
            ToolError::Failed(format!("MCP stdio '{}': stdout closed", self.server_id))
        })?;

        loop {
            if let Some((body, n)) =
                try_decode_framed(&buf).map_err(|e| ToolError::Failed(format!("MCP frame: {e}")))?
            {
                buf.drain(..n);
                return Ok(body);
            }
            if buf.len() > MAX_FRAME_BUF {
                self.connected.store(false, Ordering::SeqCst);
                return Err(ToolError::Failed("MCP frame buffer overflow".into()));
            }
            let mut chunk = [0u8; 8192];
            let n = stdout.read(&mut chunk).await.map_err(|e| {
                self.connected.store(false, Ordering::SeqCst);
                ToolError::Failed(format!("MCP stdio read: {e}"))
            })?;
            if n == 0 {
                self.connected.store(false, Ordering::SeqCst);
                return Err(ToolError::Failed(format!(
                    "MCP stdio '{}': peer closed pipe",
                    self.server_id
                )));
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }
}

// --- Connect helpers -------------------------------------------------------

async fn initialize_session(
    transport: Arc<dyn McpTransport>,
    server_id: &str,
    filter: &[String],
    timeout_ms: u64,
    max_result_chars: usize,
) -> Result<McpSession, ToolError> {
    let limit = Duration::from_millis(timeout_ms);
    let _init = transport
        .request_timeout(
            "initialize",
            Some(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "keryx", "version": env!("CARGO_PKG_VERSION") }
            })),
            limit,
        )
        .await?;

    // MCP lifecycle: client → notifications/initialized (no response wait).
    // Fail soft if peer rejects; still attempt tools/list so a broken notify path
    // does not strand an otherwise healthy stdio/remote server.
    let _ = transport
        .notify("notifications/initialized", Some(json!({})))
        .await;

    let list_val = transport
        .request_timeout("tools/list", Some(json!({})), limit)
        .await?;
    let list: ToolsListResult = serde_json::from_value(list_val)
        .map_err(|e| ToolError::Failed(format!("MCP tools/list parse on '{server_id}': {e}")))?;

    let filter_set: HashSet<&str> = filter.iter().map(String::as_str).collect();
    let tools = list
        .tools
        .into_iter()
        .filter(|t| filter_set.is_empty() || filter_set.contains(t.name.as_str()))
        .map(|t: McpToolDef| RegisteredMcpTool {
            namespaced: namespaced_tool_name(server_id, &t.name),
            local_name: t.name,
            description: t.description.unwrap_or_else(|| "MCP tool".into()),
            parameters: t.input_schema.unwrap_or_else(empty_schema),
        })
        .collect();

    Ok(McpSession {
        server_id: server_id.to_string(),
        transport,
        tools,
        timeout: limit,
        max_result_chars,
    })
}

fn resolve_env_files(
    env: &BTreeMap<String, String>,
    env_files: &BTreeMap<String, PathBuf>,
) -> Result<BTreeMap<String, String>, String> {
    let mut out = env.clone();
    for (key, path) in env_files {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("env_files {key}={}: {e}", path.display()))?;
        out.insert(key.clone(), raw.trim().to_string());
    }
    Ok(out)
}

fn read_secret_file_or_env(
    env_name: Option<&String>,
    file: Option<&PathBuf>,
) -> Result<Option<String>, String> {
    if let Some(path) = file {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("auth_token_file {}: {e}", path.display()))?;
        let t = raw.trim().to_string();
        if t.is_empty() {
            return Ok(None);
        }
        return Ok(Some(t));
    }
    if let Some(name) = env_name {
        if let Ok(v) = std::env::var(name) {
            if !v.is_empty() {
                return Ok(Some(v));
            }
        }
    }
    Ok(None)
}

async fn connect_server(cfg: &McpServerConfig) -> Result<McpSession, String> {
    let timeout_ms = cfg.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let max_result = cfg.max_result_chars.unwrap_or(DEFAULT_MAX_RESULT_CHARS);

    match &cfg.transport {
        McpTransportConfig::Stdio { command, args } => {
            let extra_env = resolve_env_files(&cfg.env, &cfg.env_files)?;
            let mut cmd = Command::new(command);
            // Do not inherit Worker process env (KERYX_*, provider API keys, etc.).
            // Only safe base vars + operator-declared env / env_files.
            cmd.env_clear();
            for key in ["PATH", "HOME", "LANG", "TMPDIR", "TERM", "USER", "LOGNAME"] {
                if let Ok(v) = std::env::var(key) {
                    cmd.env(key, v);
                }
            }
            // macOS / some tooling need DYLD_* rarely; still never forward secrets by default.
            cmd.args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            if let Some(cwd) = &cfg.cwd {
                cmd.current_dir(cwd);
            }
            for (k, v) in &extra_env {
                cmd.env(k, v);
            }
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("spawn '{}': {e}", command))?;
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| "stdio: missing stdin".to_string())?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| "stdio: missing stdout".to_string())?;
            let transport: Arc<dyn McpTransport> = Arc::new(StdioTransport {
                child: AsyncMutex::new(Some(child)),
                stdin: AsyncMutex::new(Some(stdin)),
                stdout: AsyncMutex::new(Some(stdout)),
                read_buf: AsyncMutex::new(Vec::new()),
                connected: AtomicBool::new(true),
                next_id: AtomicU64::new(1),
                server_id: cfg.server_id.clone(),
            });
            initialize_session(
                transport,
                &cfg.server_id,
                &cfg.tool_filter,
                timeout_ms,
                max_result,
            )
            .await
            .map_err(|e| e.to_string())
        }
        McpTransportConfig::Remote {
            url,
            auth_token_env,
            auth_token_file,
        } => {
            let auth_token =
                read_secret_file_or_env(auth_token_env.as_ref(), auth_token_file.as_ref())?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(timeout_ms))
                .build()
                .map_err(|e| format!("http client: {e}"))?;
            let transport: Arc<dyn McpTransport> = Arc::new(RemoteHttpTransport {
                url: url.clone(),
                auth_token,
                client,
                connected: AtomicBool::new(true),
                next_id: AtomicU64::new(1),
            });
            initialize_session(
                transport,
                &cfg.server_id,
                &cfg.tool_filter,
                timeout_ms,
                max_result,
            )
            .await
            .map_err(|e| e.to_string())
        }
    }
}

// --- Registry --------------------------------------------------------------

/// Multi-server MCP client registry implementing [`ToolRuntime`].
pub struct McpClientRegistry {
    sessions: Mutex<HashMap<String, Arc<McpSession>>>,
    /// All registered namespaced names (for CompositeTools routing).
    names: Mutex<HashSet<String>>,
    doctor: Mutex<McpDoctorReport>,
    control_plane_extra: Vec<String>,
    high_blast: Vec<String>,
}

impl McpClientRegistry {
    /// Seam 1: build a registry from a mock peer (no live process).
    #[must_use]
    pub fn from_mock(
        server_id: &str,
        peer: Arc<MockMcpPeer>,
        policy_allowlist_local: &[String],
        high_blast_local: &[String],
    ) -> Self {
        let transport: Arc<dyn McpTransport> = Arc::new(MockTransport {
            peer: Arc::clone(&peer),
            connected: AtomicBool::new(true),
        });
        // Sync path via block_in_place not needed — mock request is instant; use ready tools.
        let tools: Vec<RegisteredMcpTool> = peer
            .local_tools()
            .into_iter()
            .map(|(local, desc, schema)| RegisteredMcpTool {
                namespaced: namespaced_tool_name(server_id, &local),
                local_name: local,
                description: desc,
                parameters: schema,
            })
            .collect();
        let session = Arc::new(McpSession {
            server_id: server_id.to_string(),
            transport,
            tools: tools.clone(),
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            max_result_chars: DEFAULT_MAX_RESULT_CHARS,
        });
        let discovered: Vec<String> = tools.iter().map(|t| t.namespaced.clone()).collect();
        let allowlisted: Vec<String> = policy_allowlist_local
            .iter()
            .map(|l| namespaced_tool_name(server_id, l))
            .collect();
        let high_blast: Vec<String> = high_blast_local
            .iter()
            .map(|l| namespaced_tool_name(server_id, l))
            .collect();
        let names: HashSet<String> = discovered.iter().cloned().collect();
        let mut sessions = HashMap::new();
        sessions.insert(server_id.to_string(), session);
        Self {
            sessions: Mutex::new(sessions),
            names: Mutex::new(names),
            doctor: Mutex::new(McpDoctorReport {
                servers: vec![McpServerHealth {
                    server_id: server_id.to_string(),
                    configured: true,
                    connected: true,
                    error: None,
                    discovered_tools: discovered,
                    allowlisted_tools: allowlisted.clone(),
                    high_blast_tools: high_blast.clone(),
                }],
            }),
            control_plane_extra: allowlisted,
            high_blast,
        }
    }

    /// Registered namespaced tool names (all connected servers).
    #[must_use]
    pub fn registered_names(&self) -> HashSet<String> {
        self.names.lock().map(|n| n.clone()).unwrap_or_default()
    }

    #[must_use]
    pub fn control_plane_extra_tools(&self) -> Vec<String> {
        self.control_plane_extra.clone()
    }

    #[must_use]
    pub fn high_blast_tools(&self) -> Vec<String> {
        self.high_blast.clone()
    }

    #[must_use]
    pub fn doctor_report(&self) -> McpDoctorReport {
        self.doctor.lock().map(|d| d.clone()).unwrap_or_default()
    }

    /// Fail-closed disconnect of all sessions (Worker shutdown / tests).
    pub fn shutdown(&self) {
        if let Ok(sessions) = self.sessions.lock() {
            for s in sessions.values() {
                s.disconnect();
            }
        }
    }

    /// Disconnect one server (Seam 1 fail-closed tests).
    pub fn disconnect_server(&self, server_id: &str) {
        if let Ok(sessions) = self.sessions.lock() {
            if let Some(s) = sessions.get(server_id) {
                s.disconnect();
            }
        }
        if let Ok(mut doc) = self.doctor.lock() {
            for h in &mut doc.servers {
                if h.server_id == server_id {
                    h.connected = false;
                    h.error = Some("disconnected".into());
                }
            }
        }
    }
}

impl Drop for McpClientRegistry {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[async_trait]
impl ToolRuntime for McpClientRegistry {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        let (server_id, local) = parse_namespaced_tool(&call.name).ok_or_else(|| {
            ToolError::Denied(format!("not an MCP namespaced tool '{}'", call.name))
        })?;
        let session = {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ToolError::Failed(e.to_string()))?;
            sessions.get(server_id).cloned().ok_or_else(|| {
                ToolError::Denied(format!(
                    "MCP server '{server_id}' not registered for tool '{}'",
                    call.name
                ))
            })?
        };
        if !session.is_connected() {
            return Err(ToolError::Failed(format!(
                "MCP server '{server_id}' disconnected (fail closed)"
            )));
        }
        // Ensure tool is known on this session.
        if !session.tools.iter().any(|t| t.local_name == local) {
            return Err(ToolError::Denied(format!(
                "MCP tool '{}' not registered on server '{server_id}'",
                call.name
            )));
        }
        let content = session.call_tool(local, call.arguments).await?;
        Ok(ToolResult {
            content,
            summary: format!("mcp_client tool={}", call.name),
        })
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        let sessions = match self.sessions.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for session in sessions.values() {
            if session.is_connected() {
                out.extend(session.catalog());
            }
        }
        out
    }
}

/// Build MCP runtimes from static config.
///
/// Partial failure: broken servers contribute zero tools; Worker may still start.
/// Never logs secret values.
pub async fn build_mcp_runtimes(config: &McpConfig) -> McpRuntimeBundle {
    let mut sessions: HashMap<String, Arc<McpSession>> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut control_plane_extra: Vec<String> = Vec::new();
    let mut high_blast: Vec<String> = Vec::new();
    let mut doctor_servers: Vec<McpServerHealth> = Vec::new();

    for cfg in &config.servers {
        if !cfg.enabled {
            doctor_servers.push(McpServerHealth {
                server_id: cfg.server_id.clone(),
                configured: true,
                connected: false,
                error: Some("disabled".into()),
                discovered_tools: Vec::new(),
                allowlisted_tools: Vec::new(),
                high_blast_tools: Vec::new(),
            });
            continue;
        }

        let allowlisted: Vec<String> = cfg
            .policy_allowlist
            .iter()
            .map(|l| namespaced_tool_name(&cfg.server_id, l))
            .collect();
        let hb: Vec<String> = cfg
            .high_blast
            .iter()
            .map(|l| namespaced_tool_name(&cfg.server_id, l))
            .collect();
        control_plane_extra.extend(allowlisted.iter().cloned());
        high_blast.extend(hb.iter().cloned());

        match connect_server(cfg).await {
            Ok(session) => {
                let discovered = session.registered_names();
                names.extend(discovered.iter().cloned());
                doctor_servers.push(McpServerHealth {
                    server_id: cfg.server_id.clone(),
                    configured: true,
                    connected: true,
                    error: None,
                    discovered_tools: discovered,
                    allowlisted_tools: allowlisted,
                    high_blast_tools: hb,
                });
                sessions.insert(cfg.server_id.clone(), Arc::new(session));
            }
            Err(err) => {
                // Fail closed for this server; Worker continues.
                doctor_servers.push(McpServerHealth {
                    server_id: cfg.server_id.clone(),
                    configured: true,
                    connected: false,
                    error: Some(err),
                    discovered_tools: Vec::new(),
                    allowlisted_tools: allowlisted,
                    high_blast_tools: hb,
                });
            }
        }
    }

    let doctor = McpDoctorReport {
        servers: doctor_servers,
    };

    let runtime = if sessions.is_empty() {
        None
    } else {
        Some(Arc::new(McpClientRegistry {
            sessions: Mutex::new(sessions),
            names: Mutex::new(names),
            doctor: Mutex::new(doctor.clone()),
            control_plane_extra: control_plane_extra.clone(),
            high_blast: high_blast.clone(),
        }))
    };

    McpRuntimeBundle {
        runtime,
        control_plane_extra,
        high_blast,
        doctor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::mock::MockMcpPeer;

    #[test]
    fn tools_call_malformed_result_fails_closed() {
        // Simulate the parse path used by McpSession::call_tool.
        let bad = json!("not-an-object");
        let err = serde_json::from_value::<ToolsCallResult>(bad).unwrap_err();
        assert!(!err.to_string().is_empty());
        // Valid empty object still deserializes (defaults) — not a parse failure.
        let ok: ToolsCallResult = serde_json::from_value(json!({})).unwrap();
        assert!(ok.content.is_empty());
        assert!(!ok.is_error);
    }

    #[tokio::test]
    async fn from_mock_register_invoke_and_disconnect() {
        let peer = Arc::new(
            MockMcpPeer::default()
                .with_tool("echo", "pong")
                .with_tool("send", "sent"),
        );
        let reg = McpClientRegistry::from_mock(
            "demo",
            Arc::clone(&peer),
            &["echo".into()],
            &["send".into()],
        );
        assert!(reg.registered_names().contains("mcp.demo.echo"));
        assert!(reg.registered_names().contains("mcp.demo.send"));
        assert_eq!(reg.control_plane_extra_tools(), vec!["mcp.demo.echo"]);
        assert_eq!(reg.high_blast_tools(), vec!["mcp.demo.send"]);
        let ok = reg
            .invoke(ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({}),
            })
            .await
            .unwrap();
        assert!(ok.content.contains("pong"));
        reg.disconnect_server("demo");
        let err = reg
            .invoke(ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({}),
            })
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("fail closed") || err.to_string().contains("disconnected"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn catalog_exposes_schemas() {
        let peer = Arc::new(MockMcpPeer::default().with_tool_schema(
            "search",
            "hits",
            "search mail",
            json!({"type":"object","properties":{"q":{"type":"string"}}}),
        ));
        let reg = McpClientRegistry::from_mock("mail", peer, &[], &[]);
        let cat = reg.catalog();
        assert_eq!(cat.len(), 1);
        assert_eq!(cat[0].name, "mcp.mail.search");
        assert!(cat[0].parameters.get("properties").is_some());
    }

    /// Fixture remote HTTP JSON-RPC peer (no live SaaS): initialize + notify + list + call.
    #[tokio::test]
    async fn remote_http_jsonrpc_fixture_connect_list_call() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

        struct McpJsonRpcFixture;

        impl Respond for McpJsonRpcFixture {
            fn respond(&self, req: &Request) -> ResponseTemplate {
                let body: Value = serde_json::from_slice(&req.body).unwrap_or(json!({}));
                let m = body.get("method").and_then(Value::as_str).unwrap_or("");
                // Notifications have no id.
                if body.get("id").is_none() {
                    return ResponseTemplate::new(204);
                }
                let id = body.get("id").cloned().unwrap_or(json!(1));
                let result = match m {
                    "initialize" => json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "fixture", "version": "0" }
                    }),
                    "tools/list" => json!({
                        "tools": [{
                            "name": "ping",
                            "description": "ping fixture",
                            "inputSchema": { "type": "object", "properties": {} }
                        }]
                    }),
                    "tools/call" => json!({
                        "content": [{ "type": "text", "text": "pong-remote" }],
                        "isError": false
                    }),
                    _ => json!({}),
                };
                ResponseTemplate::new(200).set_body_json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                }))
            }
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(McpJsonRpcFixture)
            .mount(&server)
            .await;

        let cfg = McpConfig {
            servers: vec![McpServerConfig {
                server_id: "remote_svc".into(),
                enabled: true,
                transport: McpTransportConfig::Remote {
                    url: format!("{}/mcp", server.uri()),
                    auth_token_env: None,
                    auth_token_file: None,
                },
                cwd: None,
                env: BTreeMap::new(),
                env_files: BTreeMap::new(),
                tool_filter: vec![],
                high_blast: vec![],
                policy_allowlist: vec!["ping".into()],
                timeout_ms: Some(5_000),
                max_result_chars: None,
            }],
        };
        let bundle = build_mcp_runtimes(&cfg).await;
        assert!(bundle.runtime.is_some(), "remote fixture should connect");
        assert!(
            bundle
                .control_plane_extra
                .iter()
                .any(|n| n == "mcp.remote_svc.ping"),
            "{:?}",
            bundle.control_plane_extra
        );
        let reg = bundle.runtime.unwrap();
        let ok = reg
            .invoke(ToolCall {
                name: "mcp.remote_svc.ping".into(),
                arguments: json!({}),
            })
            .await
            .unwrap();
        assert!(ok.content.contains("pong-remote"), "{}", ok.content);
    }
}
