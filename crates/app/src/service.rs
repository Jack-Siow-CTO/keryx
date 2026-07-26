use crate::error::AppError;
use crate::events::RunEventHub;
use crate::model::{ModelProvider, ModelRequest};
use crate::store::SessionStore;
use async_trait::async_trait;
use keryx_domain::{Principal, Run, RunEvent, RunEventKind, RunId, Session, SessionId};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Application service for Session/Run lifecycle and the agent loop.
pub struct ControlPlane<S, M> {
    store: Arc<S>,
    model: Arc<M>,
    events: Arc<RunEventHub>,
}

impl<S, M> ControlPlane<S, M>
where
    S: SessionStore,
    M: ModelProvider,
{
    #[must_use]
    pub fn new(store: Arc<S>, model: Arc<M>) -> Self {
        Self {
            store,
            model,
            events: Arc::new(RunEventHub::new()),
        }
    }

    #[must_use]
    pub fn with_event_hub(store: Arc<S>, model: Arc<M>, events: Arc<RunEventHub>) -> Self {
        Self {
            store,
            model,
            events,
        }
    }

    #[must_use]
    pub fn event_hub(&self) -> Arc<RunEventHub> {
        Arc::clone(&self.events)
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
    async fn subscribe_run_events(
        &self,
        run_id: RunId,
    ) -> Result<(Vec<RunEvent>, broadcast::Receiver<RunEvent>), AppError>;
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

    /// Start a Run and execute the agent loop in the background so clients can subscribe to SSE.
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

        let run = Run::start(session.id, principal.id, goal.clone());
        self.store
            .create_run(run.clone())
            .await
            .map_err(AppError::Store)?;

        let store = Arc::clone(&self.store);
        let model = Arc::clone(&self.model);
        let events = Arc::clone(&self.events);
        let run_id = run.id;
        let goal_for_model = goal;

        tokio::spawn(async move {
            if let Err(err) = execute_agent_loop(store, model, events, run_id, goal_for_model).await
            {
                // Best-effort: loop already records failures on the Run when possible.
                let _ = err;
            }
        });

        Ok(run)
    }

    async fn get_run(&self, run_id: RunId) -> Result<Run, AppError> {
        self.store
            .get_run(run_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::RunNotFound)
    }

    async fn subscribe_run_events(
        &self,
        run_id: RunId,
    ) -> Result<(Vec<RunEvent>, broadcast::Receiver<RunEvent>), AppError> {
        // Ensure the Run exists before opening a stream.
        let _ = self.get_run(run_id).await?;
        self.events.subscribe(run_id).map_err(AppError::Store)
    }

    async fn count_sessions(&self) -> Result<usize, AppError> {
        self.store.count_sessions().await.map_err(AppError::Store)
    }

    async fn count_runs(&self) -> Result<usize, AppError> {
        self.store.count_runs().await.map_err(AppError::Store)
    }
}

async fn execute_agent_loop<S, M>(
    store: Arc<S>,
    model: Arc<M>,
    events: Arc<RunEventHub>,
    run_id: RunId,
    goal: String,
) -> Result<(), AppError>
where
    S: SessionStore,
    M: ModelProvider,
{
    let publish = |kind: RunEventKind| -> Result<(), AppError> {
        events
            .publish(run_id, kind)
            .map(|_| ())
            .map_err(AppError::Store)
    };

    publish(RunEventKind::RunStarted)?;
    publish(RunEventKind::ModelStarted)?;

    let mut run = store
        .get_run(run_id)
        .await
        .map_err(AppError::Store)?
        .ok_or(AppError::RunNotFound)?;

    match model.complete(ModelRequest { goal }).await {
        Ok(response) => {
            for text in response.deltas {
                publish(RunEventKind::ModelDelta { text })?;
            }
            publish(RunEventKind::ModelFinished)?;
            run.complete(response.content);
            store.update_run(run).await.map_err(AppError::Store)?;
            publish(RunEventKind::RunCompleted)?;
        }
        Err(err) => {
            let reason = err.to_string();
            run.fail(reason.clone());
            store.update_run(run).await.map_err(AppError::Store)?;
            publish(RunEventKind::RunFailed { reason })?;
        }
    }

    Ok(())
}
