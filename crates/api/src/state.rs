use crate::auth::OperatorTokenTable;
use keryx_app::ControlPlaneService;
use serde::Serialize;
use std::sync::Arc;

/// Non-secret model provider catalog for `GET /v1/providers`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProviderCatalog {
    pub default: Option<String>,
    pub providers: Vec<ProviderInfo>,
}

/// One registered (or known) model provider entry.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub auth_kind: String,
    pub display_name: String,
    pub default_model: String,
    pub models: Vec<String>,
    pub registered: bool,
    pub supports_model_override: bool,
}

/// Shared control-plane state for the HTTP adapter.
#[derive(Clone)]
pub struct AppState {
    pub control: Arc<dyn ControlPlaneService>,
    pub tokens: Arc<OperatorTokenTable>,
    pub providers: Arc<ProviderCatalog>,
}

impl AppState {
    #[must_use]
    pub fn new(control: Arc<dyn ControlPlaneService>, tokens: OperatorTokenTable) -> Self {
        Self::with_providers(control, tokens, ProviderCatalog::default())
    }

    #[must_use]
    pub fn with_providers(
        control: Arc<dyn ControlPlaneService>,
        tokens: OperatorTokenTable,
        providers: ProviderCatalog,
    ) -> Self {
        Self {
            control,
            tokens: Arc::new(tokens),
            providers: Arc::new(providers),
        }
    }
}
