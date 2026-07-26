//! Seam 1 — SSE Run events: taxonomy order, model deltas, reconnect via GET Run.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::ControlPlane;
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

fn harness_with_deltas() -> axum::Router {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_deltas(vec!["hel", "lo", "!"]));
    let control = Arc::new(ControlPlane::new(store, model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    router(AppState::new(control, tokens))
}

async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).expect("json body")
}

/// Parse `event:` names from an SSE body (ignores comments/data-only lines).
fn sse_event_names(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| line.strip_prefix("event:"))
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect()
}

async fn create_session(app: &axum::Router) -> String {
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    body_json(create).await["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn sse_emits_run_model_terminal_order_with_deltas() {
    let app = harness_with_deltas();
    let session_id = create_session(&app).await;

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "stream please" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run = body_json(start).await;
    assert_eq!(run["status"], "active");
    let run_id = run["id"].as_str().unwrap().to_string();

    // Collect the SSE stream until the body completes (server ends after terminal event).
    let events_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{run_id}/events"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events_response.status(), StatusCode::OK);
    let content_type = events_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "content-type was {content_type}"
    );

    let sse_body = String::from_utf8(body_bytes(events_response).await).unwrap();
    let names = sse_event_names(&sse_body);

    assert!(
        names.iter().any(|n| n == "run.started"),
        "missing run.started in {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "model.started"),
        "missing model.started in {names:?}"
    );
    let delta_count = names.iter().filter(|n| *n == "model.delta").count();
    assert_eq!(
        delta_count, 3,
        "expected 3 model.delta events, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "model.finished"),
        "missing model.finished in {names:?}"
    );
    assert_eq!(names.last().map(String::as_str), Some("run.completed"));

    // Event order: started before model, model.finished before terminal.
    let run_started = names.iter().position(|n| n == "run.started").unwrap();
    let model_started = names.iter().position(|n| n == "model.started").unwrap();
    let model_finished = names.iter().position(|n| n == "model.finished").unwrap();
    let completed = names.iter().position(|n| n == "run.completed").unwrap();
    assert!(run_started < model_started);
    assert!(model_started < model_finished);
    assert!(model_finished < completed);

    // After stream ends, GET Run still returns final status and result.
    let mut record = None;
    for _ in 0..50 {
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/runs/{run_id}"))
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(get).await;
        if body["status"] == "completed" {
            record = Some(body);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let record = record.expect("run completed");
    assert_eq!(record["result"], "hello!");
}

#[tokio::test]
async fn start_remains_http_request_response_not_sse() {
    let app = harness_with_deltas();
    let session_id = create_session(&app).await;
    let start = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "x" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let content_type = start
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("application/json"),
        "start_run should be JSON, got {content_type}"
    );
}
