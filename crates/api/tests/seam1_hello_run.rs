//! Seam 1 — control plane: authenticated Hello Run with fake Model provider.
//!
//! Covers: health, bearer auth fail-closed, Session + Run happy path, Principal attribution.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, SessionStore};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
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

#[tokio::test]
async fn health_endpoint_ok_without_auth() {
    let (app, _) = harness();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn missing_token_fails_closed_with_no_session_side_effects() {
    let (app, store) = harness();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(store.count_sessions().await.unwrap(), 0);
    assert_eq!(store.count_runs().await.unwrap(), 0);
}

#[tokio::test]
async fn invalid_token_fails_closed_with_no_session_side_effects() {
    let (app, store) = harness();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("authorization", "Bearer wrong-token")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(store.count_sessions().await.unwrap(), 0);
    assert_eq!(store.count_runs().await.unwrap(), 0);
}

#[tokio::test]
async fn authenticated_hello_run_completes_with_fake_model_and_principal() {
    let (app, _) = harness();

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sessions")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let session = body_json(create).await;
    assert_eq!(session["principal_id"], PRINCIPAL);
    let session_id = session["id"].as_str().expect("session id");

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "goal": "greet the operator" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run = body_json(start).await;
    assert_eq!(run["session_id"], session_id);
    assert_eq!(run["principal_id"], PRINCIPAL);
    assert_eq!(run["goal"], "greet the operator");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["result"], "hello from fake model");
    let run_id = run["id"].as_str().expect("run id");

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{run_id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let record = body_json(get).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(record["result"], "hello from fake model");
    assert_eq!(record["principal_id"], PRINCIPAL);
}
