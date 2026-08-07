use crate::auth::AuthPrincipal;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream, StreamExt};
use keryx_domain::{
    ActiveRootRunSummary, Approval, ApprovalId, ApprovalStatus, ArtifactId, ArtifactKind,
    ArtifactMeta, InboxItem, MemoryEntry, MemoryId, Run, RunEvent, RunId, RunStatus, Schedule,
    ScheduleId, ScheduleStatus, Session, SessionId, SessionSummary,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

/// Build the control-plane router (health is unauthenticated; all `/v1/*` require bearer auth).
pub fn router(state: AppState) -> Router {
    let allow_put = state.allow_artifact_put;
    let mut r = Router::new()
        .route("/health", get(health))
        .route("/v1/sessions", get(list_sessions).post(create_session))
        .route(
            "/v1/sessions/{session_id}",
            get(get_session).patch(patch_session),
        )
        .route("/v1/sessions/{session_id}/transcript", get(get_transcript))
        .route("/v1/sessions/{session_id}/runs", post(start_run))
        .route("/v1/runs/{run_id}", get(get_run))
        .route("/v1/runs/{run_id}/cancel", post(cancel_run))
        .route("/v1/runs/{run_id}/events", get(stream_run_events))
        .route("/v1/providers", get(list_providers))
        .route("/v1/approvals", get(list_approvals))
        .route("/v1/approvals/{approval_id}", get(get_approval))
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
        .route("/v1/inbox", get(list_inbox))
        .route("/v1/memory", get(list_or_search_memory).post(create_memory))
        .route(
            "/v1/memory/{memory_id}",
            get(get_memory).put(update_memory).delete(delete_memory),
        )
        .route("/v1/skills", get(list_skills))
        .route("/v1/skills/{name}", get(get_skill))
        .route("/v1/artifacts/{artifact_id}", get(get_artifact));
    if allow_put {
        r = r.route(
            "/v1/artifacts/{artifact_id}",
            post(put_artifact_test_double),
        );
    }
    r.with_state(state)
}

/// Seam 1 helper: production routes + Artifact POST enabled.
pub fn router_with_artifact_put(state: AppState) -> Router {
    router(state.with_artifact_put(true))
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
    title: String,
    title_is_custom: bool,
    created_at: i64,
    updated_at: i64,
    last_message_preview: Option<String>,
    active_root_run: Option<ActiveRootRunSummary>,
    pending_approval_count: u32,
}

impl From<SessionSummary> for SessionResponse {
    fn from(s: SessionSummary) -> Self {
        Self {
            id: s.id,
            principal_id: s.principal_id,
            title: s.title,
            title_is_custom: s.title_is_custom,
            created_at: s.created_at,
            updated_at: s.updated_at,
            last_message_preview: s.last_message_preview,
            active_root_run: s.active_root_run,
            pending_approval_count: s.pending_approval_count,
        }
    }
}

impl From<Session> for SessionResponse {
    /// Minimal response when projection is not yet loaded (create path reloads summary).
    fn from(session: Session) -> Self {
        Self {
            id: session.id.to_string(),
            principal_id: session.principal_id.to_string(),
            title: session.display_title(),
            title_is_custom: session
                .title
                .as_ref()
                .map(|t| !t.trim().is_empty())
                .unwrap_or(false),
            created_at: session.created_at,
            updated_at: session.updated_at,
            last_message_preview: None,
            active_root_run: None,
            pending_approval_count: 0,
        }
    }
}

#[derive(Serialize)]
struct SessionListResponse {
    sessions: Vec<SessionResponse>,
}

async fn list_sessions(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
) -> Result<Json<SessionListResponse>, ApiError> {
    let sessions = state.control.list_sessions().await?;
    Ok(Json(SessionListResponse {
        sessions: sessions.into_iter().map(SessionResponse::from).collect(),
    }))
}

async fn get_session(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    let session_id = SessionId::from_str(&session_id)
        .map_err(|_| ApiError::bad_request("invalid session id"))?;
    let summary = state.control.get_session(session_id).await?;
    Ok(Json(SessionResponse::from(summary)))
}

#[derive(Deserialize)]
struct PatchSessionRequest {
    /// Operator title override. Empty string clears to default (first user goal).
    title: Option<String>,
}

async fn patch_session(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(session_id): Path<String>,
    Json(body): Json<PatchSessionRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let session_id = SessionId::from_str(&session_id)
        .map_err(|_| ApiError::bad_request("invalid session id"))?;
    let title = body.title.map(|t| t.trim().to_string());
    let summary = state.control.patch_session_title(session_id, title).await?;
    Ok(Json(SessionResponse::from(summary)))
}

async fn create_session(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
) -> Result<(StatusCode, Json<SessionResponse>), ApiError> {
    let session = state.control.create_session(principal).await?;
    // Return full projection for Console list consistency.
    let summary = state.control.get_session(session.id).await?;
    Ok((StatusCode::CREATED, Json(SessionResponse::from(summary))))
}

#[derive(Deserialize)]
struct TranscriptQuery {
    /// Max messages in page (default 50, max 200).
    limit: Option<usize>,
    /// Exclusive older-bound message id (scroll up for history).
    before: Option<String>,
}

#[derive(Serialize)]
struct TranscriptPageResponse {
    session_id: String,
    messages: Vec<TranscriptMessageResponse>,
    next_before: Option<String>,
}

#[derive(Serialize)]
struct TranscriptMessageResponse {
    id: String,
    run_id: Option<String>,
    created_at: i64,
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<ToolCompactResponse>,
}

#[derive(Serialize)]
struct ToolCompactResponse {
    name: String,
    status: String,
    summary: String,
    artifact_refs: Vec<String>,
}

async fn get_transcript(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(session_id): Path<String>,
    Query(q): Query<TranscriptQuery>,
) -> Result<Json<TranscriptPageResponse>, ApiError> {
    let session_id = SessionId::from_str(&session_id)
        .map_err(|_| ApiError::bad_request("invalid session id"))?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let page = state
        .control
        .get_transcript_page(session_id, limit, q.before)
        .await?;
    Ok(Json(TranscriptPageResponse {
        session_id: page.session_id,
        messages: page
            .messages
            .into_iter()
            .map(|m| TranscriptMessageResponse {
                id: m.id,
                run_id: m.run_id.map(|id| id.to_string()),
                created_at: m.created_at,
                role: match m.role {
                    keryx_domain::MessageRole::System => "system".into(),
                    keryx_domain::MessageRole::User => "user".into(),
                    keryx_domain::MessageRole::Assistant => "assistant".into(),
                    keryx_domain::MessageRole::Tool => "tool".into(),
                },
                content: m.content,
                tool: m.tool.map(|t| ToolCompactResponse {
                    name: t.name,
                    status: t.status,
                    summary: t.summary,
                    artifact_refs: t.artifact_refs,
                }),
            })
            .collect(),
        next_before: page.next_before,
    }))
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

async fn get_approval(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApiError> {
    let id = ApprovalId::from_str(&approval_id)
        .map_err(|_| ApiError::bad_request("invalid approval id"))?;
    let approval = state.control.get_approval(id).await?;
    Ok(Json(approval.into()))
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

// --- Inbox (ADR 0028) ---

#[derive(Deserialize)]
struct InboxQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct InboxResponse {
    items: Vec<InboxItem>,
}

async fn list_inbox(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Query(q): Query<InboxQuery>,
) -> Result<Json<InboxResponse>, ApiError> {
    let items = state.control.list_inbox(q.limit.unwrap_or(50)).await?;
    Ok(Json(InboxResponse { items }))
}

// --- Memory (ADR 0029) ---

#[derive(Deserialize)]
struct MemoryQuery {
    q: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct MemoryListResponse {
    entries: Vec<MemoryEntryResponse>,
}

#[derive(Serialize)]
struct MemoryEntryResponse {
    id: String,
    content: String,
    label: Option<String>,
    source_run_id: Option<String>,
    source_principal_id: Option<String>,
}

impl From<MemoryEntry> for MemoryEntryResponse {
    fn from(e: MemoryEntry) -> Self {
        Self {
            id: e.id.to_string(),
            content: e.content,
            label: e.label,
            source_run_id: e.source_run_id.map(|id| id.to_string()),
            source_principal_id: e.source_principal_id.map(|id| id.to_string()),
        }
    }
}

#[derive(Deserialize)]
struct MemoryWriteRequest {
    content: String,
    label: Option<String>,
}

async fn list_or_search_memory(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Query(q): Query<MemoryQuery>,
) -> Result<Json<MemoryListResponse>, ApiError> {
    let entries = if let Some(query) = q.q.filter(|s| !s.trim().is_empty()) {
        state
            .control
            .search_memory(&query, q.limit.unwrap_or(50))
            .await?
    } else {
        state.control.list_memory().await?
    };
    Ok(Json(MemoryListResponse {
        entries: entries.into_iter().map(Into::into).collect(),
    }))
}

async fn create_memory(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Json(body): Json<MemoryWriteRequest>,
) -> Result<(StatusCode, Json<MemoryEntryResponse>), ApiError> {
    let entry = state
        .control
        .create_memory(principal, body.content, body.label)
        .await?;
    Ok((StatusCode::CREATED, Json(entry.into())))
}

async fn get_memory(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(memory_id): Path<String>,
) -> Result<Json<MemoryEntryResponse>, ApiError> {
    let id =
        MemoryId::from_str(&memory_id).map_err(|_| ApiError::bad_request("invalid memory id"))?;
    Ok(Json(state.control.get_memory(id).await?.into()))
}

async fn update_memory(
    State(state): State<AppState>,
    AuthPrincipal(principal): AuthPrincipal,
    Path(memory_id): Path<String>,
    Json(body): Json<MemoryWriteRequest>,
) -> Result<Json<MemoryEntryResponse>, ApiError> {
    let id =
        MemoryId::from_str(&memory_id).map_err(|_| ApiError::bad_request("invalid memory id"))?;
    Ok(Json(
        state
            .control
            .update_memory(principal, id, body.content, body.label)
            .await?
            .into(),
    ))
}

async fn delete_memory(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(memory_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id =
        MemoryId::from_str(&memory_id).map_err(|_| ApiError::bad_request("invalid memory id"))?;
    state.control.delete_memory(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- Skills read-mostly (ADR 0030) ---

#[derive(Serialize)]
struct SkillsListResponse {
    skills: Vec<SkillSummaryResponse>,
}

#[derive(Serialize)]
struct SkillSummaryResponse {
    name: String,
}

#[derive(Serialize)]
struct SkillDetailResponse {
    name: String,
    content: String,
}

async fn list_skills(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
) -> Result<Json<SkillsListResponse>, ApiError> {
    let Some(root) = state.skills_root.as_ref() else {
        return Ok(Json(SkillsListResponse { skills: vec![] }));
    };
    let mut skills = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for ent in rd.flatten() {
            if ent.path().is_dir() {
                let name = ent.file_name().to_string_lossy().to_string();
                if ent.path().join("SKILL.md").is_file() {
                    skills.push(SkillSummaryResponse { name });
                }
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(SkillsListResponse { skills }))
}

async fn get_skill(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(name): Path<String>,
) -> Result<Json<SkillDetailResponse>, ApiError> {
    if name.contains('/') || name.contains("..") || name.is_empty() {
        return Err(ApiError::bad_request("invalid skill name"));
    }
    let Some(root) = state.skills_root.as_ref() else {
        return Err(ApiError::not_found("skills root not configured"));
    };
    let path = root.join(&name).join("SKILL.md");
    let content = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::not_found(format!("skill not found: {name}")))?;
    Ok(Json(SkillDetailResponse { name, content }))
}

// --- Artifacts (ADR 0026) ---

#[derive(Serialize)]
struct ArtifactMetaResponse {
    id: String,
    kind: String,
    media_type: String,
    byte_len: u64,
    created_at: i64,
    run_id: Option<String>,
    session_id: Option<String>,
    summary: String,
}

impl From<ArtifactMeta> for ArtifactMetaResponse {
    fn from(m: ArtifactMeta) -> Self {
        Self {
            id: m.id.to_string(),
            kind: m.kind.as_str().to_string(),
            media_type: m.media_type,
            byte_len: m.byte_len,
            created_at: m.created_at,
            run_id: m.run_id,
            session_id: m.session_id,
            summary: m.summary,
        }
    }
}

/// GET metadata as JSON when Accept is application/json; otherwise raw bytes.
async fn get_artifact(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(artifact_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    let id = ArtifactId::from_str(&artifact_id)
        .map_err(|_| ApiError::bad_request("invalid artifact id"))?;
    let meta = state.control.get_artifact_meta(id).await?;
    let want_json = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("application/json"));
    if want_json {
        return Ok(Json(ArtifactMetaResponse::from(meta)).into_response());
    }
    let bytes = if let Some(text) = meta.content_text.as_ref() {
        text.clone().into_bytes()
    } else if let Some(dir) = state.artifacts_dir.as_ref() {
        let path = dir.join(format!("{artifact_id}.bin"));
        std::fs::read(&path).unwrap_or_else(|_| meta.summary.clone().into_bytes())
    } else {
        meta.summary.clone().into_bytes()
    };
    Ok((
        [
            (header::CONTENT_TYPE, meta.media_type),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
        ],
        bytes,
    )
        .into_response())
}

/// Test/seam helper: create Artifact metadata + blob under data dir.
/// Production tools write via Worker adapters; Console only GETs.
#[derive(Deserialize)]
struct PutArtifactRequest {
    kind: String,
    media_type: Option<String>,
    summary: Option<String>,
    content_base64: Option<String>,
    content_text: Option<String>,
    run_id: Option<String>,
    session_id: Option<String>,
}

async fn put_artifact_test_double(
    State(state): State<AppState>,
    AuthPrincipal(_principal): AuthPrincipal,
    Path(artifact_id): Path<String>,
    Json(body): Json<PutArtifactRequest>,
) -> Result<(StatusCode, Json<ArtifactMetaResponse>), ApiError> {
    let id = if artifact_id == "new" {
        ArtifactId::new()
    } else {
        ArtifactId::from_str(&artifact_id)
            .map_err(|_| ApiError::bad_request("invalid artifact id"))?
    };
    let kind = ArtifactKind::parse(&body.kind)
        .ok_or_else(|| ApiError::bad_request("invalid artifact kind"))?;
    let bytes = body
        .content_text
        .unwrap_or_else(|| body.content_base64.unwrap_or_default())
        .into_bytes();
    let media_type = body.media_type.unwrap_or_else(|| match kind {
        ArtifactKind::Image => "image/png".into(),
        ArtifactKind::Json => "application/json".into(),
        ArtifactKind::Diff => "text/x-diff".into(),
        ArtifactKind::Terminal | ArtifactKind::Text => "text/plain".into(),
    });
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&bytes).to_string();
    let meta = ArtifactMeta {
        id,
        kind,
        media_type,
        byte_len: bytes.len() as u64,
        created_at: now,
        run_id: body.run_id,
        session_id: body.session_id,
        summary: body.summary.unwrap_or_else(|| kind.as_str().into()),
        content_text: Some(text),
    };
    if let Some(dir) = state.artifacts_dir.as_ref() {
        std::fs::create_dir_all(dir).map_err(|e| ApiError::internal(e.to_string()))?;
        std::fs::write(dir.join(format!("{id}.bin")), &bytes)
            .map_err(|e| ApiError::internal(e.to_string()))?;
    }
    state.control.put_artifact_meta(meta.clone()).await?;
    Ok((StatusCode::CREATED, Json(meta.into())))
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
