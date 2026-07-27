use crate::auth::AuthPrincipal;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream, StreamExt};
use keryx_domain::{
    Approval, ApprovalId, ApprovalStatus, Run, RunEvent, RunId, RunStatus, Schedule, ScheduleId,
    ScheduleStatus, Session, SessionId,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

/// Build the control-plane router (health is unauthenticated; all `/v1/*` require bearer auth).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/sessions", post(create_session))
        .route("/v1/sessions/{session_id}/runs", post(start_run))
        .route("/v1/runs/{run_id}", get(get_run))
        .route("/v1/runs/{run_id}/cancel", post(cancel_run))
        .route("/v1/runs/{run_id}/events", get(stream_run_events))
        .route("/v1/providers", get(list_providers))
        .route("/v1/approvals", get(list_approvals))
        .route(
            "/v1/approvals/{approval_id}/approve",
            post(approve_approval),
        )
        .route("/v1/approvals/{approval_id}/deny", post(deny_approval))
        .route("/v1/schedules", get(list_schedules).post(create_schedule))
        .route("/v1/schedules/{schedule_id}/pause", post(pause_schedule))
        .route("/v1/schedules/{schedule_id}/resume", post(resume_schedule))
        .route("/v1/schedules/{schedule_id}/delete", post(delete_schedule))
        .route("/v1/schedules/tick", post(tick_schedules))
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct ProvidersResponse {
    default: Option<String>,
    providers: Vec<crate::state::ProviderInfo>,
}

async fn list_providers(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
) -> Json<ProvidersResponse> {
    Json(ProvidersResponse {
        default: state.providers.default.clone(),
        providers: state.providers.providers.clone(),
    })
}

#[derive(Serialize)]
struct SessionResponse {
    id: String,
    principal_id: String,
}

impl From<Session> for SessionResponse {
    fn from(session: Session) -> Self {
        Self {
            id: session.id.to_string(),
            principal_id: session.principal_id.to_string(),
        }
    }
}

async fn create_session(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Result<(StatusCode, Json<SessionResponse>), ApiError> {
    let session = state.control.create_session(principal).await?;
    Ok((StatusCode::CREATED, Json(session.into())))
}

#[derive(Deserialize)]
struct StartRunRequest {
    goal: String,
    /// Optional Model provider key (`openai`, `grok`, `openai_codex`, …).
    provider: Option<String>,
    /// Optional per-run model id override.
    model: Option<String>,
}

#[derive(Serialize)]
struct RunResponse {
    id: String,
    session_id: String,
    principal_id: String,
    goal: String,
    status: RunStatus,
    /// Run origin wire form (`control_plane`, `schedule`, `gateway:{platform}`).
    origin: String,
    /// Parent Run id when this is a Child Run.
    parent_run_id: Option<String>,
    result: Option<String>,
}

impl From<Run> for RunResponse {
    fn from(run: Run) -> Self {
        Self {
            id: run.id.to_string(),
            session_id: run.session_id.to_string(),
            principal_id: run.principal_id.to_string(),
            goal: run.goal,
            status: run.status,
            origin: run.origin.as_str(),
            parent_run_id: run.parent_run_id.map(|id| id.to_string()),
            result: run.result,
        }
    }
}

async fn start_run(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(session_id): Path<String>,
    Json(body): Json<StartRunRequest>,
) -> Result<(StatusCode, Json<RunResponse>), ApiError> {
    if body.goal.trim().is_empty() {
        return Err(ApiError::bad_request("goal must not be empty"));
    }
    let session_id = SessionId::from_str(&session_id)
        .map_err(|_| ApiError::bad_request("invalid session id"))?;
    let run = state
        .control
        .start_run(principal, session_id, body.goal, body.provider, body.model)
        .await?;
    Ok((StatusCode::CREATED, Json(run.into())))
}

async fn get_run(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(run_id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let run_id = RunId::from_str(&run_id).map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state.control.get_run(run_id).await?;
    Ok(Json(run.into()))
}

async fn cancel_run(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(run_id): Path<String>,
) -> Result<Json<RunResponse>, ApiError> {
    let run_id = RunId::from_str(&run_id).map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state.control.cancel_run(run_id).await?;
    Ok(Json(run.into()))
}

async fn stream_run_events(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(run_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let run_id = RunId::from_str(&run_id).map_err(|_| ApiError::bad_request("invalid run id"))?;
    let (replay, rx) = state.control.subscribe_run_events(run_id).await?;
    let stream = run_event_sse_stream(replay, rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[derive(Deserialize)]
struct ListApprovalsQuery {
    /// When true (default), only pending Approvals.
    #[serde(default = "default_true")]
    pending: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct ApprovalResponse {
    id: String,
    run_id: String,
    action: String,
    summary: String,
    status: ApprovalStatus,
    requested_by: String,
    decided_by: Option<String>,
}

impl From<Approval> for ApprovalResponse {
    fn from(a: Approval) -> Self {
        Self {
            id: a.id.to_string(),
            run_id: a.run_id.to_string(),
            action: a.action,
            summary: a.summary,
            status: a.status,
            requested_by: a.requested_by.to_string(),
            decided_by: a.decided_by.map(|p| p.to_string()),
        }
    }
}

#[derive(Serialize)]
struct ApprovalsListResponse {
    approvals: Vec<ApprovalResponse>,
}

async fn list_approvals(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Query(q): Query<ListApprovalsQuery>,
) -> Result<Json<ApprovalsListResponse>, ApiError> {
    let approvals = state.control.list_approvals(q.pending).await?;
    Ok(Json(ApprovalsListResponse {
        approvals: approvals.into_iter().map(Into::into).collect(),
    }))
}

async fn approve_approval(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApiError> {
    let id = ApprovalId::from_str(&approval_id)
        .map_err(|_| ApiError::bad_request("invalid approval id"))?;
    let approval = state.control.approve(principal, id).await?;
    Ok(Json(approval.into()))
}

async fn deny_approval(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApiError> {
    let id = ApprovalId::from_str(&approval_id)
        .map_err(|_| ApiError::bad_request("invalid approval id"))?;
    let approval = state.control.deny(principal, id).await?;
    Ok(Json(approval.into()))
}

#[derive(Deserialize)]
struct CreateScheduleRequest {
    goal: String,
    /// Interval between fires in seconds.
    interval_secs: u64,
    /// Optional next fire time (unix epoch secs). Default: now.
    next_fire_at: Option<i64>,
    /// Optional frozen tool allowlist snapshot.
    policy_tools: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ScheduleResponse {
    id: String,
    principal_id: String,
    session_id: Option<String>,
    goal: String,
    interval_secs: u64,
    status: ScheduleStatus,
    next_fire_at: i64,
    policy_tools: Vec<String>,
    last_fired_at: Option<i64>,
}

impl From<Schedule> for ScheduleResponse {
    fn from(s: Schedule) -> Self {
        Self {
            id: s.id.to_string(),
            principal_id: s.principal_id.to_string(),
            session_id: s.session_id.map(|id| id.to_string()),
            goal: s.goal,
            interval_secs: s.interval_secs,
            status: s.status,
            next_fire_at: s.next_fire_at,
            policy_tools: s.policy_tools,
            last_fired_at: s.last_fired_at,
        }
    }
}

#[derive(Serialize)]
struct SchedulesListResponse {
    schedules: Vec<ScheduleResponse>,
}

async fn create_schedule(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleResponse>), ApiError> {
    if body.goal.trim().is_empty() {
        return Err(ApiError::bad_request("goal must not be empty"));
    }
    if body.interval_secs == 0 {
        return Err(ApiError::bad_request("interval_secs must be >= 1"));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let schedule = state
        .control
        .create_schedule(
            principal,
            body.goal,
            body.interval_secs,
            body.next_fire_at.unwrap_or(now),
            body.policy_tools,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(schedule.into())))
}

async fn list_schedules(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
) -> Result<Json<SchedulesListResponse>, ApiError> {
    let schedules = state.control.list_schedules().await?;
    Ok(Json(SchedulesListResponse {
        schedules: schedules.into_iter().map(Into::into).collect(),
    }))
}

async fn pause_schedule(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(schedule_id): Path<String>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let id = ScheduleId::from_str(&schedule_id)
        .map_err(|_| ApiError::bad_request("invalid schedule id"))?;
    Ok(Json(state.control.pause_schedule(id).await?.into()))
}

async fn resume_schedule(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(schedule_id): Path<String>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let id = ScheduleId::from_str(&schedule_id)
        .map_err(|_| ApiError::bad_request("invalid schedule id"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(Json(state.control.resume_schedule(id, now).await?.into()))
}

async fn delete_schedule(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(schedule_id): Path<String>,
) -> Result<Json<ScheduleResponse>, ApiError> {
    let id = ScheduleId::from_str(&schedule_id)
        .map_err(|_| ApiError::bad_request("invalid schedule id"))?;
    Ok(Json(state.control.delete_schedule(id).await?.into()))
}

#[derive(Deserialize)]
struct TickSchedulesRequest {
    /// Deterministic clock (unix epoch seconds) for Seam 1 / operator tests.
    now: i64,
}

#[derive(Serialize)]
struct TickSchedulesResponse {
    started_runs: Vec<RunResponse>,
}

async fn tick_schedules(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Json(body): Json<TickSchedulesRequest>,
) -> Result<Json<TickSchedulesResponse>, ApiError> {
    let runs = state.control.tick_schedules(body.now).await?;
    Ok(Json(TickSchedulesResponse {
        started_runs: runs.into_iter().map(Into::into).collect(),
    }))
}

type LiveStream = Pin<Box<dyn Stream<Item = RunEvent> + Send>>;

/// Replay buffered events, then live broadcast; stop after a terminal Run event.
fn run_event_sse_stream(
    replay: Vec<RunEvent>,
    rx: tokio::sync::broadcast::Receiver<RunEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let live: LiveStream = Box::pin(BroadcastStream::new(rx).filter_map(|item| async move {
        // Drop lagged gaps; clients rely on seq + GET Run for correctness.
        item.ok()
    }));

    stream::unfold(
        StreamState {
            replay: replay.into_iter(),
            live,
            last_seq: 0,
            done: false,
        },
        |mut state| async move {
            if state.done {
                return None;
            }

            let next = if let Some(event) = state.replay.next() {
                Some(event)
            } else {
                state.live.next().await
            };

            let event = next?;

            if event.seq <= state.last_seq {
                return Some((None, state));
            }
            state.last_seq = event.seq;
            let terminal = event.is_terminal();
            let sse = to_sse_event(&event);
            if terminal {
                state.done = true;
            }
            Some((Some(Ok(sse)), state))
        },
    )
    .filter_map(|item| async move { item })
}

struct StreamState {
    replay: std::vec::IntoIter<RunEvent>,
    live: LiveStream,
    last_seq: u64,
    done: bool,
}

fn to_sse_event(event: &RunEvent) -> Event {
    let data = serde_json::json!({
        "run_id": event.run_id.to_string(),
        "seq": event.seq,
        "kind": event.kind,
    });
    Event::default()
        .event(event.kind.name())
        .id(event.seq.to_string())
        .data(data.to_string())
}
