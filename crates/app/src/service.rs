use crate::error::AppError;
use crate::model::{ModelProvider, ModelRequest};
use crate::store::SessionStore;
use async_trait::async_trait;
use keryx_domain::{Principal, Run, RunId, Session, SessionId};
use std::sync::Arc;

/// Application service for Session/Run lifecycle and the agent loop.
pub struct ControlPlane<S, M> {
    store: Arc<S>,
    model: Arc<M>,
}

impl<S, M> ControlPlane<S, M>
where
    S: SessionStore,
    M: ModelProvider,
{
    #[must_use]
    pub fn new(store: Arc<S>, model: Arc<M>) -> Self {
        Self { store, model }
    }
}

/// Object-safe control-plane surface used by the HTTP adapter (and Seam 1 tests).
#[async_trait]
pub trait ControlPlaneService: Send + Sync {
    async fn create_session(&self, principal: Principal) -> Result<Session, AppError>;
    async fn start_run(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
    ) -> Result<Run, AppError>;
    async fn get_run(&self, run_id: RunId) -> Result<Run, AppError>;
    async fn count_sessions(&self) -> Result<usize, AppError>;
    async fn count_runs(&self) -> Result<usize, AppError>;
}

#[async_trait]
impl<S, M> ControlPlaneService for ControlPlane<S, M>
where
    S: SessionStore + 'static,
    M: ModelProvider + 'static,
{
    async fn create_session(&self, principal: Principal) -> Result<Session, AppError> {
        let session = Session::new(principal.id);
        self.store
            .create_session(session.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(session)
    }

    /// Start a Run on a Session and execute the agent loop to a terminal state.
    ///
    /// v1 Hello Run: single model completion, no tools. Synchronous within the request
    /// so the client receives a completed Run record (SSE/async execution lands later).
    async fn start_run(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
    ) -> Result<Run, AppError> {
        let session = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;

        let mut run = Run::start(session.id, principal.id, goal.clone());
        self.store
            .create_run(run.clone())
            .await
            .map_err(AppError::Store)?;

        match self.model.complete(ModelRequest { goal }).await {
            Ok(response) => {
                run.complete(response.content);
            }
            Err(err) => {
                run.fail(err.to_string());
            }
        }

        self.store
            .update_run(run.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(run)
    }

    async fn get_run(&self, run_id: RunId) -> Result<Run, AppError> {
        self.store
            .get_run(run_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::RunNotFound)
    }

    async fn count_sessions(&self) -> Result<usize, AppError> {
        self.store.count_sessions().await.map_err(AppError::Store)
    }

    async fn count_runs(&self) -> Result<usize, AppError> {
        self.store.count_runs().await.map_err(AppError::Store)
    }
}
