//! Isolated browser tools (Seam 1 doubles; production uses isolated profile, not daily browser).

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use keryx_domain::RunOrigin;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Mutex;

/// In-memory isolated browser profile (not consumer-web model cookies).
#[derive(Debug, Default)]
pub struct IsolatedBrowserState {
    url: Mutex<String>,
    tabs: Mutex<Vec<String>>,
    domain_allowlist: HashSet<String>,
}

impl IsolatedBrowserState {
    #[must_use]
    pub fn new(allowlist: HashSet<String>) -> Self {
        Self {
            url: Mutex::new(String::new()),
            tabs: Mutex::new(vec!["about:blank".into()]),
            domain_allowlist: allowlist,
        }
    }
}

pub struct BrowserTools {
    allowed: HashSet<String>,
    origin: RunOrigin,
    state: std::sync::Arc<IsolatedBrowserState>,
}

impl BrowserTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        origin: RunOrigin,
        state: std::sync::Arc<IsolatedBrowserState>,
    ) -> Self {
        Self {
            allowed,
            origin,
            state,
        }
    }
}

#[async_trait]
impl ToolRuntime for BrowserTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        // Prefer browser tools for web (documented); isolated profile only.
        match call.name.as_str() {
            "browser_navigate" => self.navigate(&call.arguments).await,
            "browser_snapshot" | "browser_screenshot" => self.snapshot().await,
            "browser_click" => self.click(&call.arguments).await,
            "browser_type" => self.type_text(&call.arguments).await,
            "browser_wait" => Ok(ToolResult {
                content: "waited".into(),
                summary: "browser_wait".into(),
            }),
            "browser_tabs" => {
                let tabs = self.state.tabs.lock().map_err(|e| ToolError::Failed(e.to_string()))?;
                Ok(ToolResult {
                    content: tabs.join("\n"),
                    summary: format!("browser_tabs n={}", tabs.len()),
                })
            }
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}

impl BrowserTools {
    async fn navigate(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing url".into()))?;
        // High-blast unrestricted navigation: reduced origin limited to allowlist.
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                if !self.state.domain_allowlist.is_empty()
                    && !self.state.domain_allowlist.iter().any(|d| host.ends_with(d))
                {
                    // Approval-class deny for high-blast off-allowlist (fail closed for reduced).
                    if self.origin.is_reduced_trust() {
                        return Err(ToolError::Denied(format!(
                            "browser_navigate domain '{host}' not allowlisted (reduced origin)"
                        )));
                    }
                    return Err(ToolError::Denied(format!(
                        "browser_navigate domain '{host}' not allowlisted (high-blast; Approval required)"
                    )));
                }
            }
        }
        *self
            .state
            .url
            .lock()
            .map_err(|e| ToolError::Failed(e.to_string()))? = url.to_string();
        if let Ok(mut tabs) = self.state.tabs.lock() {
            if let Some(t) = tabs.first_mut() {
                *t = url.to_string();
            }
        }
        Ok(ToolResult {
            content: format!("navigated isolated profile to {url}"),
            summary: format!("browser_navigate url={}", truncate(url, 60)),
        })
    }

    async fn snapshot(&self) -> Result<ToolResult, ToolError> {
        let url = self
            .state
            .url
            .lock()
            .map_err(|e| ToolError::Failed(e.to_string()))?
            .clone();
        Ok(ToolResult {
            content: format!("snapshot url={url} (isolated; not consumer-web cookies)"),
            summary: "browser_snapshot".into(),
        })
    }

    async fn click(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let sel = args
            .get("selector")
            .and_then(Value::as_str)
            .unwrap_or("body");
        Ok(ToolResult {
            content: format!("clicked {sel}"),
            summary: format!("browser_click sel={}", truncate(sel, 40)),
        })
    }

    async fn type_text(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("");
        // Redact secrets-like in summary only.
        Ok(ToolResult {
            content: format!("typed {} chars", text.len()),
            summary: format!("browser_type chars={}", text.len()),
        })
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}
