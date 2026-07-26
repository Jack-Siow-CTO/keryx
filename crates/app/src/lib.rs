//! Keryx app: agent loop orchestration, budgets, Active Run exclusivity, global cap, cancel.
//!
//! Depends on domain ports/types only—not concrete adapters (ADR 0008).

mod error;
mod events;
mod limits;
mod model;
mod registry;
mod service;
mod store;
mod tools;

pub use error::AppError;
pub use events::RunEventHub;
pub use limits::{RunBudgets, RunLimits};
pub use model::{ModelError, ModelProvider, ModelRequest, ModelResponse};
pub use service::{ControlPlane, ControlPlaneService};
pub use store::SessionStore;
pub use tools::{summarize_tool_args, DenyAllTools, ToolCall, ToolError, ToolResult, ToolRuntime};

/// Workspace smoke: app crate depends on domain and is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-app"
}

/// Confirms the domain dependency is wired (dependency direction: domain ← app).
#[must_use]
pub fn domain_crate_name() -> &'static str {
    keryx_domain::crate_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_smoke() {
        assert_eq!(crate_name(), "keryx-app");
        assert_eq!(domain_crate_name(), "keryx-domain");
    }
}
