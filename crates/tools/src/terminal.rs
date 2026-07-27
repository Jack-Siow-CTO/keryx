//! Terminal/process Tools: local and Docker backends under Policy.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use keryx_domain::RunOrigin;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

/// Exec backend selected by Policy / Run origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecBackend {
    Local,
    Docker,
}

/// Pluggable command execution (Seam 1 doubles + real local/docker).
#[async_trait]
pub trait ExecBackendRunner: Send + Sync {
    async fn run(
        &self,
        backend: ExecBackend,
        command: &str,
        cwd: Option<&Path>,
    ) -> Result<String, ToolError>;
}

/// Real local `sh -c` and `docker run` backends.
#[derive(Debug, Default)]
pub struct SystemExecRunner {
    /// Docker image for docker backend (default alpine).
    pub docker_image: String,
}

impl SystemExecRunner {
    #[must_use]
    pub fn new() -> Self {
        Self {
            docker_image: std::env::var("KERYX_DOCKER_IMAGE")
                .unwrap_or_else(|_| "alpine:3.20".into()),
        }
    }
}

#[async_trait]
impl ExecBackendRunner for SystemExecRunner {
    async fn run(
        &self,
        backend: ExecBackend,
        command: &str,
        cwd: Option<&Path>,
    ) -> Result<String, ToolError> {
        match backend {
            ExecBackend::Local => {
                let mut cmd = tokio::process::Command::new("sh");
                cmd.arg("-c")
                    .arg(command)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if let Some(c) = cwd {
                    cmd.current_dir(c);
                }
                let out = cmd
                    .output()
                    .await
                    .map_err(|e| ToolError::Failed(format!("local exec: {e}")))?;
                let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                if !out.stderr.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                if !out.status.success() {
                    return Err(ToolError::Failed(format!(
                        "local exec exit={}: {}",
                        out.status.code().unwrap_or(-1),
                        truncate(&text, 500)
                    )));
                }
                Ok(truncate(&text, 8_000))
            }
            ExecBackend::Docker => {
                let args = vec![
                    "run".into(),
                    "--rm".into(),
                    self.docker_image.clone(),
                    "sh".into(),
                    "-c".into(),
                    command.to_string(),
                ];
                let _ = cwd; // cwd mapped via volume could be future work
                let out = tokio::process::Command::new("docker")
                    .args(&args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| ToolError::Failed(format!("docker exec: {e}")))?;
                let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                if !out.stderr.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                if !out.status.success() {
                    return Err(ToolError::Failed(format!(
                        "docker exec exit={}: {}",
                        out.status.code().unwrap_or(-1),
                        truncate(&text, 500)
                    )));
                }
                Ok(truncate(&text, 8_000))
            }
        }
    }
}

/// Deterministic double for Seam 1 (no real shell/docker).
#[derive(Debug, Default)]
pub struct FixedExecRunner {
    pub responses: Mutex<Vec<String>>,
}

#[async_trait]
impl ExecBackendRunner for FixedExecRunner {
    async fn run(
        &self,
        backend: ExecBackend,
        command: &str,
        _cwd: Option<&Path>,
    ) -> Result<String, ToolError> {
        if command.contains("DENY") {
            return Err(ToolError::Failed("fixed runner deny".into()));
        }
        let mut guard = self
            .responses
            .lock()
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        if let Some(r) = guard.pop() {
            return Ok(format!("[{backend:?}] {r}"));
        }
        Ok(format!("[{backend:?}] ran: {command}"))
    }
}

/// Terminal tool with origin-aware backend defaults.
pub struct TerminalTools {
    allowed: HashSet<String>,
    runner: std::sync::Arc<dyn ExecBackendRunner>,
    /// Allowed cwd roots (path jail for cwd).
    cwd_roots: Vec<PathBuf>,
    /// When set, force this backend (tests); else derive from origin.
    force_backend: Option<ExecBackend>,
    /// Run origin for backend selection (set per harness / worker).
    origin: RunOrigin,
    /// Local exec requires Approval (high-blast) when true.
    local_requires_approval: bool,
    /// Pending approval callback — returns true if approved.
    /// For Seam 1, control plane Approval path is used via high-blast naming.
    pub high_blast_local: bool,
}

impl TerminalTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        runner: std::sync::Arc<dyn ExecBackendRunner>,
        origin: RunOrigin,
    ) -> Self {
        Self {
            allowed,
            runner,
            cwd_roots: Vec::new(),
            force_backend: None,
            origin,
            local_requires_approval: true,
            high_blast_local: true,
        }
    }

    #[must_use]
    pub fn with_cwd_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.cwd_roots = roots;
        self
    }

    #[must_use]
    pub fn with_force_backend(mut self, backend: ExecBackend) -> Self {
        self.force_backend = Some(backend);
        self
    }

    fn select_backend(&self, requested: Option<&str>) -> Result<ExecBackend, ToolError> {
        if let Some(forced) = self.force_backend {
            return Ok(forced);
        }
        if let Some(r) = requested {
            return match r {
                "local" => Ok(ExecBackend::Local),
                "docker" => Ok(ExecBackend::Docker),
                other => Err(ToolError::Failed(format!("unknown exec backend '{other}'"))),
            };
        }
        // Defaults by Run origin: reduced → Docker; control_plane → local.
        if self.origin.is_reduced_trust() {
            Ok(ExecBackend::Docker)
        } else {
            Ok(ExecBackend::Local)
        }
    }
}

#[async_trait]
impl ToolRuntime for TerminalTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "run_terminal" | "shell_exec" => self.run_terminal(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        let mut out = Vec::new();
        if self.allowed.contains("run_terminal") {
            out.push(ToolSpec::empty_params(
                "run_terminal",
                "Run a shell command (backend local or docker)",
            ));
        }
        if self.allowed.contains("shell_exec") {
            out.push(ToolSpec::empty_params(
                "shell_exec",
                "Run a shell command (alias of run_terminal)",
            ));
        }
        out
    }
}

impl TerminalTools {
    async fn run_terminal(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let command = args
            .get("command")
            .or_else(|| args.get("cmd"))
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing command".into()))?
            .to_string();
        if command.trim().is_empty() {
            return Err(ToolError::Failed("empty command".into()));
        }
        // Fail closed: reject obvious path escapes in command shell chaining is still power —
        // command allowlist is Policy-level; here we constrain cwd only.
        let backend = self.select_backend(args.get("backend").and_then(Value::as_str))?;

        if backend == ExecBackend::Local
            && self.local_requires_approval
            && self.high_blast_local
            && self.origin == RunOrigin::ControlPlane
        {
            // Signal high-blast for Approval gate in agent loop (same pattern as Soul edit).
            // Agent loop checks tool name + path; for terminal we use Denied that service
            // can map — instead return a structured fail that approval path handles via
            // tool name shell_exec high-blast list.
            // Actual Approval is enforced by ControlPlane when tool is in high-blast set.
        }

        // Reduced origin may not use local unless escalated.
        if backend == ExecBackend::Local && self.origin.is_reduced_trust() {
            return Err(ToolError::Denied(
                "local exec denied for reduced Run origin (use docker backend)".into(),
            ));
        }

        let cwd = if let Some(c) = args.get("cwd").and_then(Value::as_str) {
            Some(resolve_cwd(&self.cwd_roots, c)?)
        } else {
            None
        };

        let output = self.runner.run(backend, &command, cwd.as_deref()).await?;
        let summary = format!(
            "run_terminal backend={backend:?} cmd={} out_chars={}",
            truncate(&command, 40),
            output.chars().count()
        );
        Ok(ToolResult {
            content: output,
            summary,
        })
    }
}

fn resolve_cwd(roots: &[PathBuf], user: &str) -> Result<PathBuf, ToolError> {
    if roots.is_empty() {
        return Err(ToolError::PathJail("no cwd roots configured".into()));
    }
    if user.contains("..") {
        return Err(ToolError::PathJail("cwd escapes roots".into()));
    }
    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|e| ToolError::PathJail(e.to_string()))?;
        let joined = root.join(user);
        let resolved = if joined.exists() {
            joined
                .canonicalize()
                .map_err(|e| ToolError::PathJail(e.to_string()))?
        } else {
            return Err(ToolError::PathJail("cwd does not exist".into()));
        };
        if resolved.starts_with(&root) {
            return Ok(resolved);
        }
    }
    Err(ToolError::PathJail("cwd outside allowlisted roots".into()))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}
