//! Seam 1 — web_search / web_extract with doubles (no live network), SSRF fail-closed.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{ControlPlane, ModelResponse, RunLimits, SessionStore, ToolCall};
use keryx_domain::MessageRole;
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use keryx_tools::{FixedWebExtract, FixedWebSearch, SearchHit, WebTools};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const TOKEN: &str = "test-operator-token";
const PRINCIPAL: &str = "operator-main";

fn harness(model: FakeModelProvider) -> (axum::Router, Arc<InMemorySessionStore>) {
    let store = Arc::new(InMemorySessionStore::new());
    let search = Arc::new(FixedWebSearch {
        hits: vec![SearchHit {
            title: "Public Doc".into(),
            url: "https://example.com/doc".into(),
            snippet: "safe public snippet".into(),
        }],
    });
    let extract = Arc::new(FixedWebExtract {
        pages: HashMap::from([(
            "https://example.com/doc".into(),
            "PUBLIC_BODY_MARKER_only_in_transcript secret_api_key=SHOULD_NOT_APPEAR_IN_EVENTS"
                .into(),
        )]),
        enforce_ssrf: true,
    });
    let tools = Arc::new(WebTools::new(
        HashSet::from(["web_search".into(), "web_extract".into()]),
        search,
        extract,
    ));
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

#[tokio::test]
async fn web_search_and_extract_happy_path_with_summarized_events() {
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "web_search".into(),
                    arguments: json!({ "query": "keryx docs", "token": "super-secret-token" }),
                },
                ToolCall {
                    name: "web_extract".into(),
                    arguments: json!({ "url": "https://example.com/doc" }),
                },
            ],
        ),
        ModelResponse::text("web tools done"),
    ]);

    let (app, store) = harness(model);
    let session_id = create_session(&app).await;
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "search and extract" }).to_string()))
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
        .map(|m| m.content.clone())
        .collect();
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.contains("web_search") && m.contains("example.com")),
        "{tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|m| m.contains("PUBLIC_BODY_MARKER_only_in_transcript")),
        "full extract body should land in transcript: {tool_msgs:?}"
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
    assert!(
        events_body.contains("[REDACTED]"),
        "secret arg key should redact: {events_body}"
    );
    assert!(
        !events_body.contains("super-secret-token"),
        "secret leaked: {events_body}"
    );
    assert!(
        !events_body.contains("PUBLIC_BODY_MARKER_only_in_transcript"),
        "extract body must not flood SSE: {events_body}"
    );
    assert!(
        !events_body.contains("SHOULD_NOT_APPEAR_IN_EVENTS"),
        "body secret leaked to events: {events_body}"
    );
}

#[tokio::test]
async fn web_extract_ssrf_private_ip_denied() {
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "web_extract".into(),
                    arguments: json!({ "url": "http://127.0.0.1/admin" }),
                },
                ToolCall {
                    name: "web_extract".into(),
                    arguments: json!({ "url": "http://169.254.169.254/latest/meta-data/" }),
                },
                ToolCall {
                    name: "web_extract".into(),
                    arguments: json!({ "url": "http://192.168.0.1/" }),
                },
            ],
        ),
        ModelResponse::text("ssrf blocked"),
    ]);

    let (app, store) = harness(model);
    let session_id = create_session(&app).await;
    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "ssrf" }).to_string()))
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
        .map(|m| m.content.clone())
        .collect();
    assert_eq!(tool_msgs.len(), 3);
    assert!(
        tool_msgs
            .iter()
            .all(|m| m.contains("denied") || m.contains("private") || m.contains("link-local")),
        "all private targets must fail closed: {tool_msgs:?}"
    );
}
