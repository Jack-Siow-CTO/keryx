use crate::auth::OperatorTokenTable;
use keryx_app::ControlPlaneService;
use serde::Serialize;
use std::path::PathBuf;
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
    /// Optional skills root for Console read-mostly Skills API (ADR 0030).
    pub skills_root: Option<PathBuf>,
    /// Optional Worker data dir for Artifact blobs (ADR 0026).
    pub artifacts_dir: Option<PathBuf>,
    /// When true, mount Seam 1 Artifact PUT (never default for production Worker).
    pub allow_artifact_put: bool,
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
            skills_root: None,
            artifacts_dir: None,
            allow_artifact_put: false,
        }
    }

    #[must_use]
    pub fn with_console_paths(
        mut self,
        skills_root: Option<PathBuf>,
        artifacts_dir: Option<PathBuf>,
    ) -> Self {
        self.skills_root = skills_root;
        self.artifacts_dir = artifacts_dir;
        self
    }

    #[must_use]
    pub fn with_artifact_put(mut self, allow: bool) -> Self {
        self.allow_artifact_put = allow;
        self
    }
}
