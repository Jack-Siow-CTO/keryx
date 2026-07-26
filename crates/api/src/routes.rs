use crate::auth::AuthPrincipal;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use keryx_domain::{Run, RunId, RunStatus, Session, SessionId};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Build the control-plane router (health is unauthenticated; all `/v1/*` require bearer auth).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/sessions", post(create_session))
        .route("/v1/sessions/{session_id}/runs", post(start_run))
        .route("/v1/runs/{run_id}", get(get_run))
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
}

#[derive(Serialize)]
struct RunResponse {
    id: String,
    session_id: String,
    principal_id: String,
    goal: String,
    status: RunStatus,
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
        .start_run(principal, session_id, body.goal)
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
