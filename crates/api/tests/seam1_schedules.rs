//! Seam 1 — Schedules: CRUD/pause/resume, fire with origin=schedule, frozen tools, durability.
//!
//! ADR 0035 checklist line 7 (GHA half): create Schedule → tick fires Run with origin
//! `schedule`, reduced Policy, and frozen `policy_tools` applied at fire.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
};
use keryx_domain::{MessageRole, Policy, Principal, RunOrigin, ScheduleStatus};
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

fn harness() -> (
    axum::Router,
    Arc<InMemorySessionStore>,
    Arc<ControlPlane<InMemorySessionStore, FakeModelProvider>>,
) {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_fixed_content("schedule fire ok"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));
    (app, store, control)
}

#[tokio::test]
async fn schedule_crud_pause_resume_delete() {
    let (app, _, _) = harness();
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "goal": "morning briefing",
                        "interval_secs": 60,
                        "next_fire_at": 1_000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body = body_json(create).await;
    assert_eq!(body["status"], "active");
    assert_eq!(body["goal"], "morning briefing");
    let id = body["id"].as_str().unwrap().to_string();

    let pause = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/pause"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(pause).await["status"], "paused");

    let resume = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/resume"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(resume).await["status"], "active");

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        body_json(list).await["schedules"].as_array().unwrap().len(),
        1
    );

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/schedules/{id}/delete"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_json(del).await["status"], "deleted");
}

#[tokio::test]
async fn tick_fires_run_with_origin_schedule_reduced_policy() {
    let (app, store, control) = harness();
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let schedule = control
        .create_schedule(principal, "do work".into(), 60, 1_000, None)
        .await
        .unwrap();
    assert!(Policy::for_origin(&RunOrigin::Schedule).allows_tool("read_file"));
    assert!(!Policy::for_origin(&RunOrigin::Schedule).allows_tool("write_file"));

    let tick = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 1_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick.status(), StatusCode::OK);
    let body = body_json(tick).await;
    let runs = body["started_runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["origin"], "schedule");
    assert_eq!(runs[0]["goal"], "do work");

    // Double-fire same second: no second start.
    let tick2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 1_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(body_json(tick2).await["started_runs"]
        .as_array()
        .unwrap()
        .is_empty());

    // Wait for run to finish so next fire can create another root.
    let run_id = runs[0]["id"].as_str().unwrap();
    for _ in 0..50 {
        let r = store
            .get_run(run_id.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let updated = store.get_schedule(schedule.id).await.unwrap().unwrap();
    assert_eq!(updated.last_fired_at, Some(1_000));
    assert!(updated.next_fire_at > 1_000);
}

#[tokio::test]
async fn missed_fire_advances_without_storm() {
    let mut s = keryx_domain::Schedule::new(
        keryx_domain::PrincipalId::new("p"),
        "g",
        60,
        100,
        vec!["read_file".into()],
    );
    // far past due
    s.record_fire(10_000);
    assert!(s.next_fire_at > 10_000 || s.next_fire_at == 10_000 + 60);
    // only one step — not a catch-up loop of many intervals
    assert!(s.next_fire_at <= 10_000 + 120);
}

#[tokio::test]
async fn schedule_survives_sqlite_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteSessionStore::open(dir.path()).unwrap());
    let model = Arc::new(FakeModelProvider::with_fixed_content("x"));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let schedule = control
        .create_schedule(principal, "persist".into(), 30, 500, None)
        .await
        .unwrap();
    let id = schedule.id;
    drop(control);
    drop(store);

    let reopened = SqliteSessionStore::open(dir.path()).unwrap();
    let got = reopened.get_schedule(id).await.unwrap().expect("row");
    assert_eq!(got.goal, "persist");
    assert_eq!(got.status, ScheduleStatus::Active);
    assert_eq!(got.interval_secs, 30);
}

#[tokio::test]
async fn unauthenticated_schedule_fail_closed() {
    let (app, _, _) = harness();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"goal":"x","interval_secs":1}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_schedule_freezes_policy_tools_default_and_explicit() {
    let (app, store, _) = harness();

    // Default: freeze reduced allowlist at create.
    let create_default = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "goal": "default freeze",
                        "interval_secs": 60,
                        "next_fire_at": 2_000
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_default.status(), StatusCode::CREATED);
    let body = body_json(create_default).await;
    let tools = body["policy_tools"].as_array().unwrap();
    assert!(
        tools.iter().any(|t| t.as_str() == Some("read_file")),
        "default freeze includes reduced tools: {tools:?}"
    );
    assert!(
        !tools.iter().any(|t| t.as_str() == Some("write_file")),
        "default freeze must not include write_file: {tools:?}"
    );
    let id = body["id"].as_str().unwrap().to_string();
    let stored = store
        .get_schedule(id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.policy_tools,
        body["policy_tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    // Explicit freeze is stored as authored.
    let create_explicit = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "goal": "tight freeze",
                        "interval_secs": 60,
                        "next_fire_at": 3_000,
                        "policy_tools": ["read_file"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_explicit.status(), StatusCode::CREATED);
    let body = body_json(create_explicit).await;
    assert_eq!(body["policy_tools"], json!(["read_file"]));
}

/// Checklist line 7 GHA: fire applies frozen policy_tools (not live reduced re-derive only).
#[tokio::test]
async fn tick_applies_frozen_policy_tools_at_fire() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "briefing").unwrap();

    // Model tries a tool outside the frozen allowlist and one inside.
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "search_files".into(),
                    arguments: json!({ "query": "secret" }),
                },
                ToolCall {
                    name: "read_file".into(),
                    arguments: json!({ "path": "note.txt" }),
                },
            ],
        ),
        ModelResponse::text("schedule done"),
    ]);

    let tools = Arc::new(WorkspaceFsTools::new(
        vec![root.path().to_path_buf()],
        HashSet::from([
            "read_file".into(),
            "write_file".into(),
            "apply_patch".into(),
            "search_files".into(),
        ]),
    ));
    let store = Arc::new(InMemorySessionStore::new());
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
    // Freeze only read_file — search_files is on reduced template but NOT frozen.
    let schedule = control
        .create_schedule(
            principal,
            "tight fire".into(),
            60,
            5_000,
            Some(vec!["read_file".into()]),
        )
        .await
        .unwrap();
    assert_eq!(schedule.policy_tools, vec!["read_file".to_string()]);

    let tick = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 5_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick.status(), StatusCode::OK);
    let body = body_json(tick).await;
    let runs = body["started_runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["origin"], "schedule");
    assert_eq!(runs[0]["goal"], "tight fire");
    let run_id = runs[0]["id"].as_str().unwrap();
    let session_id: keryx_domain::SessionId =
        runs[0]["session_id"].as_str().unwrap().parse().unwrap();

    // Wait for terminal.
    for _ in 0..100 {
        let r = store
            .get_run(run_id.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        if r.status.is_terminal() {
            assert_eq!(r.origin, RunOrigin::Schedule);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let transcript = store.get_transcript(session_id).await.unwrap();
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| m.content.clone())
        .collect();
    assert!(
        tool_msgs.iter().any(|c| {
            c.contains("search_files") && (c.contains("denied") || c.contains("Policy"))
        }),
        "search_files must be denied under frozen allowlist: {tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|c| c.contains("read_file") && c.contains("briefing")),
        "read_file must succeed under frozen allowlist: {tool_msgs:?}"
    );
}

#[tokio::test]
async fn tick_default_schedule_uses_reduced_frozen_tools() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "ok").unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "write_file".into(),
                    arguments: json!({ "path": "evil.txt", "content": "nope" }),
                },
                ToolCall {
                    name: "read_file".into(),
                    arguments: json!({ "path": "note.txt" }),
                },
            ],
        ),
        ModelResponse::text("reduced freeze ok"),
    ]);

    let tools = Arc::new(WorkspaceFsTools::new(
        vec![root.path().to_path_buf()],
        HashSet::from([
            "read_file".into(),
            "write_file".into(),
            "apply_patch".into(),
            "search_files".into(),
        ]),
    ));
    let store = Arc::new(InMemorySessionStore::new());
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
    // None → freeze reduced tools at create (create_schedule default).
    let schedule = control
        .create_schedule(principal, "default fire".into(), 60, 6_000, None)
        .await
        .unwrap();
    assert!(schedule.policy_tools.contains(&"read_file".to_string()));
    assert!(!schedule.policy_tools.contains(&"write_file".to_string()));

    let tick = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/schedules/tick")
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "now": 6_000 }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick.status(), StatusCode::OK);
    let runs = body_json(tick).await["started_runs"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["origin"], "schedule");
    let run_id = runs[0]["id"].as_str().unwrap();
    let session_id: keryx_domain::SessionId =
        runs[0]["session_id"].as_str().unwrap().parse().unwrap();

    for _ in 0..100 {
        let r = store
            .get_run(run_id.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        if r.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        !root.path().join("evil.txt").exists(),
        "write_file must not execute under schedule frozen reduced Policy"
    );
    let transcript = store.get_transcript(session_id).await.unwrap();
    let tool_msgs: Vec<_> = transcript
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .map(|m| m.content.clone())
        .collect();
    assert!(
        tool_msgs.iter().any(|c| {
            c.contains("write_file") && (c.contains("denied") || c.contains("Policy"))
        }),
        "write_file denied: {tool_msgs:?}"
    );
    assert!(
        tool_msgs
            .iter()
            .any(|c| c.contains("read_file") && c.contains("ok")),
        "read_file allowed: {tool_msgs:?}"
    );
}
