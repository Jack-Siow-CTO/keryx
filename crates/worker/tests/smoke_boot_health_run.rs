//! L4-style smoke: compose Worker pieces in-process (boot, health, fake-model Run).
//!
//! Full binary process smoke is ops-facing; this keeps CI free of long-lived processes
//! while verifying the composition root wiring the binary uses.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, RunLimits};
use keryx_model::FakeModelProvider;
use keryx_storage::SqliteSessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "smoke-token";
const PRINCIPAL: &str = "smoke-operator";

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn smoke_boot_health_and_fake_model_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::with_fixed_content("smoke-ok"));
    let control = Arc::new(ControlPlane::with_limits(
        store,
        model,
        RunLimits::default().with_global_cap(2),
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(control, tokens));

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(body_json(health).await["status"], "ok");

    let session = app
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
    assert_eq!(session.status(), StatusCode::CREATED);
    let session_id = body_json(session).await["id"].as_str().unwrap().to_string();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "smoke" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    for _ in 0..100 {
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
            assert_eq!(body["result"], "smoke-ok");
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("smoke run did not complete");
}
