use crate::error::AppError;
use crate::events::RunEventHub;
use crate::limits::{RunBudgets, RunLimits};
use crate::model::{ModelProvider, ModelRequest};
use crate::registry::ActiveRunRegistry;
use crate::store::SessionStore;
use crate::tools::{summarize_tool_args, DenyAllTools, ToolRuntime};
use async_trait::async_trait;
use keryx_domain::{
    Principal, Run, RunEvent, RunEventKind, RunId, RunStatus, Session, SessionId, TranscriptMessage,
};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const MAX_AGENT_STEPS: u32 = 8;

/// Application service for Session/Run lifecycle and the agent loop.
pub struct ControlPlane<S, M> {
    store: Arc<S>,
    model: Arc<M>,
    tools: Arc<dyn ToolRuntime>,
    events: Arc<RunEventHub>,
    limits: RunLimits,
    active: Arc<Mutex<ActiveRunRegistry>>,
}

impl<S, M> ControlPlane<S, M>
where
    S: SessionStore,
    M: ModelProvider,
{
    #[must_use]
    pub fn new(store: Arc<S>, model: Arc<M>) -> Self {
        Self::with_limits(store, model, RunLimits::default())
    }

    #[must_use]
    pub fn with_limits(store: Arc<S>, model: Arc<M>, limits: RunLimits) -> Self {
        Self::with_tools(store, model, limits, Arc::new(DenyAllTools))
    }

    #[must_use]
    pub fn with_tools(
        store: Arc<S>,
        model: Arc<M>,
        limits: RunLimits,
        tools: Arc<dyn ToolRuntime>,
    ) -> Self {
        Self {
            store,
            model,
            tools,
            events: Arc::new(RunEventHub::new()),
            limits,
            active: Arc::new(Mutex::new(ActiveRunRegistry::new())),
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
        provider: Option<String>,
        model: Option<String>,
    ) -> Result<Run, AppError>;
    async fn get_run(&self, run_id: RunId) -> Result<Run, AppError>;
    async fn cancel_run(&self, run_id: RunId) -> Result<Run, AppError>;
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

    async fn start_run(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
        provider: Option<String>,
        model_id: Option<String>,
    ) -> Result<Run, AppError> {
        let session = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;

        let (run, cancel) = {
            let mut active = self.active.lock().await;
            if let Some(existing) = active.active_for_session(session.id) {
                return Err(AppError::ActiveRunExists {
                    session_id: session.id,
                    run_id: existing,
                });
            }
            if active.active_count() >= self.limits.global_active_cap {
                return Err(AppError::GlobalCapExceeded {
                    cap: self.limits.global_active_cap,
                });
            }

            let run = Run::start(session.id, principal.id, goal.clone());
            self.store
                .create_run(run.clone())
                .await
                .map_err(AppError::Store)?;
            let cancel = active.register(session.id, run.id);
            (run, cancel)
        };

        let store = Arc::clone(&self.store);
        let model = Arc::clone(&self.model);
        let tools = Arc::clone(&self.tools);
        let events = Arc::clone(&self.events);
        let active = Arc::clone(&self.active);
        let budgets = self.limits.default_budgets.clone();
        let run_id = run.id;
        let run_session = run.session_id;

        tokio::spawn(async move {
            let _ = execute_agent_loop(
                store,
                model,
                tools,
                events,
                active,
                run_id,
                run_session,
                goal,
                provider,
                model_id,
                budgets,
                cancel,
            )
            .await;
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

    async fn cancel_run(&self, run_id: RunId) -> Result<Run, AppError> {
        let run = self.get_run(run_id).await?;
        if run.status != RunStatus::Active {
            return Err(AppError::RunNotActive);
        }

        if let Some(token) = self.active.lock().await.cancel_token(run_id) {
            token.cancel();
        }

        for _ in 0..100 {
            let run = self.get_run(run_id).await?;
            if run.status.is_terminal() {
                return Ok(run);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let mut run = self.get_run(run_id).await?;
        if !run.status.is_terminal() {
            run.cancel();
            self.store
                .update_run(run.clone())
                .await
                .map_err(AppError::Store)?;
            let _ = self.events.publish(run_id, RunEventKind::RunCancelled);
            self.active.lock().await.clear(run.session_id, run_id);
        }
        self.get_run(run_id).await
    }

    async fn subscribe_run_events(
        &self,
        run_id: RunId,
    ) -> Result<(Vec<RunEvent>, broadcast::Receiver<RunEvent>), AppError> {
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

async fn load_run<S: SessionStore>(store: &S, run_id: RunId) -> Result<Run, AppError> {
    store
        .get_run(run_id)
        .await
        .map_err(AppError::Store)?
        .ok_or(AppError::RunNotFound)
}

async fn clear_active(active: &Mutex<ActiveRunRegistry>, session_id: SessionId, run_id: RunId) {
    active.lock().await.clear(session_id, run_id);
}

async fn finalize_run<S: SessionStore>(
    store: &S,
    events: &RunEventHub,
    active: &Mutex<ActiveRunRegistry>,
    session_id: SessionId,
    run_id: RunId,
    run: Run,
    kind: RunEventKind,
) -> Result<(), AppError> {
    store.update_run(run).await.map_err(AppError::Store)?;
    events.publish(run_id, kind).map_err(AppError::Store)?;
    clear_active(active, session_id, run_id).await;
    Ok(())
}

#[allow(clippy::too_many_arguments)] // agent loop needs store, model, tools, events, registry, ids, budgets, cancel
async fn execute_agent_loop<S, M>(
    store: Arc<S>,
    model: Arc<M>,
    tools: Arc<dyn ToolRuntime>,
    events: Arc<RunEventHub>,
    active: Arc<Mutex<ActiveRunRegistry>>,
    run_id: RunId,
    session_id: SessionId,
    goal: String,
    provider: Option<String>,
    model_id: Option<String>,
    budgets: RunBudgets,
    cancel: CancellationToken,
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

    // User goal is part of durable Transcript once the Run completes successfully;
    // append up front so tool steps and subsequent model calls see it.
    store
        .append_transcript(session_id, TranscriptMessage::user(goal.clone()))
        .await
        .map_err(AppError::Store)?;

    let mut tool_calls_used: u64 = 0;

    for _step in 0..MAX_AGENT_STEPS {
        if cancel.is_cancelled() {
            let mut run = load_run(store.as_ref(), run_id).await?;
            run.cancel();
            return finalize_run(
                store.as_ref(),
                events.as_ref(),
                active.as_ref(),
                session_id,
                run_id,
                run,
                RunEventKind::RunCancelled,
            )
            .await;
        }

        publish(RunEventKind::ModelStarted)?;

        let transcript = store
            .get_transcript(session_id)
            .await
            .map_err(AppError::Store)?;
        let model_future = model.complete(ModelRequest {
            goal: goal.clone(),
            transcript: transcript.messages,
            provider: provider.clone(),
            model: model_id.clone(),
        });

        let model_result = if let Some(max_duration) = budgets.max_duration {
            tokio::select! {
                () = cancel.cancelled() => {
                    let mut run = load_run(store.as_ref(), run_id).await?;
                    run.cancel();
                    return finalize_run(
                        store.as_ref(),
                        events.as_ref(),
                        active.as_ref(),
                        session_id,
                        run_id,
                        run,
                        RunEventKind::RunCancelled,
                    ).await;
                }
                result = timeout(max_duration, model_future) => {
                    match result {
                        Ok(inner) => inner,
                        Err(_) => {
                            let mut run = load_run(store.as_ref(), run_id).await?;
                            let reason = "budget exceeded: time".to_string();
                            run.fail(reason.clone());
                            let _ = publish(RunEventKind::RunBudget {
                                message: reason.clone(),
                            });
                            return finalize_run(
                                store.as_ref(),
                                events.as_ref(),
                                active.as_ref(),
                                session_id,
                                run_id,
                                run,
                                RunEventKind::RunFailed { reason },
                            ).await;
                        }
                    }
                }
            }
        } else {
            tokio::select! {
                () = cancel.cancelled() => {
                    let mut run = load_run(store.as_ref(), run_id).await?;
                    run.cancel();
                    return finalize_run(
                        store.as_ref(),
                        events.as_ref(),
                        active.as_ref(),
                        session_id,
                        run_id,
                        run,
                        RunEventKind::RunCancelled,
                    ).await;
                }
                result = model_future => result,
            }
        };

        let mut run = load_run(store.as_ref(), run_id).await?;
        if cancel.is_cancelled() {
            run.cancel();
            return finalize_run(
                store.as_ref(),
                events.as_ref(),
                active.as_ref(),
                session_id,
                run_id,
                run,
                RunEventKind::RunCancelled,
            )
            .await;
        }

        let response = match model_result {
            Ok(r) => r,
            Err(err) => {
                let reason = err.to_string();
                run.fail(reason.clone());
                return finalize_run(
                    store.as_ref(),
                    events.as_ref(),
                    active.as_ref(),
                    session_id,
                    run_id,
                    run,
                    RunEventKind::RunFailed { reason },
                )
                .await;
            }
        };

        for text in response.deltas {
            if cancel.is_cancelled() {
                run.cancel();
                return finalize_run(
                    store.as_ref(),
                    events.as_ref(),
                    active.as_ref(),
                    session_id,
                    run_id,
                    run,
                    RunEventKind::RunCancelled,
                )
                .await;
            }
            publish(RunEventKind::ModelDelta { text })?;
        }
        publish(RunEventKind::ModelFinished)?;

        if let Some(max_tokens) = budgets.max_tokens {
            if response.tokens_used > max_tokens {
                let reason = format!(
                    "budget exceeded: tokens (used={}, max={max_tokens})",
                    response.tokens_used
                );
                run.fail(reason.clone());
                let _ = publish(RunEventKind::RunBudget {
                    message: reason.clone(),
                });
                return finalize_run(
                    store.as_ref(),
                    events.as_ref(),
                    active.as_ref(),
                    session_id,
                    run_id,
                    run,
                    RunEventKind::RunFailed { reason },
                )
                .await;
            }
        }

        if response.tool_calls.is_empty() {
            store
                .append_transcript(
                    session_id,
                    TranscriptMessage::assistant(response.content.clone()),
                )
                .await
                .map_err(AppError::Store)?;
            run.complete(response.content);
            return finalize_run(
                store.as_ref(),
                events.as_ref(),
                active.as_ref(),
                session_id,
                run_id,
                run,
                RunEventKind::RunCompleted,
            )
            .await;
        }

        // Tool phase
        for call in response.tool_calls {
            if cancel.is_cancelled() {
                run.cancel();
                return finalize_run(
                    store.as_ref(),
                    events.as_ref(),
                    active.as_ref(),
                    session_id,
                    run_id,
                    run,
                    RunEventKind::RunCancelled,
                )
                .await;
            }

            tool_calls_used += 1;
            if let Some(max_tool_calls) = budgets.max_tool_calls {
                if tool_calls_used > max_tool_calls {
                    let reason = format!(
                        "budget exceeded: tool_calls (used={tool_calls_used}, max={max_tool_calls})"
                    );
                    run.fail(reason.clone());
                    let _ = publish(RunEventKind::RunBudget {
                        message: reason.clone(),
                    });
                    return finalize_run(
                        store.as_ref(),
                        events.as_ref(),
                        active.as_ref(),
                        session_id,
                        run_id,
                        run,
                        RunEventKind::RunFailed { reason },
                    )
                    .await;
                }
            }

            let args_summary = summarize_tool_args(&call.arguments);
            publish(RunEventKind::ToolStarted {
                name: format!("{} ({args_summary})", call.name),
            })?;

            match tools.invoke(call.clone()).await {
                Ok(result) => {
                    publish(RunEventKind::ToolFinished {
                        name: format!("{}: {}", call.name, result.summary),
                    })?;
                    store
                        .append_transcript(
                            session_id,
                            TranscriptMessage {
                                role: keryx_domain::MessageRole::Tool,
                                content: format!("{}: {}", call.name, result.content),
                            },
                        )
                        .await
                        .map_err(AppError::Store)?;
                }
                Err(err) => {
                    let summary = err.to_string();
                    publish(RunEventKind::ToolFinished {
                        name: format!("{}: error={summary}", call.name),
                    })?;
                    // Policy deny / path jail fail closed but still record for the model / client.
                    store
                        .append_transcript(
                            session_id,
                            TranscriptMessage {
                                role: keryx_domain::MessageRole::Tool,
                                content: format!("{}: error={summary}", call.name),
                            },
                        )
                        .await
                        .map_err(AppError::Store)?;
                }
            }
        }
        // Continue agent loop with updated Transcript.
    }

    let mut run = load_run(store.as_ref(), run_id).await?;
    let reason = "agent loop exceeded max steps".to_string();
    run.fail(reason.clone());
    finalize_run(
        store.as_ref(),
        events.as_ref(),
        active.as_ref(),
        session_id,
        run_id,
        run,
        RunEventKind::RunFailed { reason },
    )
    .await
}
