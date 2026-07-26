//! Seam 1 — SQLite durability, interrupted Active Runs, Transcript continuity.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ControlPlaneService, SessionStore};
use keryx_domain::{Principal, RunStatus};
use keryx_model::FakeModelProvider;
use keryx_storage::SqliteSessionStore;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

async fn wait_completed(app: &axum::Router, run_id: &str) -> Value {
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
        if body["status"] != "active" {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run did not complete");
}

#[tokio::test]
async fn session_transcript_and_run_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::with_fixed_content("answer-one"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(control, tokens));

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
    let session_id = body_json(create).await["id"].as_str().unwrap().to_string();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "first goal" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_completed(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(record["result"], "answer-one");

    // Drop process-local handles; reopen same data dir.
    drop(app);
    drop(store);

    let store2 = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let session = store2
        .get_session(session_id.parse().unwrap())
        .await
        .unwrap()
        .expect("session survives reopen");
    assert_eq!(session.principal_id.to_string(), PRINCIPAL);

    let run = store2
        .get_run(run_id.parse().unwrap())
        .await
        .unwrap()
        .expect("run survives reopen");
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.result.as_deref(), Some("answer-one"));

    let transcript = store2
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(transcript.messages.len(), 2);
    assert_eq!(transcript.messages[0].content, "first goal");
    assert_eq!(transcript.messages[1].content, "answer-one");
}

#[tokio::test]
async fn active_run_becomes_interrupted_on_reopen_no_mid_loop_resume() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());

    // Simulate a crash mid-Run by writing an Active Run record directly, then reopening.
    let session = keryx_domain::Session::new(keryx_domain::PrincipalId::new(PRINCIPAL));
    store.create_session(session.clone()).await.unwrap();
    let run = keryx_domain::Run::start(session.id, session.principal_id.clone(), "in flight");
    assert_eq!(run.status, RunStatus::Active);
    store.create_run(run.clone()).await.unwrap();
    drop(store);

    let store2 = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let recovered = store2.get_run(run.id).await.unwrap().unwrap();
    assert_eq!(recovered.status, RunStatus::Interrupted);
    assert_eq!(recovered.result.as_deref(), Some("interrupted"));

    // No mid-loop resume: client continues via a new Run.
    let model = Arc::new(FakeModelProvider::greeting());
    let control = ControlPlane::new(Arc::clone(&store2), model);
    let next = control
        .start_run(
            Principal::new(PRINCIPAL),
            session.id,
            "continue after interrupt".into(),
        )
        .await
        .unwrap();
    assert_eq!(next.status, RunStatus::Active);
    // Wait for completion
    for _ in 0..50 {
        let r = control.get_run(next.id).await.unwrap();
        if r.status.is_terminal() {
            assert_eq!(r.status, RunStatus::Completed);
            // Transcript from interrupted run was never appended (crash before complete).
            // New run still works; transcript_msgs reflects prior durable messages only.
            assert!(r.result.unwrap().contains("transcript_msgs="));
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("new run after interrupt did not complete");
}

#[tokio::test]
async fn new_run_sees_prior_transcript_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::greeting());
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(control, tokens));

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
    let session_id = body_json(create).await["id"].as_str().unwrap().to_string();

    let start1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "remember apples" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run1 = body_json(start1).await["id"].as_str().unwrap().to_string();
    let rec1 = wait_completed(&app, &run1).await;
    assert_eq!(rec1["status"], "completed");
    // First run sees empty transcript.
    assert!(
        rec1["result"]
            .as_str()
            .unwrap()
            .contains("transcript_msgs=0"),
        "first run result: {}",
        rec1["result"]
    );

    drop(app);
    drop(store);

    let store2 = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model2 = Arc::new(FakeModelProvider::greeting());
    let control2 = Arc::new(ControlPlane::new(store2, model2));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app2 = router(AppState::new(control2, tokens));

    let start2 = app2
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "what did I say?" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run2 = body_json(start2).await["id"].as_str().unwrap().to_string();
    let rec2 = wait_completed(&app2, &run2).await;
    assert_eq!(rec2["status"], "completed");
    // Second run sees 2 messages from the first completed Run.
    assert!(
        rec2["result"]
            .as_str()
            .unwrap()
            .contains("transcript_msgs=2"),
        "second run result: {}",
        rec2["result"]
    );
}
