//! Seam 1 — Approval queue: wait → approve / deny, auth fail-closed, SSE milestones.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ModelResponse, RunContextConfig, RunLimits, SessionStore, ToolCall};
use keryx_domain::{Approval, ApprovalStatus, PrincipalId};
use keryx_model::FakeModelProvider;
use keryx_storage::{InMemorySessionStore, SqliteSessionStore};
use keryx_tools::WorkspaceFsTools;
use serde_json::{json, Value};
use std::collections::HashSet;
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

async fn wait_pending_approval(app: &axum::Router) -> String {
    for _ in 0..200 {
        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals?pending=true")
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = body_json(list).await;
        if let Some(id) = body["approvals"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|a| a["id"].as_str())
        {
            return id.to_string();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no pending approval");
}

async fn wait_terminal(app: &axum::Router, run_id: &str) -> Value {
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
        if body["status"] != "active" {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run did not finish");
}

fn harness_with_soul(model: FakeModelProvider) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let soul = dir.path().join("SOUL.md");
    std::fs::write(&soul, "identity").unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("SOUL.md"), "identity").unwrap();

    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(WorkspaceFsTools::new(
        vec![ws.clone()],
        HashSet::from(["write_file".into()]),
    ));
    let control = Arc::new(ControlPlane::with_tools_and_context(
        store,
        Arc::new(model),
        RunLimits::default(),
        tools,
        RunContextConfig {
            soul_path: Some(soul),
            context_files: vec![],
            workspace_roots: vec![ws],
            missing: keryx_app::MissingContextPolicy::Soft,
        },
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    (router(AppState::new(control, tokens)), dir)
}

#[tokio::test]
async fn unauthenticated_approvals_fail_closed() {
    let (app, _dir) = harness_with_soul(FakeModelProvider::with_fixed_content("x"));
    let list = app
        .oneshot(
            Request::builder()
                .uri("/v1/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn high_blast_wait_then_approve_allows_write() {
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "write_file".into(),
                arguments: json!({ "path": "SOUL.md", "content": "approved-write" }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let (app, dir) = harness_with_soul(model);
    let ws = dir.path().join("ws");

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
                .body(Body::from(json!({ "goal": "edit soul" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    let approval_id = wait_pending_approval(&app).await;

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/approvals/{approval_id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let got = body_json(get).await;
    assert_eq!(got["id"], approval_id);
    assert_eq!(got["status"], "pending");
    assert_eq!(got["action"], "write_file");
    assert!(got["requested_by"].is_string());
    assert!(got["decided_by"].is_null());

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/approvals/00000000-0000-4000-8000-000000000000")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let approve = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/approvals/{approval_id}/approve"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let body = body_json(approve).await;
    assert_eq!(body["status"], "approved");
    assert_eq!(body["decided_by"], PRINCIPAL);

    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(
        std::fs::read_to_string(ws.join("SOUL.md")).unwrap(),
        "approved-write"
    );

    let events = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{run_id}/events"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        events
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("approval.waiting"), "{text}");
    assert!(text.contains("approval.resolved"), "{text}");
}

#[tokio::test]
async fn high_blast_wait_then_deny_fails_closed() {
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "write_file".into(),
                arguments: json!({ "path": "SOUL.md", "content": "evil" }),
            }],
        ),
        ModelResponse::text("denied path"),
    ]);
    let (app, dir) = harness_with_soul(model);
    let ws = dir.path().join("ws");

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
                .body(Body::from(json!({ "goal": "edit soul" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    let approval_id = wait_pending_approval(&app).await;
    let deny = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/approvals/{approval_id}/deny"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deny.status(), StatusCode::OK);
    assert_eq!(body_json(deny).await["status"], "denied");

    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(
        std::fs::read_to_string(ws.join("SOUL.md")).unwrap(),
        "identity"
    );
}

#[tokio::test]
async fn unauthenticated_approve_and_deny_fail_closed() {
    let (app, _dir) = harness_with_soul(FakeModelProvider::with_fixed_content("x"));
    for path in [
        "/v1/approvals/00000000-0000-0000-0000-000000000001/approve",
        "/v1/approvals/00000000-0000-0000-0000-000000000001/deny",
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn approval_pending_survives_sqlite_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let session = keryx_domain::Session::new(PrincipalId::new(PRINCIPAL));
    store.create_session(session.clone()).await.unwrap();
    let run = keryx_domain::Run::start(session.id, session.principal_id.clone(), "goal");
    store.create_run(run.clone()).await.unwrap();
    let approval = Approval::pending(run.id, session.principal_id, "write_file", "path=SOUL.md");
    let id = approval.id;
    store.create_approval(approval).await.unwrap();

    drop(store);
    let reopened = SqliteSessionStore::open(dir.path()).unwrap();
    let restored = reopened.get_approval(id).await.unwrap().expect("row");
    assert_eq!(restored.status, ApprovalStatus::Pending);
    assert_eq!(restored.action, "write_file");
    let list = reopened.list_approvals(true).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);
}
