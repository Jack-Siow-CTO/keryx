//! Computer-use against an **isolated agent desktop** (not personal Mac session).

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use keryx_domain::RunOrigin;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Mutex;

/// Isolated agent desktop double (screenshot + input). Never attaches to daily desktop by default.
#[derive(Debug, Default)]
pub struct IsolatedDesktop {
    frame: Mutex<String>,
    attach_personal: bool,
}

impl IsolatedDesktop {
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: Mutex::new("desktop-frame-0".into()),
            attach_personal: false,
        }
    }

    #[must_use]
    pub fn personal_attach_enabled(&self) -> bool {
        self.attach_personal
    }
}

pub struct ComputerUseTools {
    allowed: HashSet<String>,
    origin: RunOrigin,
    desktop: std::sync::Arc<IsolatedDesktop>,
}

impl ComputerUseTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        origin: RunOrigin,
        desktop: std::sync::Arc<IsolatedDesktop>,
    ) -> Self {
        Self {
            allowed,
            origin,
            desktop,
        }
    }
}

#[async_trait]
impl ToolRuntime for ComputerUseTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        // Prefer browser tools for web; computer-use for non-browser GUIs.
        if call
            .arguments
            .get("prefer_browser_hint")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(ToolError::Failed(
                "prefer browser tools for web tasks; computer-use is for desktop apps".into(),
            ));
        }
        // Personal Mac / daily desktop attach is not the default path.
        if call
            .arguments
            .get("attach_personal_desktop")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(ToolError::Denied(
                "attach to personal desktop is disabled by default".into(),
            ));
        }
        if self.origin.is_reduced_trust() {
            return Err(ToolError::Denied(
                "computer-use denied for reduced origin by default (fail closed)".into(),
            ));
        }
        match call.name.as_str() {
            "computer_screenshot" => {
                let frame = self
                    .desktop
                    .frame
                    .lock()
                    .map_err(|e| ToolError::Failed(e.to_string()))?
                    .clone();
                Ok(ToolResult {
                    content: format!("isolated agent desktop frame={frame}"),
                    summary: "computer_screenshot isolated=true personal=false".into(),
                })
            }
            "computer_click" | "computer_type" | "computer_key" => {
                let action = call.name.as_str();
                Ok(ToolResult {
                    content: format!("{action} on isolated agent desktop"),
                    summary: format!("{action} isolated=true"),
                })
            }
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}
