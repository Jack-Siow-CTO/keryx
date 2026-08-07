//! Seam 1 — Child Runs: agent-facing spawn, linkage, exclusivity, budget carve, cancel cascade.
//!
//! ADR 0035 checklist line 4 / #79: Policy-gated product path (spawn_child_run tool).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunBudgets, RunLimits, SessionStore, ToolCall,
};
use keryx_domain::{Policy, Principal, RunOrigin, RunStatus};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
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

#[tokio::test]
async fn policy_subset_cannot_exceed_parent() {
    let parent = Policy::control_plane_default();
    let mut wider = Policy::deny_all();
    wider.allowed_tools = parent.allowed_tools.clone();
    wider.allowed_tools.insert("shell_exec".into());
    let child = wider.subset_of(&parent);
    assert!(!child.allows_tool("shell_exec"));
    assert!(child.allows_tool("read_file"));
}

#[tokio::test]
async fn spawn_child_under_active_root_with_linkage() {
    let store = Arc::new(InMemorySessionStore::new());
    // Parent stays active long enough to spawn.
    let model = Arc::new(FakeModelProvider::with_delay(
        Duration::from_millis(200),
        "parent done",
    ));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let parent = control
        .start_run(principal, session.id, "parent goal".into(), None, None)
        .await
        .unwrap();
    assert!(parent.is_root());
    assert_eq!(parent.origin, RunOrigin::ControlPlane);

    let child = control
        .spawn_child_run(parent.id, "child goal".into(), Some(2))
        .await
        .unwrap();
    assert!(!child.is_root());
    assert_eq!(child.parent_run_id, Some(parent.id));
    assert_eq!(child.session_id, parent.session_id);
    assert_eq!(child.goal, "child goal");

    // Wait for both to leave active.
    for _ in 0..100 {
        let p = store.get_run(parent.id).await.unwrap().unwrap();
        let c = store.get_run(child.id).await.unwrap().unwrap();
        if p.status.is_terminal() && c.status.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let c = store.get_run(child.id).await.unwrap().unwrap();
    assert!(c.status.is_terminal());
    assert_eq!(c.parent_run_id, Some(parent.id));
}

#[tokio::test]
async fn session_still_one_active_root_while_child_runs() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_delay(
        Duration::from_millis(150),
        "slow",
    ));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let parent = control
        .start_run(principal.clone(), session.id, "root".into(), None, None)
        .await
        .unwrap();
    let _child = control
        .spawn_child_run(parent.id, "delegate".into(), Some(1))
        .await
        .unwrap();

    // Second root must still be rejected.
    let err = control
        .start_run(principal, session.id, "second root".into(), None, None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("active run") || err.to_string().contains("already has"),
        "{err}"
    );
}

#[tokio::test]
async fn cancel_root_cascades_to_children() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_delay(
        Duration::from_secs(5),
        "never",
    ));
    let control = Arc::new(ControlPlane::new(Arc::clone(&store), model));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));

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
                .body(Body::from(json!({ "goal": "long parent" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let parent_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let parent_run = parent_id.parse().unwrap();

    let child = control
        .spawn_child_run(parent_run, "long child".into(), Some(2))
        .await
        .unwrap();

    let cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/runs/{parent_id}/cancel"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel.status(), StatusCode::OK);

    for _ in 0..100 {
        let p = store.get_run(parent_run).await.unwrap().unwrap();
        let c = store.get_run(child.id).await.unwrap().unwrap();
        if p.status.is_terminal() && c.status.is_terminal() {
            assert!(
                matches!(p.status, RunStatus::Cancelled | RunStatus::Interrupted),
                "parent {p:?}"
            );
            assert!(
                matches!(c.status, RunStatus::Cancelled | RunStatus::Interrupted),
                "child {c:?}"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("cancel cascade did not complete");
}

#[tokio::test]
async fn child_budget_carved_from_parent() {
    let parent = RunBudgets {
        max_duration: Some(Duration::from_secs(10)),
        max_tokens: Some(1000),
        max_tool_calls: Some(10),
    };
    let child = parent.carve_for_child(Some(100));
    assert_eq!(child.max_tool_calls, Some(10)); // capped by parent
    assert_eq!(child.max_tokens, Some(500));
    assert_eq!(child.max_duration, Some(Duration::from_secs(5)));

    let child2 = parent.carve_for_child(Some(3));
    assert_eq!(child2.max_tool_calls, Some(3));
}

#[tokio::test]
async fn get_run_exposes_parent_linkage_via_http() {
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_delay(
        Duration::from_millis(100),
        "ok",
    ));
    let control = Arc::new(ControlPlane::with_limits(
        Arc::clone(&store),
        model,
        RunLimits::default(),
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));

    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let parent = control
        .start_run(principal, session.id, "p".into(), None, None)
        .await
        .unwrap();
    let child = control
        .spawn_child_run(parent.id, "c".into(), Some(1))
        .await
        .unwrap();

    let get = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{}", child.id))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let body = body_json(get).await;
    assert_eq!(body["parent_run_id"], parent.id.to_string());
    assert_eq!(body["goal"], "c");
}

// --- ADR 0035 line 4: agent-facing spawn (Policy-gated tool) ---

async fn wait_run_terminal(store: &InMemorySessionStore, run_id: keryx_domain::RunId) {
    for _ in 0..200 {
        let r = store.get_run(run_id).await.unwrap().unwrap();
        if r.status.is_terminal() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run {run_id} did not finish");
}

#[tokio::test]
async fn agent_tool_spawn_child_under_policy_with_linkage() {
    // Root model: call spawn_child_run, then finish. Child: quick reply.
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "spawn_child_run".into(),
                arguments: json!({
                    "goal": "delegated research",
                    "max_tool_calls": 2
                }),
            }],
        ),
        ModelResponse::text("parent done after spawn"),
        // Child agent loop may consume subsequent script steps.
        ModelResponse::text("child done"),
        ModelResponse::text("child done"),
    ]);
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(model);
    let control = Arc::new(ControlPlane::with_limits(
        Arc::clone(&store),
        model,
        RunLimits::default(),
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));

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
                .body(Body::from(
                    json!({ "goal": "parent orchestrates" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::CREATED);
    let parent_body = body_json(start).await;
    let parent_id = parent_body["id"].as_str().unwrap().to_string();
    assert!(
        parent_body["parent_run_id"].is_null(),
        "root has no parent: {parent_body}"
    );

    wait_run_terminal(store.as_ref(), parent_id.parse().expect("parent run id")).await;

    // Child must exist with linkage via GET Run (control-plane projection).
    let runs = store
        .list_runs_for_session(session_id.parse().unwrap())
        .await
        .unwrap();
    let child = runs
        .iter()
        .find(|r| !r.is_root())
        .expect("agent tool must spawn a Child Run");
    assert_eq!(child.goal, "delegated research");
    assert_eq!(
        child.parent_run_id.map(|id| id.to_string()),
        Some(parent_id.clone())
    );

    let get_child = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/runs/{}", child.id))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_child.status(), StatusCode::OK);
    let child_body = body_json(get_child).await;
    assert_eq!(child_body["parent_run_id"], parent_id);
    assert_eq!(child_body["goal"], "delegated research");

    // Parent transcript records the tool observation.
    let transcript = store
        .get_transcript(session_id.parse().unwrap())
        .await
        .unwrap();
    let tool_obs = transcript.messages.iter().any(|m| {
        m.tool
            .as_ref()
            .map(|t| t.name == "spawn_child_run" && t.status == "ok")
            .unwrap_or(false)
    });
    assert!(
        tool_obs,
        "expected spawn_child_run ok tool row in Transcript"
    );
}

#[tokio::test]
async fn agent_tool_spawn_keeps_one_active_root() {
    // Delay each model step so parent stays Active after spawn while we probe exclusivity.
    let model = Arc::new(FakeModelProvider::with_delay_and_script(
        Duration::from_millis(50),
        vec![
            ModelResponse::with_tool_calls(
                "",
                vec![ToolCall {
                    name: "spawn_child_run".into(),
                    arguments: json!({ "goal": "child work" }),
                }],
            ),
            ModelResponse::text("parent done"),
            ModelResponse::text("child done"),
            ModelResponse::text("child done"),
        ],
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_limits(
        Arc::clone(&store),
        Arc::clone(&model),
        RunLimits::default(),
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let parent = control
        .start_run(
            principal.clone(),
            session.id,
            "root via tool".into(),
            None,
            None,
        )
        .await
        .unwrap();

    // Wait until a child appears (spawn tool ran).
    let mut child_seen = false;
    for _ in 0..100 {
        let runs = store.list_runs_for_session(session.id).await.unwrap();
        if runs.iter().any(|r| !r.is_root()) {
            child_seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(child_seen, "child must appear after agent spawn tool");

    // Second root still rejected while root Active (children do not free the slot).
    let parent_still = store.get_run(parent.id).await.unwrap().unwrap();
    if parent_still.status == RunStatus::Active {
        let err = control
            .start_run(principal, session.id, "second root".into(), None, None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("active run") || err.to_string().contains("already has"),
            "{err}"
        );
    }
    let roots: Vec<_> = store
        .list_runs_for_session(session.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.is_root())
        .collect();
    assert_eq!(roots.len(), 1, "only one root Run in Session: {roots:?}");
}

#[tokio::test]
async fn agent_tool_spawn_cancel_root_cascades() {
    // Parent spawns child; delay keeps both Active until cancel.
    let model = Arc::new(FakeModelProvider::with_delay_and_script(
        Duration::from_secs(5),
        vec![
            ModelResponse::with_tool_calls(
                "",
                vec![ToolCall {
                    name: "spawn_child_run".into(),
                    arguments: json!({ "goal": "child to cancel" }),
                }],
            ),
            ModelResponse::text("parent never finishes if cancelled"),
            ModelResponse::text("child slow"),
            ModelResponse::text("child slow 2"),
        ],
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_limits(
        Arc::clone(&store),
        Arc::clone(&model),
        RunLimits::default(),
    ));
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let app = router(AppState::new(Arc::clone(&control) as _, tokens));

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
                .body(Body::from(json!({ "goal": "parent long" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let parent_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let parent_run: keryx_domain::RunId = parent_id.parse().unwrap();

    // Wait for child after first delayed model step + spawn.
    let mut child_id = None;
    for _ in 0..300 {
        let runs = store
            .list_runs_for_session(session_id.parse().unwrap())
            .await
            .unwrap();
        if let Some(c) = runs.iter().find(|r| !r.is_root()) {
            child_id = Some(c.id);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let child_id = child_id.expect("agent must spawn child before parent finishes");

    let cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/runs/{parent_id}/cancel"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel.status(), StatusCode::OK);

    for _ in 0..100 {
        let p = store.get_run(parent_run).await.unwrap().unwrap();
        let c = store.get_run(child_id).await.unwrap().unwrap();
        if p.status.is_terminal() && c.status.is_terminal() {
            assert!(
                matches!(p.status, RunStatus::Cancelled | RunStatus::Interrupted),
                "parent {p:?}"
            );
            assert!(
                matches!(c.status, RunStatus::Cancelled | RunStatus::Interrupted),
                "child {c:?}"
            );
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("cancel cascade after agent spawn did not complete");
}

#[tokio::test]
async fn reduced_policy_denies_spawn_child_run_tool() {
    let model = Arc::new(FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "spawn_child_run".into(),
                arguments: json!({ "goal": "should be denied" }),
            }],
        ),
        ModelResponse::text("done without child"),
    ]));
    let store = Arc::new(InMemorySessionStore::new());
    let control = Arc::new(ControlPlane::with_limits(
        Arc::clone(&store),
        model,
        RunLimits::default(),
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "gateway reduced".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    wait_run_terminal(store.as_ref(), run.id).await;

    let runs = store.list_runs_for_session(session.id).await.unwrap();
    assert!(
        runs.iter().all(|r| r.is_root()),
        "reduced Policy must not spawn children: {runs:?}"
    );
    let transcript = store.get_transcript(session.id).await.unwrap();
    let denied = transcript.messages.iter().any(|m| {
        m.tool
            .as_ref()
            .map(|t| {
                t.name == "spawn_child_run" && (t.status == "error" || t.summary.contains("denied"))
            })
            .unwrap_or(false)
    });
    assert!(
        denied,
        "expected Policy deny on spawn_child_run for reduced origin"
    );
}

#[tokio::test]
async fn control_plane_policy_allows_spawn_child_run() {
    assert!(Policy::control_plane_default().allows_tool("spawn_child_run"));
    assert!(!Policy::reduced().allows_tool("spawn_child_run"));
}
