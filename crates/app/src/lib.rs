//! Keryx app: agent loop orchestration, budgets, Active Run exclusivity, global cap, cancel.
//!
//! Depends on domain ports/types only—not concrete adapters (ADR 0008).

mod approval_broker;
mod context;
mod error;
mod events;
mod limits;
mod model;
mod registry;
mod service;
mod store;
mod tools;

pub use approval_broker::ApprovalBroker;
pub use context::{
    load_run_context, path_targets_protected, resolve_context_path_jailed, LoadedRunContext,
    MissingContextPolicy, RunContextConfig,
};
pub use error::AppError;
pub use events::RunEventHub;
pub use limits::{RunBudgets, RunLimits};
pub use model::{ModelError, ModelProvider, ModelRequest, ModelResponse};
pub use service::{ControlPlane, ControlPlaneService, APPROVAL_TIMEOUT};
pub use store::SessionStore;
pub use tools::{
    catalog_for_policy, summarize_tool_args, DenyAllTools, ToolCall, ToolError, ToolResult,
    ToolRuntime, ToolSpec,
};

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
