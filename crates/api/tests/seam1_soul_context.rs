//! Seam 1 — Soul + workspace Context files attach to Runs; high-blast edit denied.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ModelResponse, RunContextConfig, RunLimits, SessionStore, ToolCall};
use keryx_domain::MessageRole;
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
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
    panic!("run did not finish");
}

#[tokio::test]
async fn soul_and_context_attach_to_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let soul = dir.path().join("SOUL.md");
    std::fs::write(&soul, "You are Keryx Soul: prefer short answers.").unwrap();
    let ws = dir.path().join("workspace");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("CONTEXT.md"), "Project rule: use snake_case.").unwrap();

    let model = FakeModelProvider::with_fixed_content("noted soul and context");
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(WorkspaceFsTools::new(
        vec![ws.clone()],
        HashSet::from([
            "read_file".into(),
            "write_file".into(),
            "apply_patch".into(),
        ]),
    ));
    let control = Arc::new(ControlPlane::with_tools_and_context(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
        RunContextConfig {
            soul_path: Some(soul),
            context_files: vec!["CONTEXT.md".into()],
            workspace_roots: vec![ws],
            missing: keryx_app::MissingContextPolicy::Soft,
        },
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
                .body(Body::from(json!({ "goal": "hello" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    let system: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .collect();
    assert!(
        system
            .iter()
            .any(|m| m.content.contains("[Soul]") && m.content.contains("Keryx Soul")),
        "Soul must attach: {system:?}"
    );
    assert!(
        system
            .iter()
            .any(|m| m.content.contains("[Context file: CONTEXT.md]")
                && m.content.contains("snake_case")),
        "Context must attach: {system:?}"
    );
    // Soul ≠ Memory ≠ Skill labels stay distinct in content.
    assert!(system.iter().all(|m| !m.content.starts_with("[Memory]")));
    assert!(system.iter().all(|m| !m.content.starts_with("[Skill]")));
}

#[tokio::test]
async fn soul_edit_is_high_blast_denied() {
    let dir = tempfile::tempdir().unwrap();
    let soul = dir.path().join("SOUL.md");
    std::fs::write(&soul, "original soul").unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    // Also protect by placing soul name under workspace for path match tests.
    std::fs::write(ws.join("SOUL.md"), "workspace soul copy").unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "write_file".into(),
                arguments: json!({ "path": "SOUL.md", "content": "hijacked" }),
            }],
        ),
        ModelResponse::text("tried to edit soul"),
    ]);
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(WorkspaceFsTools::new(
        vec![ws.clone()],
        HashSet::from(["write_file".into()]),
    ));
    let control = Arc::new(ControlPlane::with_tools_and_context(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
        RunContextConfig {
            soul_path: Some(soul.clone()),
            context_files: vec![],
            workspace_roots: vec![ws.clone()],
            missing: keryx_app::MissingContextPolicy::Soft,
        },
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
                .body(Body::from(json!({ "goal": "edit soul" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    // High-blast waits on Approval — deny it.
    let mut denied = false;
    for _ in 0..100 {
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
        let body = body_json(list).await;
        if let Some(id) = body["approvals"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|a| a["id"].as_str())
        {
            let deny = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/v1/approvals/{id}/deny"))
                        .header("authorization", format!("Bearer {TOKEN}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(deny.status(), StatusCode::OK);
            denied = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(denied, "expected pending Approval for soul edit");

    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("high-blast") || m.content.contains("denied"))
        }),
        "expected high-blast deny: {:?}",
        transcript.messages
    );
    // Workspace SOUL.md must not be rewritten.
    assert_eq!(
        std::fs::read_to_string(ws.join("SOUL.md")).unwrap(),
        "workspace soul copy"
    );
    assert_eq!(std::fs::read_to_string(&soul).unwrap(), "original soul");
}

#[tokio::test]
async fn context_edit_apply_patch_high_blast_denied() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("CONTEXT.md"), "project rules").unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "apply_patch".into(),
                arguments: json!({
                    "path": "context.md",
                    "old_string": "project",
                    "new_string": "hijacked"
                }),
            }],
        ),
        ModelResponse::text("tried patch"),
    ]);
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(WorkspaceFsTools::new(
        vec![ws.clone()],
        HashSet::from(["apply_patch".into()]),
    ));
    let control = Arc::new(ControlPlane::with_tools_and_context(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
        RunContextConfig {
            soul_path: None,
            context_files: vec!["CONTEXT.md".into()],
            workspace_roots: vec![ws.clone()],
            missing: keryx_app::MissingContextPolicy::Soft,
        },
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
                .body(Body::from(json!({ "goal": "patch context" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    // Deny the high-blast Approval for context patch.
    for _ in 0..100 {
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
        let body = body_json(list).await;
        if let Some(id) = body["approvals"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|a| a["id"].as_str())
        {
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/v1/approvals/{id}/deny"))
                        .header("authorization", format!("Bearer {TOKEN}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(
        std::fs::read_to_string(ws.join("CONTEXT.md")).unwrap(),
        "project rules"
    );
    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("high-blast") || m.content.contains("denied"))
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn missing_soul_fails_soft() {
    let dir = tempfile::tempdir().unwrap();
    let missing_soul = dir.path().join("no-such-soul.md");
    let model = FakeModelProvider::with_fixed_content("ok without soul");
    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_tools_and_context(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        Arc::new(keryx_app::DenyAllTools),
        RunContextConfig {
            soul_path: Some(missing_soul),
            context_files: vec![],
            workspace_roots: vec![],
            missing: keryx_app::MissingContextPolicy::Soft,
        },
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
                .body(Body::from(json!({ "goal": "go" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        transcript
            .messages
            .iter()
            .any(|m| { m.role == MessageRole::System && m.content.contains("not loaded") }),
        "soft missing note expected: {:?}",
        transcript.messages
    );
}
