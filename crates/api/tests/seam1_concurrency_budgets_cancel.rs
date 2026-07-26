//! Seam 1 — Active Run exclusivity, global cap, budgets, cancel.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, RunBudgets, RunLimits};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

fn router_with(model: FakeModelProvider, limits: RunLimits) -> axum::Router {
    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_limits(store, Arc::new(model), limits));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    router(AppState::new(control, tokens))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
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

async fn start_run(app: &axum::Router, session_id: &str, goal: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": goal }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn wait_status(app: &axum::Router, run_id: &str, want: &str) -> Value {
    for _ in 0..200 {
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
        if body["status"] == want {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run {run_id} did not reach status {want}");
}

#[tokio::test]
async fn second_run_on_same_session_while_active_is_rejected() {
    let app = router_with(
        FakeModelProvider::with_delay(Duration::from_millis(200), "slow"),
        RunLimits::default().with_global_cap(8),
    );
    let session_id = create_session(&app).await;

    let first = start_run(&app, &session_id, "first").await;
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(body_json(first).await["status"], "active");

    let second = start_run(&app, &session_id, "second").await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let err = body_json(second).await;
    let msg = err["error"].as_str().unwrap_or("");
    assert!(
        msg.contains("already has active run"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn global_cap_rejects_cross_session_overload() {
    let app = router_with(
        FakeModelProvider::with_delay(Duration::from_millis(200), "slow"),
        RunLimits::default().with_global_cap(1),
    );
    let session_a = create_session(&app).await;
    let session_b = create_session(&app).await;

    let first = start_run(&app, &session_a, "a").await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = start_run(&app, &session_b, "b").await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let err = body_json(second).await;
    let msg = err["error"].as_str().unwrap_or("");
    assert!(
        msg.contains("global active run cap exceeded"),
        "unexpected error: {msg}"
    );
}

#[tokio::test]
async fn time_budget_terminates_run() {
    let app = router_with(
        FakeModelProvider::with_delay(Duration::from_millis(200), "too slow"),
        RunLimits::default().with_budgets(RunBudgets {
            max_duration: Some(Duration::from_millis(30)),
            max_tokens: None,
            max_tool_calls: None,
        }),
    );
    let session_id = create_session(&app).await;
    let start = start_run(&app, &session_id, "budget time").await;
    assert_eq!(start.status(), StatusCode::CREATED);
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    let record = wait_status(&app, &run_id, "failed").await;
    let result = record["result"].as_str().unwrap_or("");
    assert!(
        result.contains("budget exceeded: time"),
        "unexpected result: {result}"
    );
}

#[tokio::test]
async fn token_budget_terminates_run() {
    let app = router_with(
        FakeModelProvider::with_fixed_content("abcdefghij"), // 10 chars
        RunLimits::default().with_budgets(RunBudgets {
            max_duration: None,
            max_tokens: Some(5),
            max_tool_calls: None,
        }),
    );
    let session_id = create_session(&app).await;
    let start = start_run(&app, &session_id, "budget tokens").await;
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_status(&app, &run_id, "failed").await;
    let result = record["result"].as_str().unwrap_or("");
    assert!(
        result.contains("budget exceeded: tokens"),
        "unexpected result: {result}"
    );
}

#[tokio::test]
async fn tool_call_budget_terminates_run() {
    let app = router_with(
        FakeModelProvider::with_tool_calls("x", vec!["read_file", "write_file"]),
        RunLimits::default().with_budgets(RunBudgets {
            max_duration: None,
            max_tokens: None,
            max_tool_calls: Some(1),
        }),
    );
    let session_id = create_session(&app).await;
    let start = start_run(&app, &session_id, "budget tools").await;
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_status(&app, &run_id, "failed").await;
    let result = record["result"].as_str().unwrap_or("");
    assert!(
        result.contains("budget exceeded: tool_calls"),
        "unexpected result: {result}"
    );
}

#[tokio::test]
async fn cancel_clears_active_run_and_marks_cancelled() {
    let app = router_with(
        FakeModelProvider::with_delay(Duration::from_secs(5), "never"),
        RunLimits::default(),
    );
    let session_id = create_session(&app).await;
    let start = start_run(&app, &session_id, "cancel me").await;
    assert_eq!(start.status(), StatusCode::CREATED);
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    let cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/runs/{run_id}/cancel"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel.status(), StatusCode::OK);
    let cancelled = body_json(cancel).await;
    assert_eq!(cancelled["status"], "cancelled");

    // Session exclusivity cleared: a new Run can start.
    let next = start_run(&app, &session_id, "after cancel").await;
    assert_eq!(next.status(), StatusCode::CREATED);
}
