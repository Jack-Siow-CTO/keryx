use crate::auth::OperatorTokenTable;
use keryx_app::ControlPlaneService;
use std::sync::Arc;

/// Shared control-plane state for the HTTP adapter.
#[derive(Clone)]
pub struct AppState {
    pub control: Arc<dyn ControlPlaneService>,
    pub tokens: Arc<OperatorTokenTable>,
}

impl AppState {
    #[must_use]
    pub fn new(control: Arc<dyn ControlPlaneService>, tokens: OperatorTokenTable) -> Self {
        Self {
            control,
            tokens: Arc::new(tokens),
        }
    }
}
