//! Seam 1 — Memory CRUD/search + session_search; reduced origin denies memory_write.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall};
use keryx_domain::{MessageRole, Principal, RunOrigin};
use keryx_model::FakeModelProvider;
use keryx_storage::{InMemorySessionStore, SqliteSessionStore};
use keryx_tools::MemoryTools;
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
    panic!("timeout");
}

#[tokio::test]
async fn memory_write_search_and_session_search() {
    let store = Arc::new(InMemorySessionStore::new());
    let tools = Arc::new(MemoryTools::new(
        Arc::clone(&store),
        HashSet::from([
            "memory_write".into(),
            "memory_search".into(),
            "session_search".into(),
            "memory_read".into(),
        ]),
    ));
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "memory_write".into(),
                    arguments: json!({
                        "content": "operator prefers dark mode UI",
                        "label": "pref"
                    }),
                },
                ToolCall {
                    name: "memory_search".into(),
                    arguments: json!({ "query": "dark mode" }),
                },
                ToolCall {
                    name: "session_search".into(),
                    arguments: json!({ "query": "dark mode" }),
                },
            ],
        ),
        ModelResponse::text("remembered"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
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
                .body(Body::from(json!({ "goal": "remember prefs" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");

    let mem = store.list_memory().await.unwrap();
    assert_eq!(mem.len(), 1);
    assert!(mem[0].content.contains("dark mode"));

    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool && m.content.contains("memory_search") && m.content.contains("dark")
        }),
        "{:?}",
        transcript.messages
    );
    // session_search must surface prior user goal / tool text.
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && m.content.contains("session_search")
                && (m.content.contains("dark") || m.content.contains("remember") || m.content.contains("hits="))
        }),
        "session_search must return hits over Transcript: {:?}",
        transcript.messages
    );
    // Direct store search also finds the user goal transcript.
    let hits = store
        .search_transcripts("remember prefs", 10)
        .await
        .unwrap();
    assert!(
        !hits.is_empty(),
        "search_transcripts should find user goal"
    );
}

#[tokio::test]
async fn reduced_origin_denies_memory_write() {
    let store = Arc::new(InMemorySessionStore::new());
    // Adapter allows write; Policy for gateway must still deny.
    let tools = Arc::new(MemoryTools::new(
        Arc::clone(&store),
        HashSet::from(["memory_write".into(), "memory_search".into()]),
    ));
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "memory_write".into(),
                arguments: json!({ "content": "evil rewrite" }),
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
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "gw memory".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    for _ in 0..100 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        store.list_memory().await.unwrap().is_empty(),
        "reduced origin must not write Memory"
    );
    let transcript = store.get_transcript(session.id).await.unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && m.content.contains("memory_write")
                && (m.content.contains("denied") || m.content.contains("Policy"))
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn memory_survives_sqlite_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let mut entry = keryx_domain::MemoryEntry::new("durable fact about project keryx");
    entry.label = Some("fact".into());
    let id = entry.id;
    store.create_memory(entry).await.unwrap();
    let hits = store.search_memory("keryx", 10).await.unwrap();
    assert_eq!(hits.len(), 1);

    // CRUD update under FTS
    let mut updated = store.get_memory(id).await.unwrap().unwrap();
    updated.content = "updated durable fact about project keryx".into();
    store.update_memory(updated).await.unwrap();
    assert!(
        store
            .search_memory("updated", 10)
            .await
            .unwrap()
            .iter()
            .any(|e| e.id == id)
    );

    drop(store);
    let reopened = SqliteSessionStore::open(dir.path()).unwrap();
    let got = reopened.get_memory(id).await.unwrap().expect("row");
    assert!(got.content.contains("updated durable"));
    let hits = reopened.search_memory("project", 10).await.unwrap();
    assert!(!hits.is_empty());

    // delete
    reopened.delete_memory(id).await.unwrap();
    assert!(reopened.get_memory(id).await.unwrap().is_none());
    assert!(reopened.search_memory("keryx", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn sqlite_memory_tools_e2e_with_fts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let tools = Arc::new(MemoryTools::new(
        Arc::clone(&store),
        HashSet::from([
            "memory_write".into(),
            "memory_search".into(),
            "memory_update".into(),
            "memory_delete".into(),
            "memory_read".into(),
        ]),
    ));
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "memory_write".into(),
                    arguments: json!({ "content": "sqlite fts unique-token-xyz" }),
                },
                ToolCall {
                    name: "memory_search".into(),
                    arguments: json!({ "query": "unique-token-xyz" }),
                },
            ],
        ),
        ModelResponse::text("fts ok"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
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
                .body(Body::from(json!({ "goal": "fts" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    assert_eq!(wait_terminal(&app, &run_id).await["status"], "completed");
    let hits = store.search_memory("unique-token-xyz", 5).await.unwrap();
    assert_eq!(hits.len(), 1);
}
