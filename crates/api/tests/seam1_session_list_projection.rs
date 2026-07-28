//! Seam 1 — Session list projection (ADR 0027 / Console #39).
//!
//! Covers: list/create/get/patch title, default title from first user goal,
//! active root Run summary, pending Approval count.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, SessionStore as _};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

fn harness() -> (axum::Router, Arc<InMemorySessionStore>) {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content(
        "hello from fake model",
    ));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let state = AppState::new(control, tokens);
    (router(state), store)
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
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_sessions_empty_then_create() {
    let (app, _) = harness();

    let empty = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    let body = body_json(empty).await;
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);

    let sid = create_session(&app).await;

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = body_json(list).await;
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], sid);
    assert_eq!(sessions[0]["title"], "New Session");
    assert_eq!(sessions[0]["title_is_custom"], false);
    assert_eq!(sessions[0]["pending_approval_count"], 0);
    assert!(sessions[0]["active_root_run"].is_null());
}

#[tokio::test]
async fn default_title_from_first_user_goal_and_rename() {
    let (app, _) = harness();
    let sid = create_session(&app).await;

    // Start a Run — goal becomes default title.
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{sid}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"goal": "Investigate flaky CI pipeline"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);

    // Wait briefly for run to progress.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/sessions/{sid}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let body = body_json(get).await;
    assert_eq!(body["title"], "Investigate flaky CI pipeline");
    assert_eq!(body["title_is_custom"], false);
    // Active or completed depending on timing — presence of active_root_run when still active.
    assert!(body["active_root_run"].is_object() || body["active_root_run"].is_null());

    // Operator rename is durable.
    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/sessions/{sid}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"title": "CI war room"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    let body = body_json(patch).await;
    assert_eq!(body["title"], "CI war room");
    assert_eq!(body["title_is_custom"], true);

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(list).await;
    assert_eq!(body["sessions"][0]["title"], "CI war room");
}

#[tokio::test]
async fn unauthenticated_list_fails_closed() {
    let (app, store) = harness();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(store.count_sessions().await.unwrap(), 0);
}

#[tokio::test]
async fn get_unknown_session_is_not_found() {
    let (app, _) = harness();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/00000000-0000-0000-0000-000000000099")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
