//! Seam 1 — Run origin stamp + origin-based reduced Policy (fail closed).
//!
//! Covers issue #14: control_plane origin on HTTP start_run; reduced Policy for
//! gateway/schedule origins; SQLite durability of origin.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
};
use keryx_domain::{MessageRole, Principal, RunOrigin};
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

fn tools_with_read_write(root: &std::path::Path) -> Arc<WorkspaceFsTools> {
    Arc::new(WorkspaceFsTools::new(
        vec![root.to_path_buf()],
        HashSet::from([
            "read_file".into(),
            "write_file".into(),
            "apply_patch".into(),
            "search_files".into(),
        ]),
    ))
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

async fn wait_terminal(app: &axum::Router, run_id: &str) -> Value {
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
    panic!("run {run_id} did not leave active status in time");
}

#[tokio::test]
async fn control_plane_start_run_stamps_origin_control_plane() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content("ok"));
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
    assert_eq!(create.status(), StatusCode::CREATED);
    let session_id = body_json(create).await["id"].as_str().unwrap().to_string();

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "hello origin" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run = body_json(start).await;
    assert_eq!(run["origin"], "control_plane");
    assert_eq!(run["principal_id"], PRINCIPAL);

    let run_id = run["id"].as_str().unwrap();
    let record = wait_terminal(&app, run_id).await;
    assert_eq!(record["origin"], "control_plane");
    assert_eq!(record["status"], "completed");
}

#[tokio::test]
async fn reduced_gateway_origin_denies_write_file_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "alpha").unwrap();

    // Adapter allows write/patch; origin Policy must still deny for gateway.
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "write_file".into(),
                    arguments: json!({ "path": "evil.txt", "content": "should-not-write" }),
                },
                ToolCall {
                    name: "apply_patch".into(),
                    arguments: json!({
                        "path": "note.txt",
                        "old_string": "alpha",
                        "new_string": "evil"
                    }),
                },
                ToolCall {
                    name: "read_file".into(),
                    arguments: json!({ "path": "note.txt" }),
                },
            ],
        ),
        ModelResponse::text("handled under reduced Policy"),
    ]);

    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools_with_read_write(root.path()),
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();

    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "gateway write attempt".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(run.origin.as_str(), "gateway:telegram");

    // Poll via store until terminal.
    let mut terminal = None;
    for _ in 0..100 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            terminal = Some(r);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let terminal = terminal.expect("run finished");
    assert_eq!(terminal.origin.as_str(), "gateway:telegram");
    assert_eq!(terminal.status, keryx_domain::RunStatus::Completed);

    assert!(
        !root.path().join("evil.txt").exists(),
        "write_file must not execute under reduced Policy"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "alpha",
        "apply_patch must not execute under reduced Policy"
    );

    let transcript = store.get_transcript(session.id).await.unwrap();
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .collect();
    assert!(
        tool_msgs.iter().any(|m| {
            m.content.contains("write_file")
                && (m.content.contains("denied") || m.content.contains("Policy"))
        }),
        "expected Policy deny for write_file: {tool_msgs:?}"
    );
    assert!(
        tool_msgs.iter().any(|m| {
            m.content.contains("apply_patch")
                && (m.content.contains("denied") || m.content.contains("Policy"))
        }),
        "expected Policy deny for apply_patch: {tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.content.contains("read_file") && m.content.contains("alpha")),
        "read_file should still work under reduced Policy: {tool_msgs:?}"
    );
}

#[tokio::test]
async fn reduced_schedule_origin_denies_unknown_and_write() {
    let root = tempfile::tempdir().unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "shell_exec".into(),
                    arguments: json!({ "cmd": "id" }),
                },
                ToolCall {
                    name: "write_file".into(),
                    arguments: json!({ "path": "x.txt", "content": "nope" }),
                },
            ],
        ),
        ModelResponse::text("denied both"),
    ]);

    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools_with_read_write(root.path()),
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "schedule attempt".into(),
            RunOrigin::schedule(),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(run.origin.as_str(), "schedule");

    for _ in 0..100 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let transcript = store.get_transcript(session.id).await.unwrap();
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| m.content.clone())
        .collect();
    assert_eq!(tool_msgs.len(), 2, "both tools should produce deny results");
    assert!(
        tool_msgs
            .iter()
            .all(|c| c.contains("denied") || c.contains("Policy")),
        "expected fail-closed denials: {tool_msgs:?}"
    );
    assert!(!root.path().join("x.txt").exists());
}

#[tokio::test]
async fn control_plane_origin_allows_write_via_http() {
    let root = tempfile::tempdir().unwrap();
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "write_file".into(),
                arguments: json!({ "path": "ok.txt", "content": "trusted" }),
            }],
        ),
        ModelResponse::text("wrote"),
    ]);

    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools_with_read_write(root.path()),
    ));
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
                .body(Body::from(json!({ "goal": "write ok" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run = body_json(start).await;
    assert_eq!(run["origin"], "control_plane");
    let run_id = run["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(
        std::fs::read_to_string(root.path().join("ok.txt")).unwrap(),
        "trusted"
    );
}

#[tokio::test]
async fn sqlite_retains_run_origin_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::with_fixed_content("durability"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();

    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "persist origin".into(),
            RunOrigin::gateway("discord"),
            None,
            None,
        )
        .await
        .unwrap();
    let run_id = run.id;

    for _ in 0..100 {
        let r = store.get_run(run_id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Reopen SQLite (Worker restart simulation).
    drop(control);
    drop(store);
    let reopened = SqliteSessionStore::open(dir.path()).unwrap();
    let restored = reopened.get_run(run_id).await.unwrap().expect("run row");
    assert_eq!(restored.origin.as_str(), "gateway:discord");
    assert_eq!(restored.goal, "persist origin");
    assert!(restored.status.is_terminal());
}
