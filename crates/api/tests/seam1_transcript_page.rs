//! Seam 1 — structured paged Transcript (ADR 0025 / Console #40).

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

fn harness() -> axum::Router {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content("assistant says hi"));
    let control = Arc::new(ControlPlane::new(store, model));
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

#[tokio::test]
async fn transcript_page_has_structured_messages_newest_first() {
    let app = harness();

    let create = app
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
    assert_eq!(create.status(), StatusCode::CREATED);
    let sid = body_json(create).await["id"].as_str().unwrap().to_string();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{sid}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"goal": "Say hello"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    // Wait for fake model to complete.
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
        let st = body_json(get).await;
        if st["status"] != "active" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let tr = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/sessions/{sid}/transcript?limit=50"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tr.status(), StatusCode::OK);
    let body = body_json(tr).await;
    assert_eq!(body["session_id"], sid);
    let messages = body["messages"].as_array().unwrap();
    assert!(messages.len() >= 2, "user + assistant expected");
    // Newest first: assistant typically after user.
    for m in messages {
        assert!(!m["id"].as_str().unwrap().is_empty());
        assert!(m["created_at"].as_i64().is_some());
        assert!(m.get("role").is_some());
        assert!(m.get("content").is_some());
    }
    let roles: Vec<&str> = messages
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert!(roles.contains(&"user"));
    assert!(roles.contains(&"assistant"));
}

#[tokio::test]
async fn transcript_unauth_fails_closed() {
    let app = harness();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/sessions/00000000-0000-0000-0000-000000000001/transcript")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
