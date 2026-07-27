//! Seam 1 — workspace file tools, path jail, tool events, transcript.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ModelResponse, RunLimits, SessionStore, ToolCall};
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

fn harness(
    root: &std::path::Path,
    model: FakeModelProvider,
) -> (axum::Router, Arc<InMemorySessionStore>) {
    harness_with_tools(
        root,
        model,
        HashSet::from([
            "read_file".into(),
            "write_file".into(),
            "apply_patch".into(),
            "search_files".into(),
        ]),
    )
}

fn harness_with_tools(
    root: &std::path::Path,
    model: FakeModelProvider,
    allowed: HashSet<String>,
) -> (axum::Router, Arc<InMemorySessionStore>) {
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(WorkspaceFsTools::new(vec![root.to_path_buf()], allowed));
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    (router(AppState::new(control, tokens)), store)
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

async fn collect_sse_events(app: &axum::Router, run_id: &str) -> Vec<String> {
    let events_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{run_id}/events"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events_response.status(), StatusCode::OK);
    let body = String::from_utf8(
        events_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    body.lines()
        .filter_map(|line| line.strip_prefix("event:"))
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect()
}

#[tokio::test]
async fn workspace_read_write_and_tool_events() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "alpha").unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "read_file".into(),
                    arguments: json!({ "path": "note.txt", "token": "super-secret" }),
                },
                ToolCall {
                    name: "write_file".into(),
                    arguments: json!({ "path": "out.txt", "content": "beta" }),
                },
            ],
        ),
        ModelResponse::text("done with tools"),
    ]);

    let (app, store) = harness(root.path(), model);
    let session_id = create_session(&app).await;
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "use files" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(record["result"], "done with tools");
    assert_eq!(
        std::fs::read_to_string(root.path().join("out.txt")).unwrap(),
        "beta"
    );

    let names = collect_sse_events(&app, &run_id).await;
    assert!(names.iter().any(|n| n == "tool.started"));
    assert!(names.iter().any(|n| n == "tool.finished"));
    assert_eq!(names.last().map(String::as_str), Some("run.completed"));

    // Secret-like arg redacted in event payload stream body.
    let events_body = {
        let resp = app
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
        String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    };
    assert!(
        events_body.contains("[REDACTED]"),
        "expected redacted secret in events: {events_body}"
    );
    assert!(
        !events_body.contains("super-secret"),
        "secret leaked in events: {events_body}"
    );

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert!(transcript
        .messages
        .iter()
        .any(|m| m.role == MessageRole::Tool && m.content.contains("alpha")));
    assert!(transcript
        .messages
        .iter()
        .any(|m| m.role == MessageRole::Assistant && m.content == "done with tools"));
}

#[tokio::test]
async fn path_escape_denied_and_unknown_tool_denied() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("ok.txt"), "safe").unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "read_file".into(),
                    arguments: json!({ "path": "../escape.txt" }),
                },
                ToolCall {
                    name: "shell_exec".into(),
                    arguments: json!({ "cmd": "id" }),
                },
            ],
        ),
        ModelResponse::text("handled denials"),
    ]);

    let (app, store) = harness(root.path(), model);
    let session_id = create_session(&app).await;
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "try escape" }).to_string()))
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
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .collect();
    assert!(
        tool_msgs.iter().any(|m| m.content.contains("path jail")),
        "expected path jail error in transcript: {tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.content.contains("denied") || m.content.contains("disallowed")),
        "expected unknown tool denial: {tool_msgs:?}"
    );
}

#[tokio::test]
async fn apply_patch_and_search_files_happy_path_and_jail() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("src.txt"), "alpha beta gamma").unwrap();
    // Query is short; BODY_ONLY_MARKER must appear only in file content (transcript), not SSE.
    std::fs::write(
        root.path().join("findme.txt"),
        "search-query-token BODY_ONLY_MARKER_9f3a",
    )
    .unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "search_files".into(),
                    arguments: json!({ "query": "search-query-token" }),
                },
                ToolCall {
                    name: "apply_patch".into(),
                    arguments: json!({
                        "path": "src.txt",
                        "old_string": "beta",
                        "new_string": "BETA"
                    }),
                },
                ToolCall {
                    name: "apply_patch".into(),
                    arguments: json!({
                        "path": "../escape.txt",
                        "old_string": "x",
                        "new_string": "y"
                    }),
                },
                ToolCall {
                    name: "search_files".into(),
                    arguments: json!({ "query": "x", "path": "/etc" }),
                },
            ],
        ),
        ModelResponse::text("patch and search done"),
    ]);

    let (app, store) = harness(root.path(), model);
    let session_id = create_session(&app).await;
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "patch and search" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert_eq!(
        std::fs::read_to_string(root.path().join("src.txt")).unwrap(),
        "alpha BETA gamma"
    );

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| m.content.clone())
        .collect();
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.contains("search_files") && m.contains("findme")),
        "expected search hit: {tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.contains("apply_patch") && m.contains("replacement")),
        "expected successful patch: {tool_msgs:?}"
    );
    assert!(
        tool_msgs.iter().any(|m| m.contains("path jail")),
        "expected path jail on patch escape: {tool_msgs:?}"
    );
    // Absolute outside roots denied for search_files path arg.
    assert!(
        tool_msgs
            .iter()
            .filter(|m| m.contains("search_files"))
            .any(|m| m.contains("path jail") || m.contains("outside")),
        "expected search path jail: {tool_msgs:?}"
    );

    let events_body = {
        let resp = app
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
        String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    };
    // Summaries present; full file content marker must not appear on SSE (only in transcript).
    // Query is "needle-in-haystack"; body-only marker is different.
    assert!(events_body.contains("apply_patch") || events_body.contains("tool.finished"));
    assert!(
        !events_body.contains("BODY_ONLY_MARKER_9f3a"),
        "file body leaked into events: {events_body}"
    );
    assert!(
        transcript
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Tool && m.content.contains("BODY_ONLY_MARKER_9f3a")),
        "expected body marker only in transcript tool result"
    );
}
