//! Seam 1 — Terminal tools: docker default for reduced, local Approval for control_plane.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
    ToolRuntime,
};
use keryx_domain::{MessageRole, Principal, RunOrigin};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use keryx_tools::{ExecBackend, FixedExecRunner, TerminalTools};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

async fn body_json(r: axum::response::Response) -> Value {
    serde_json::from_slice(
        &r.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap()
}

#[tokio::test]
async fn control_plane_local_exec_requires_approval_then_runs() {
    let store = Arc::new(InMemorySessionStore::new());
    let runner = Arc::new(FixedExecRunner::default());
    let tools = Arc::new(
        TerminalTools::new(
            HashSet::from(["run_terminal".into()]),
            runner,
            RunOrigin::ControlPlane,
        )
        .with_force_backend(ExecBackend::Local),
    );
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "run_terminal".into(),
                arguments: json!({ "command": "echo hi", "backend": "local" }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));

    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run(principal, session.id, "exec".into(), None, None)
        .await
        .unwrap();

    // Approve pending high-blast.
    let mut approved = false;
    for _ in 0..50 {
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
                        .uri(format!("/v1/approvals/{id}/approve"))
                        .header("authorization", format!("Bearer {TOKEN}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            approved = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(approved);

    for _ in 0..50 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let transcript = store.get_transcript(session.id).await.unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool && m.content.contains("run_terminal") && m.content.contains("Local")
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn reduced_origin_denies_local_exec() {
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(TerminalTools::new(
        HashSet::from(["run_terminal".into()]),
        Arc::new(FixedExecRunner::default()),
        RunOrigin::gateway("telegram"),
    ));
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "run_terminal".into(),
                arguments: json!({ "command": "id", "backend": "local" }),
            }],
        ),
        ModelResponse::text("ok"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "gw".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    for _ in 0..50 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let transcript = store.get_transcript(session.id).await.unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("denied") || m.content.contains("docker"))
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn reduced_origin_docker_backend_works_with_double() {
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(
        TerminalTools::new(
            HashSet::from(["run_terminal".into()]),
            Arc::new(FixedExecRunner::default()),
            RunOrigin::Schedule,
        )
        .with_force_backend(ExecBackend::Docker),
    );
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "run_terminal".into(),
                arguments: json!({ "command": "echo docker", "backend": "docker" }),
            }],
        ),
        ModelResponse::text("ok"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "sched".into(),
            RunOrigin::Schedule,
            None,
            None,
        )
        .await
        .unwrap();
    for _ in 0..50 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let transcript = store.get_transcript(session.id).await.unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool && m.content.contains("Docker")
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn reduced_origin_local_denied_even_if_tool_wired_as_control_plane() {
    // Production footgun: worker may wire TerminalTools with ControlPlane origin.
    // Agent loop must still deny local for reduced Runs.
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(
        TerminalTools::new(
            HashSet::from(["run_terminal".into()]),
            Arc::new(FixedExecRunner::default()),
            RunOrigin::ControlPlane, // mis-wired adapter origin
        )
        .with_force_backend(ExecBackend::Local),
    );
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "run_terminal".into(),
                arguments: json!({ "command": "id", "backend": "local" }),
            }],
        ),
        ModelResponse::text("x"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "gw".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    for _ in 0..50 {
        if store
            .get_run(run.id)
            .await
            .unwrap()
            .unwrap()
            .status
            .is_terminal()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let t = store.get_transcript(session.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && m.content.contains("denied")
                && m.content.contains("reduced")
        }),
        "expected service-layer reduced local deny: {:?}",
        t.messages
    );
}

#[tokio::test]
async fn cwd_escape_denied() {
    let tools = TerminalTools::new(
        HashSet::from(["run_terminal".into()]),
        Arc::new(FixedExecRunner::default()),
        RunOrigin::ControlPlane,
    )
    .with_cwd_roots(vec![std::env::temp_dir()])
    .with_force_backend(ExecBackend::Docker);
    let err = tools
        .invoke(ToolCall {
            name: "run_terminal".into(),
            arguments: json!({ "command": "pwd", "backend": "docker", "cwd": "../escape" }),
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("path jail") || err.to_string().contains("cwd"));
}
