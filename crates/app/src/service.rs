use crate::approval_broker::ApprovalBroker;
use crate::context::{load_run_context, path_targets_protected, RunContextConfig};
use crate::error::AppError;
use crate::events::RunEventHub;
use crate::limits::{RunBudgets, RunLimits};
use crate::model::{ModelProvider, ModelRequest};
use crate::registry::ActiveRunRegistry;
use crate::store::SessionStore;
use crate::tools::{catalog_for_policy, summarize_tool_args, DenyAllTools, ToolError, ToolRuntime};
use async_trait::async_trait;
use keryx_domain::{
    ActiveRootRunSummary, Approval, ApprovalId, ApprovalStatus, MessageRole, Policy, Principal,
    Run, RunEvent, RunEventKind, RunId, RunOrigin, RunStatus, Schedule, ScheduleId, Session,
    SessionId, SessionSummary, TranscriptMessage,
};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const MAX_AGENT_STEPS: u32 = 8;

/// Max wall time an agent loop waits for Principal Approval (fail closed on expiry).
///
/// Spec story 33: approval timeout → tool fails closed. Not configurable via RunBudgets
/// yet; override later if needed. Documented for operators / tests.
pub const APPROVAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Application service for Session/Run lifecycle and the agent loop.
pub struct ControlPlane<S, M> {
    store: Arc<S>,
    model: Arc<M>,
    tools: Arc<dyn ToolRuntime>,
    events: Arc<RunEventHub>,
    limits: RunLimits,
    active: Arc<Mutex<ActiveRunRegistry>>,
    /// Soul + Context file attachment config (distinct from Memory/Skill).
    run_context: RunContextConfig,
    approvals: Arc<ApprovalBroker>,
    /// Exact tool names (typically `mcp.<server>.<tool>`) added to control_plane Policy only.
    control_plane_extra_tools: BTreeSet<String>,
    /// Config-declared high-blast tools (MCP or other) requiring Approval — no name heuristics.
    high_blast_tools: HashSet<String>,
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
        Self::with_tools_and_context(store, model, limits, tools, RunContextConfig::default())
    }

    #[must_use]
    pub fn with_tools_and_context(
        store: Arc<S>,
        model: Arc<M>,
        limits: RunLimits,
        tools: Arc<dyn ToolRuntime>,
        run_context: RunContextConfig,
    ) -> Self {
        Self {
            store,
            model,
            tools,
            events: Arc::new(RunEventHub::new()),
            limits,
            active: Arc::new(Mutex::new(ActiveRunRegistry::new())),
            run_context,
            approvals: Arc::new(ApprovalBroker::new()),
            control_plane_extra_tools: BTreeSet::new(),
            high_blast_tools: HashSet::new(),
        }
    }

    /// Exact tool names merged into control_plane Policy only (not gateway/schedule).
    ///
    /// Used for operator MCP allowlists: connect ≠ allow.
    #[must_use]
    pub fn with_control_plane_extra_tools(
        mut self,
        tools: impl IntoIterator<Item = String>,
    ) -> Self {
        self.control_plane_extra_tools.extend(tools);
        self
    }

    /// Config-declared high-blast tool names (exact match → Approval path).
    #[must_use]
    pub fn with_high_blast_tools(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.high_blast_tools.extend(tools);
        self
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
    /// Operator Session list projection (title, active root, pending Approvals).
    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, AppError>;
    async fn get_session(&self, session_id: SessionId) -> Result<SessionSummary, AppError>;
    /// Rename Session title (durable on Worker). Empty title clears override.
    async fn patch_session_title(
        &self,
        session_id: SessionId,
        title: Option<String>,
    ) -> Result<SessionSummary, AppError>;
    /// Paged Transcript (newest first). `before` = exclusive older-bound message id.
    async fn get_transcript_page(
        &self,
        session_id: SessionId,
        limit: usize,
        before: Option<String>,
    ) -> Result<keryx_domain::TranscriptPage, AppError>;
    /// Start a Run with `origin=control_plane` (trusted control-plane API path).
    async fn start_run(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
        provider: Option<String>,
        model: Option<String>,
    ) -> Result<Run, AppError>;
    /// Start a Run with an explicit Run origin (Gateways, Schedules, Seam 1 reduced-Policy tests).
    ///
    /// HTTP control plane always uses [`start_run`] (control_plane origin). This method is for
    /// trusted internal adapters and tests that must exercise reduced-origin Policy templates.
    async fn start_run_with_origin(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
        origin: RunOrigin,
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
    async fn list_approvals(&self, pending_only: bool) -> Result<Vec<Approval>, AppError>;
    async fn get_approval(&self, id: ApprovalId) -> Result<Approval, AppError>;
    async fn approve(&self, principal: Principal, id: ApprovalId) -> Result<Approval, AppError>;
    async fn deny(&self, principal: Principal, id: ApprovalId) -> Result<Approval, AppError>;
    /// Spawn a Child Run under an Active parent (tool / internal control-plane path).
    async fn spawn_child_run(
        &self,
        parent_run_id: RunId,
        goal: String,
        max_tool_calls: Option<u64>,
    ) -> Result<Run, AppError>;

    async fn create_schedule(
        &self,
        principal: Principal,
        goal: String,
        interval_secs: u64,
        next_fire_at: i64,
        policy_tools: Option<Vec<String>>,
    ) -> Result<Schedule, AppError>;
    async fn list_schedules(&self) -> Result<Vec<Schedule>, AppError>;
    async fn get_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError>;
    async fn pause_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError>;
    async fn resume_schedule(&self, id: ScheduleId, now: i64) -> Result<Schedule, AppError>;
    async fn delete_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError>;
    /// Fire due Schedules at `now` (deterministic clock for tests). Returns started Runs.
    async fn tick_schedules(&self, now: i64) -> Result<Vec<Run>, AppError>;
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

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, AppError> {
        let sessions = self.store.list_sessions().await.map_err(AppError::Store)?;
        let mut out = Vec::with_capacity(sessions.len());
        for session in sessions {
            out.push(self.project_session(session).await?);
        }
        Ok(out)
    }

    async fn get_session(&self, session_id: SessionId) -> Result<SessionSummary, AppError> {
        let session = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;
        self.project_session(session).await
    }

    async fn patch_session_title(
        &self,
        session_id: SessionId,
        title: Option<String>,
    ) -> Result<SessionSummary, AppError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;
        match title {
            Some(t) => session.set_title(t),
            None => {
                session.title = None;
                session.touch();
            }
        }
        self.store
            .update_session(session.clone())
            .await
            .map_err(AppError::Store)?;
        self.project_session(session).await
    }

    async fn get_transcript_page(
        &self,
        session_id: SessionId,
        limit: usize,
        before: Option<String>,
    ) -> Result<keryx_domain::TranscriptPage, AppError> {
        let _ = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;
        let (messages, next_before) = self
            .store
            .get_transcript_page(session_id, limit, before.as_deref())
            .await
            .map_err(AppError::Store)?;
        Ok(keryx_domain::TranscriptPage {
            session_id: session_id.to_string(),
            messages,
            next_before,
        })
    }

    async fn start_run(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
        provider: Option<String>,
        model_id: Option<String>,
    ) -> Result<Run, AppError> {
        self.start_run_with_origin(
            principal,
            session_id,
            goal,
            RunOrigin::ControlPlane,
            provider,
            model_id,
        )
        .await
    }

    async fn start_run_with_origin(
        &self,
        principal: Principal,
        session_id: SessionId,
        goal: String,
        origin: RunOrigin,
        provider: Option<String>,
        model_id: Option<String>,
    ) -> Result<Run, AppError> {
        let mut session = self
            .store
            .get_session(session_id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::SessionNotFound)?;

        let (run, cancel, principal_id) = {
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

            let principal_id = principal.id.clone();
            let run =
                Run::start_with_origin(session.id, principal_id.clone(), goal.clone(), origin);
            self.store
                .create_run(run.clone())
                .await
                .map_err(AppError::Store)?;
            // Touch Session activity for list projection ordering.
            session.touch();
            let _ = self.store.update_session(session.clone()).await;
            let cancel = active.register(session.id, run.id);
            (run, cancel, principal_id)
        };

        let store = Arc::clone(&self.store);
        let model = Arc::clone(&self.model);
        let tools = Arc::clone(&self.tools);
        let events = Arc::clone(&self.events);
        let active = Arc::clone(&self.active);
        let approvals = Arc::clone(&self.approvals);
        let budgets = self.limits.default_budgets.clone();
        let run_context = self.run_context.clone();
        let control_plane_extra_tools = self.control_plane_extra_tools.clone();
        let high_blast_tools = self.high_blast_tools.clone();
        let run_id = run.id;
        let run_session = run.session_id;
        let run_origin = run.origin.clone();

        tokio::spawn(async move {
            let _ = execute_agent_loop(
                store,
                model,
                tools,
                events,
                active,
                approvals,
                run_id,
                run_session,
                principal_id,
                goal,
                run_origin,
                provider,
                model_id,
                budgets,
                run_context,
                control_plane_extra_tools,
                high_blast_tools,
                None, // root: derive Policy from origin + extras
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

        // Cancel root and all Child Runs in the tree.
        self.active.lock().await.cancel_tree(run_id);

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
            if run.is_root() {
                self.active.lock().await.clear(run.session_id, run_id);
            } else {
                self.active.lock().await.clear_child(run_id);
            }
        }
        self.get_run(run_id).await
    }

    async fn spawn_child_run(
        &self,
        parent_run_id: RunId,
        goal: String,
        max_tool_calls: Option<u64>,
    ) -> Result<Run, AppError> {
        self.spawn_child_run_inner(parent_run_id, goal, max_tool_calls)
            .await
    }

    async fn create_schedule(
        &self,
        principal: Principal,
        goal: String,
        interval_secs: u64,
        next_fire_at: i64,
        policy_tools: Option<Vec<String>>,
    ) -> Result<Schedule, AppError> {
        if goal.trim().is_empty() {
            return Err(AppError::Store("schedule goal must not be empty".into()));
        }
        // Frozen Policy snapshot: default to reduced (schedule origin) allowlist.
        let tools = policy_tools
            .unwrap_or_else(|| Policy::reduced().allowed_tools.iter().cloned().collect());
        let schedule = Schedule::new(principal.id, goal, interval_secs, next_fire_at, tools);
        self.store
            .create_schedule(schedule.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(schedule)
    }

    async fn list_schedules(&self) -> Result<Vec<Schedule>, AppError> {
        self.store.list_schedules().await.map_err(AppError::Store)
    }

    async fn get_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError> {
        self.store
            .get_schedule(id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::ScheduleNotFound)
    }

    async fn pause_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError> {
        let mut s = self.get_schedule(id).await?;
        s.pause();
        self.store
            .update_schedule(s.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(s)
    }

    async fn resume_schedule(&self, id: ScheduleId, now: i64) -> Result<Schedule, AppError> {
        let mut s = self.get_schedule(id).await?;
        s.resume(now);
        self.store
            .update_schedule(s.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(s)
    }

    async fn delete_schedule(&self, id: ScheduleId) -> Result<Schedule, AppError> {
        let mut s = self.get_schedule(id).await?;
        s.mark_deleted();
        self.store
            .update_schedule(s.clone())
            .await
            .map_err(AppError::Store)?;
        Ok(s)
    }

    async fn tick_schedules(&self, now: i64) -> Result<Vec<Run>, AppError> {
        let schedules = self.list_schedules().await?;
        let mut started = Vec::new();
        for mut schedule in schedules {
            if !schedule.is_due(now) {
                continue;
            }
            // Double-fire guard: if we already fired at this exact second, skip.
            if schedule.last_fired_at == Some(now) {
                continue;
            }
            let principal = Principal {
                id: schedule.principal_id.clone(),
            };
            let session_id = if let Some(sid) = schedule.session_id {
                sid
            } else {
                let session = self.create_session(principal.clone()).await?;
                schedule.session_id = Some(session.id);
                session.id
            };
            // Start Run with origin=schedule (reduced Policy via origin).
            match self
                .start_run_with_origin(
                    principal,
                    session_id,
                    schedule.goal.clone(),
                    RunOrigin::Schedule,
                    None,
                    None,
                )
                .await
            {
                Ok(run) => {
                    schedule.record_fire(now);
                    let _ = self.store.update_schedule(schedule).await;
                    started.push(run);
                }
                Err(AppError::ActiveRunExists { .. }) | Err(AppError::GlobalCapExceeded { .. }) => {
                    // Missed fire: leave next_fire_at so a later tick retries (no double-fire).
                    // Documented: overload skips this tick without advancing schedule.
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(started)
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

    async fn list_approvals(&self, pending_only: bool) -> Result<Vec<Approval>, AppError> {
        self.store
            .list_approvals(pending_only)
            .await
            .map_err(AppError::Store)
    }

    async fn get_approval(&self, id: ApprovalId) -> Result<Approval, AppError> {
        self.store
            .get_approval(id)
            .await
            .map_err(AppError::Store)?
            .ok_or(AppError::ApprovalNotFound)
    }

    async fn approve(&self, principal: Principal, id: ApprovalId) -> Result<Approval, AppError> {
        self.decide_approval(principal, id, true).await
    }

    async fn deny(&self, principal: Principal, id: ApprovalId) -> Result<Approval, AppError> {
        self.decide_approval(principal, id, false).await
    }
}

impl<S, M> ControlPlane<S, M>
where
    S: SessionStore + 'static,
    M: ModelProvider + 'static,
{
    /// Build operator Session list/detail projection (ADR 0027).
    async fn project_session(&self, session: Session) -> Result<SessionSummary, AppError> {
        let runs = self
            .store
            .list_runs_for_session(session.id)
            .await
            .map_err(AppError::Store)?;
        let transcript = self
            .store
            .get_transcript(session.id)
            .await
            .map_err(AppError::Store)?;
        // Prefer ordered first user Transcript message (stable "first goal"),
        // then any root Run goal as fallback.
        let first_user = transcript
            .messages
            .iter()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str());
        let first_root_goal = runs
            .iter()
            .filter(|r| r.is_root())
            .map(|r| r.goal.as_str())
            .next();
        let first_goal = first_user.or(first_root_goal);
        let title_is_custom = session
            .title
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let title = session.display_title_with_goal(first_goal);

        let active_root_run = runs
            .iter()
            .find(|r| r.is_root() && r.status == RunStatus::Active)
            .map(|r| ActiveRootRunSummary {
                id: r.id.to_string(),
                goal: r.goal.clone(),
                status: match r.status {
                    RunStatus::Active => "active".into(),
                    RunStatus::Completed => "completed".into(),
                    RunStatus::Failed => "failed".into(),
                    RunStatus::Cancelled => "cancelled".into(),
                    RunStatus::Interrupted => "interrupted".into(),
                },
                origin: r.origin.as_str().to_string(),
            });

        let last_message_preview = transcript
            .messages
            .last()
            .map(|m| truncate_preview(&m.content, 120));

        let run_ids: std::collections::HashSet<_> = runs.iter().map(|r| r.id).collect();
        let approvals = self
            .store
            .list_approvals(true)
            .await
            .map_err(AppError::Store)?;
        let pending_approval_count = approvals
            .iter()
            .filter(|a| run_ids.contains(&a.run_id))
            .count() as u32;

        Ok(SessionSummary {
            id: session.id.to_string(),
            principal_id: session.principal_id.to_string(),
            title,
            title_is_custom,
            created_at: session.created_at,
            updated_at: session.updated_at,
            last_message_preview,
            active_root_run,
            pending_approval_count,
        })
    }

    async fn spawn_child_run_inner(
        &self,
        parent_run_id: RunId,
        goal: String,
        max_tool_calls: Option<u64>,
    ) -> Result<Run, AppError> {
        if goal.trim().is_empty() {
            return Err(AppError::Store("child goal must not be empty".into()));
        }
        let parent = self.get_run(parent_run_id).await?;
        if parent.status != RunStatus::Active {
            return Err(AppError::RunNotActive);
        }
        if !parent.is_root() {
            // Vertical slice: only root may spawn (avoids deep trees for now).
            return Err(AppError::Store(
                "only root Runs may spawn Child Runs in this version".into(),
            ));
        }

        // Freeze parent Policy snapshot at spawn so later Worker config changes cannot
        // expand child authority mid-process. Children inherit exact parent allowlist;
        // if spawn API later accepts a tighter tool set, intersect via subset_of(parent).
        let parent_policy = policy_for_run(&parent.origin, &self.control_plane_extra_tools);
        let child_policy = parent_policy.clone();
        // Budgets carved from / capped by parent defaults.
        let child_budgets = self.limits.default_budgets.carve_for_child(max_tool_calls);

        let child = Run::start_child(
            parent.session_id,
            parent.principal_id.clone(),
            parent.id,
            goal.clone(),
            parent.origin.clone(),
        );
        self.store
            .create_run(child.clone())
            .await
            .map_err(AppError::Store)?;

        let cancel = {
            let mut active = self.active.lock().await;
            active.register_child(parent.id, child.id)
        };

        let _ = self.events.publish(
            parent.id,
            RunEventKind::ChildRunStarted {
                child_run_id: child.id.to_string(),
                goal: goal.clone(),
            },
        );

        let store = Arc::clone(&self.store);
        let model = Arc::clone(&self.model);
        let tools = Arc::clone(&self.tools);
        let events = Arc::clone(&self.events);
        let active = Arc::clone(&self.active);
        let approvals = Arc::clone(&self.approvals);
        let control_plane_extra_tools = self.control_plane_extra_tools.clone();
        let high_blast_tools = self.high_blast_tools.clone();
        // Child: isolated transcript slice — no Soul re-attach (parent already has identity).
        let run_context = RunContextConfig::default();
        let child_id = child.id;
        let session_id = child.session_id;
        let principal_id = child.principal_id.clone();
        let origin = child.origin.clone();
        let parent_id = parent.id;

        tokio::spawn(async move {
            let result = execute_agent_loop(
                store.clone(),
                model,
                tools,
                events.clone(),
                active.clone(),
                approvals,
                child_id,
                session_id,
                principal_id,
                goal,
                origin,
                None,
                None,
                child_budgets,
                run_context,
                control_plane_extra_tools,
                high_blast_tools,
                Some(child_policy),
                cancel,
            )
            .await;

            // Mark child clear in registry; notify parent.
            let status = store
                .get_run(child_id)
                .await
                .ok()
                .flatten()
                .map(|r| match r.status {
                    RunStatus::Active => "active",
                    RunStatus::Completed => "completed",
                    RunStatus::Failed => "failed",
                    RunStatus::Cancelled => "cancelled",
                    RunStatus::Interrupted => "interrupted",
                })
                .unwrap_or("unknown")
                .to_string();
            let _ = events.publish(
                parent_id,
                RunEventKind::ChildRunFinished {
                    child_run_id: child_id.to_string(),
                    status,
                },
            );
            active.lock().await.clear_child(child_id);
            let _ = result;
        });

        Ok(child)
    }

    async fn decide_approval(
        &self,
        principal: Principal,
        id: ApprovalId,
        approve: bool,
    ) -> Result<Approval, AppError> {
        let mut approval = self.get_approval(id).await?;
        if approval.status != ApprovalStatus::Pending {
            return Err(AppError::ApprovalNotPending);
        }
        if approve {
            approval.approve(principal.id.clone());
        } else {
            approval.deny(principal.id.clone());
        }
        // CAS-style: only transition from pending (concurrent decide → conflict).
        let updated = self
            .store
            .update_approval_if_pending(approval.clone())
            .await
            .map_err(AppError::Store)?;
        if !updated {
            return Err(AppError::ApprovalNotPending);
        }
        self.approvals.resolve(id, approval.status);
        let decision = if approve { "approved" } else { "denied" };
        let _ = self.events.publish(
            approval.run_id,
            RunEventKind::ApprovalResolved {
                approval_id: approval.id.to_string(),
                decision: decision.into(),
                principal_id: principal.id.to_string(),
            },
        );
        Ok(approval)
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

/// Build Policy for a Run: origin template + control_plane-only extras (MCP allowlist).
fn policy_for_run(origin: &RunOrigin, control_plane_extra: &BTreeSet<String>) -> Policy {
    let base = Policy::for_origin(origin);
    match origin {
        RunOrigin::ControlPlane if !control_plane_extra.is_empty() => {
            base.with_extra_tools(control_plane_extra.iter().cloned())
        }
        // Reduced origins never receive MCP extras by default (connect ≠ allow).
        _ => base,
    }
}

#[allow(clippy::too_many_arguments)] // agent loop orchestration bundle
async fn execute_agent_loop<S, M>(
    store: Arc<S>,
    model: Arc<M>,
    tools: Arc<dyn ToolRuntime>,
    events: Arc<RunEventHub>,
    active: Arc<Mutex<ActiveRunRegistry>>,
    approvals: Arc<ApprovalBroker>,
    run_id: RunId,
    session_id: SessionId,
    principal_id: keryx_domain::PrincipalId,
    goal: String,
    origin: RunOrigin,
    provider: Option<String>,
    model_id: Option<String>,
    budgets: RunBudgets,
    run_context: RunContextConfig,
    control_plane_extra_tools: BTreeSet<String>,
    high_blast_tools: HashSet<String>,
    // Frozen Policy snapshot (Child Runs). When set, used instead of re-deriving
    // from origin + live Worker extras so children cannot gain tools mid-process.
    policy_override: Option<Policy>,
    cancel: CancellationToken,
) -> Result<(), AppError>
where
    S: SessionStore,
    M: ModelProvider,
{
    // Origin-selected Policy template (fail closed for tools not on the allowlist),
    // or a frozen parent snapshot for Child Runs.
    let policy =
        policy_override.unwrap_or_else(|| policy_for_run(&origin, &control_plane_extra_tools));

    let publish = |kind: RunEventKind| -> Result<(), AppError> {
        events
            .publish(run_id, kind)
            .map(|_| ())
            .map_err(AppError::Store)
    };

    publish(RunEventKind::RunStarted)?;

    // Attach Soul + workspace Context files once per Run (system Transcript).
    // Soft-missing by default; distinct from Memory/Skill.
    let loaded = load_run_context(&run_context);
    let protected_paths = loaded.protected_paths.clone();
    for msg in loaded.messages {
        store
            .append_transcript(session_id, msg)
            .await
            .map_err(AppError::Store)?;
    }

    // User goal is part of durable Transcript once the Run completes successfully;
    // append up front so tool steps and subsequent model calls see it.
    store
        .append_transcript(
            session_id,
            TranscriptMessage::user(goal.clone()).with_run_id(run_id),
        )
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
        // Catalog = registered ∩ Policy (model never sees tools it cannot invoke).
        let tools_for_model = catalog_for_policy(&tools.catalog(), |name| policy.allows_tool(name));
        let model_future = model.complete(ModelRequest {
            goal: goal.clone(),
            transcript: transcript.messages,
            provider: provider.clone(),
            model: model_id.clone(),
            tools: tools_for_model,
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
                    TranscriptMessage::assistant(response.content.clone()).with_run_id(run_id),
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

            // Fail closed: origin Policy deny before adapter execution.
            let mut call = call;
            // Reduced origin: never local exec (Docker default / fail closed).
            // Enforced here so adapter wiring cannot bypass Run origin.
            if matches!(call.name.as_str(), "run_terminal" | "shell_exec")
                && origin.is_reduced_trust()
            {
                let backend = call
                    .arguments
                    .get("backend")
                    .and_then(|v| v.as_str())
                    .unwrap_or("docker");
                if backend == "local" {
                    let summary = "local exec denied for reduced Run origin (use docker backend)";
                    publish(RunEventKind::ToolFinished {
                        name: format!("{}: error={summary}", call.name),
                    })?;
                    store
                        .append_transcript(
                            session_id,
                            TranscriptMessage::tool_compact(
                                call.name.clone(),
                                "error",
                                summary,
                                vec![],
                            )
                            .with_run_id(run_id),
                        )
                        .await
                        .map_err(AppError::Store)?;
                    continue;
                }
                // Force docker when backend omitted on reduced origin.
                if call.arguments.get("backend").is_none() {
                    if let Some(obj) = call.arguments.as_object_mut() {
                        obj.insert("backend".into(), serde_json::json!("docker"));
                    }
                }
            }

            let needs_approval = is_high_blast_soul_context_edit(
                &call,
                &protected_paths,
                &run_context.workspace_roots,
            ) || is_high_blast_local_terminal(&call, &origin)
                || (call.name == "skill_manage" && !origin.is_reduced_trust())
                || high_blast_tools.contains(&call.name);

            let tool_outcome = if !policy.allows_tool(&call.name) {
                Err(ToolError::Denied(format!(
                    "tool '{}' denied by Policy for origin {}",
                    call.name,
                    origin.as_str()
                )))
            } else if needs_approval {
                match request_and_wait_approval(
                    store.as_ref(),
                    events.as_ref(),
                    approvals.as_ref(),
                    run_id,
                    principal_id.clone(),
                    &call,
                    &cancel,
                )
                .await
                {
                    Ok(true) => tools.invoke(call.clone()).await,
                    Ok(false) => Err(ToolError::Denied(
                        "high-blast: Approval denied (fail closed)".into(),
                    )),
                    Err(e) => Err(e),
                }
            } else {
                tools.invoke(call.clone()).await
            };

            match tool_outcome {
                Ok(result) => {
                    publish(RunEventKind::ToolFinished {
                        name: format!("{}: {}", call.name, result.summary),
                    })?;
                    store
                        .append_transcript(
                            session_id,
                            TranscriptMessage::tool_compact(
                                call.name.clone(),
                                "ok",
                                result.summary.clone(),
                                vec![],
                            )
                            .with_run_id(run_id),
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
                            TranscriptMessage::tool_compact(
                                call.name.clone(),
                                "error",
                                summary,
                                vec![],
                            )
                            .with_run_id(run_id),
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

/// Local terminal exec is high-blast for control_plane origin (Approval required).
fn is_high_blast_local_terminal(call: &crate::tools::ToolCall, origin: &RunOrigin) -> bool {
    if !matches!(call.name.as_str(), "run_terminal" | "shell_exec") {
        return false;
    }
    if origin.is_reduced_trust() {
        return false; // reduced uses docker path; local is denied in adapter
    }
    let backend = call
        .arguments
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("local");
    backend == "local"
}

/// True when a write-class tool targets a loaded Soul or Context file path.
fn is_high_blast_soul_context_edit(
    call: &crate::tools::ToolCall,
    protected: &[PathBuf],
    workspace_roots: &[PathBuf],
) -> bool {
    if protected.is_empty() {
        return false;
    }
    if !matches!(call.name.as_str(), "write_file" | "apply_patch") {
        return false;
    }
    let Some(path) = call.arguments.get("path").and_then(|v| v.as_str()) else {
        return false;
    };
    path_targets_protected(path, workspace_roots, protected)
}

/// Create a pending Approval, emit SSE, wait for Principal decision (or cancel/timeout).
///
/// Returns `Ok(true)` if approved, `Ok(false)` if denied, `Err` on cancel/timeout/store failure.
///
/// Waiter is registered **before** the durable row is inserted so a concurrent
/// decide after list cannot lose the oneshot (register-before-visible).
///
/// On [`APPROVAL_TIMEOUT`] expiry: CAS-deny the row, resolve the broker (no waiter leak),
/// fail the tool closed (spec story 33).
async fn request_and_wait_approval<S: SessionStore>(
    store: &S,
    events: &RunEventHub,
    broker: &ApprovalBroker,
    run_id: RunId,
    principal_id: keryx_domain::PrincipalId,
    call: &crate::tools::ToolCall,
    cancel: &CancellationToken,
) -> Result<bool, ToolError> {
    let summary = summarize_tool_args(&call.arguments);
    let approval = Approval::pending(run_id, principal_id, call.name.clone(), summary.clone());
    let approval_id = approval.id;

    // Register waiter first (before durable row is listable).
    let rx = broker.register(approval_id);

    store
        .create_approval(approval)
        .await
        .map_err(|e| ToolError::Failed(format!("approval store: {e}")))?;

    // If a decide raced in (should be rare with register-first), honor store status.
    if let Ok(Some(current)) = store.get_approval(approval_id).await {
        match current.status {
            ApprovalStatus::Approved => return Ok(true),
            ApprovalStatus::Denied => return Ok(false),
            ApprovalStatus::Pending => {}
        }
    }

    events
        .publish(
            run_id,
            RunEventKind::ApprovalWaiting {
                approval_id: approval_id.to_string(),
                action: call.name.clone(),
                summary,
            },
        )
        .map_err(ToolError::Failed)?;

    async fn fail_closed_pending<S: SessionStore>(
        store: &S,
        broker: &ApprovalBroker,
        approval_id: ApprovalId,
        system_actor: &str,
    ) {
        if let Ok(Some(mut a)) = store.get_approval(approval_id).await {
            if a.status == ApprovalStatus::Pending {
                a.deny(keryx_domain::PrincipalId::new(system_actor));
                let _ = store.update_approval_if_pending(a).await;
                broker.resolve(approval_id, ApprovalStatus::Denied);
            }
        }
    }

    tokio::select! {
        () = cancel.cancelled() => {
            fail_closed_pending(store, broker, approval_id, "system:cancel").await;
            Err(ToolError::Failed("approval wait cancelled".into()))
        }
        () = tokio::time::sleep(APPROVAL_TIMEOUT) => {
            fail_closed_pending(store, broker, approval_id, "system:timeout").await;
            Err(ToolError::Failed(format!(
                "approval timed out after {}s (fail closed)",
                APPROVAL_TIMEOUT.as_secs()
            )))
        }
        decision = rx => {
            match decision {
                Ok(ApprovalStatus::Approved) => Ok(true),
                Ok(ApprovalStatus::Denied) | Ok(ApprovalStatus::Pending) => Ok(false),
                Err(_) => {
                    // Channel closed: re-read store for late decision.
                    match store.get_approval(approval_id).await {
                        Ok(Some(a)) if a.status == ApprovalStatus::Approved => Ok(true),
                        Ok(Some(_)) => Ok(false),
                        _ => Err(ToolError::Failed(
                            "approval waiter dropped without decision".into(),
                        )),
                    }
                }
            }
        }
    }
}

fn truncate_preview(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}
