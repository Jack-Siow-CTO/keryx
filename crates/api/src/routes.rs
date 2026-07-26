use crate::auth::AuthPrincipal;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream, StreamExt};
use keryx_domain::{Run, RunEvent, RunId, RunStatus, Session, SessionId};
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
