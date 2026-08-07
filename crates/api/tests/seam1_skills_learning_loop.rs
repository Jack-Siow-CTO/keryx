//! Seam 1 — Skills learning loop (ADR 0035 checklist line 6 / #76).
//!
//! Factory auto-commit OFF: control_plane Run skill_manage → pending Approval →
//! Approve writes agentskills package under skills root; list/load; later Run loads.
//! Deny leaves root unchanged. Gateway origin never silent-writes.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use keryx_api::{router, AppState, OperatorTokenTable};
use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
};
use keryx_domain::{MessageRole, Principal, RunOrigin};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use keryx_tools::SkillsTools;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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

fn skill_tools(root: PathBuf) -> Arc<SkillsTools> {
    Arc::new(SkillsTools::new(
        root,
        HashSet::from([
            "skills_list".into(),
            "skill_load".into(),
            "skill_manage".into(),
        ]),
    ))
}

fn harness(
    model: FakeModelProvider,
    skills_root: PathBuf,
    auto_commit: bool,
) -> (
    axum::Router,
    Arc<InMemorySessionStore>,
    Arc<ControlPlane<InMemorySessionStore, FakeModelProvider>>,
) {
    let store = Arc::new(InMemorySessionStore::new());
    let tools = skill_tools(skills_root.clone());
    let mut control = ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    );
    if auto_commit {
        control = control.with_skill_auto_commit(true);
    }
    // Factory default is OFF — leave skill_auto_commit false unless requested.
    let control = Arc::new(control);
    let tokens = OperatorTokenTable::new().with_token(TOKEN, PRINCIPAL);
    let state = AppState::new(Arc::clone(&control) as _, tokens)
        .with_console_paths(Some(skills_root), None);
    (router(state), store, control)
}

#[tokio::test]
async fn factory_skill_auto_commit_default_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    std::fs::create_dir_all(&root).unwrap();
    let model = FakeModelProvider::with_fixed_content("x");
    let (_app, _store, control) = harness(model, root, false);
    assert!(
        !control.skill_auto_commit(),
        "factory default skill auto-commit must be OFF"
    );
}

#[tokio::test]
async fn draft_approve_list_load_and_deny_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    std::fs::create_dir_all(&root).unwrap();
    // Empty root OK.
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());

    let skill_body = "# daily-note\n\nCapture a short daily note under notes/.\n";
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "daily-note",
                    "content": skill_body
                }),
            }],
        ),
        ModelResponse::text("proposed skill"),
    ]);
    let (app, _store, _control) = harness(model, root.clone(), false);

    // Empty list before any package.
    let list0 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/skills")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list0.status(), StatusCode::OK);
    assert_eq!(
        body_json(list0).await["skills"].as_array().unwrap().len(),
        0
    );

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
                .body(Body::from(
                    json!({ "goal": "create daily-note skill" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();

    // Proposal is pending Approval (high-blast skill_manage); root still empty.
    let approval_id = wait_pending_approval(&app).await;
    assert!(
        !root.join("daily-note").join("SKILL.md").is_file(),
        "skills root must not write before Approve"
    );

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
    let abody = body_json(approve).await;
    assert_eq!(abody["status"], "approved");
    assert_eq!(abody["action"], "skill_manage");

    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");

    let written = std::fs::read_to_string(root.join("daily-note").join("SKILL.md")).unwrap();
    assert_eq!(written, skill_body);

    // Control-plane Skills list surfaces the package.
    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/skills")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let skills = body_json(list).await["skills"].as_array().unwrap().clone();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["name"], "daily-note");

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/skills/daily-note")
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    assert!(body_json(get).await["content"]
        .as_str()
        .unwrap()
        .contains("daily note"));

    // Later Run loads the skill via skill_load.
    let load_model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_load".into(),
                arguments: json!({ "name": "daily-note" }),
            }],
        ),
        ModelResponse::text("loaded"),
    ]);
    let (app2, _, _) = harness(load_model, root.clone(), false);
    let create2 = app2
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
    let session2_id = body_json(create2).await["id"].as_str().unwrap().to_string();
    let start2 = app2
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session2_id}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "load skill" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run2 = body_json(start2).await["id"].as_str().unwrap().to_string();
    let rec2 = wait_terminal(&app2, &run2).await;
    assert_eq!(rec2["status"], "completed");
    let page = app2
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/sessions/{session2_id}/transcript?limit=50"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let page_body = body_json(page).await;
    let msgs = page_body["messages"].as_array().expect("messages");
    let loaded = msgs.iter().any(|m| {
        m["role"] == "tool"
            && m["content"]
                .as_str()
                .map(|c| c.contains("daily note") || c.contains("daily-note"))
                .unwrap_or(false)
    });
    assert!(loaded, "later Run must load skill content: {page_body}");

    // Second proposal Deny → root unchanged for that proposal.
    let deny_model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "evil-skill",
                    "content": "# evil\nnever\n"
                }),
            }],
        ),
        ModelResponse::text("denied"),
    ]);
    let (app3, _, _) = harness(deny_model, root.clone(), false);
    let create3 = app3
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
    let session3 = body_json(create3).await["id"].as_str().unwrap().to_string();
    let start3 = app3
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/sessions/{session3}/runs"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "goal": "bad skill" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run3 = body_json(start3).await["id"].as_str().unwrap().to_string();
    let deny_id = wait_pending_approval(&app3).await;
    let deny = app3
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/approvals/{deny_id}/deny"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deny.status(), StatusCode::OK);
    let rec3 = wait_terminal(&app3, &run3).await;
    assert_eq!(rec3["status"], "completed");
    assert!(
        !root.join("evil-skill").exists(),
        "Deny must leave skills root unchanged for that proposal"
    );
    // Prior package still present.
    assert!(root.join("daily-note").join("SKILL.md").is_file());
}

#[tokio::test]
async fn gateway_origin_never_silent_writes_skills_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    std::fs::create_dir_all(&root).unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "from-gateway",
                    "content": "# gw\nfrom telegram\n"
                }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    // Even with auto-commit ON, gateway must not silent-write.
    let store = Arc::new(InMemorySessionStore::new());
    let tools = skill_tools(root.clone());
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            tools,
        )
        .with_skill_auto_commit(true),
    );
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal.clone(),
            session.id,
            "gateway propose skill".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();

    // Wait for pending Approval or terminal without write.
    let mut saw_pending = false;
    for _ in 0..200 {
        let pending = control.list_approvals(true).await.unwrap();
        if !pending.is_empty() {
            saw_pending = true;
            assert_eq!(pending[0].action, "skill_manage");
            // Approve so the Run can finish cleanly (prove write only after Approve).
            let _ = control
                .approve(principal.clone(), pending[0].id)
                .await
                .unwrap();
            break;
        }
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        saw_pending,
        "gateway skill_manage must surface pending Approval, never silent-write"
    );
    for _ in 0..200 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        root.join("from-gateway").join("SKILL.md").is_file(),
        "after Approve, package is written"
    );
}

#[tokio::test]
async fn gateway_deny_leaves_skills_root_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    std::fs::create_dir_all(&root).unwrap();

    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "gw-denied",
                    "content": "# no\n"
                }),
            }],
        ),
        ModelResponse::text("ok"),
    ]);
    let store = Arc::new(InMemorySessionStore::new());
    let tools = skill_tools(root.clone());
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
            principal.clone(),
            session.id,
            "gw deny".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    let mut denied = false;
    for _ in 0..200 {
        let pending = control.list_approvals(true).await.unwrap();
        if let Some(a) = pending.first() {
            let _ = control.deny(principal.clone(), a.id).await.unwrap();
            denied = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(denied);
    for _ in 0..200 {
        let r = store.get_run(run.id).await.unwrap().unwrap();
        if r.status != keryx_domain::RunStatus::Active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!Path::new(&root).join("gw-denied").exists());
    let transcript = store.get_transcript(session.id).await.unwrap();
    assert!(
        transcript.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("denied") || m.content.contains("Approval"))
        }),
        "{:?}",
        transcript.messages
    );
}

#[tokio::test]
async fn trusted_auto_commit_on_skips_approval_for_control_plane() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("skills");
    std::fs::create_dir_all(&root).unwrap();
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "trusted",
                    "content": "# trusted\nok\n"
                }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let (app, _, control) = harness(model, root.clone(), true);
    assert!(control.skill_auto_commit());

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
                .body(Body::from(json!({ "goal": "auto apply" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = body_json(start).await["id"].as_str().unwrap().to_string();
    let record = wait_terminal(&app, &run_id).await;
    assert_eq!(record["status"], "completed");
    assert!(root.join("trusted").join("SKILL.md").is_file());
    // No pending Approvals left / none created for this path.
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
    assert!(body_json(list).await["approvals"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn skills_root_doctor_posture() {
    use keryx_tools::skills_root_doctor_status;

    let missing = tempfile::tempdir().unwrap().path().join("nope");
    assert_eq!(
        skills_root_doctor_status(&missing).kind,
        keryx_tools::SkillsRootDoctorKind::Missing
    );

    let empty = tempfile::tempdir().unwrap();
    assert_eq!(
        skills_root_doctor_status(empty.path()).kind,
        keryx_tools::SkillsRootDoctorKind::Empty
    );

    let with_pkg = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(with_pkg.path().join("demo")).unwrap();
    std::fs::write(with_pkg.path().join("demo").join("SKILL.md"), "# d\n").unwrap();
    assert_eq!(
        skills_root_doctor_status(with_pkg.path()).kind,
        keryx_tools::SkillsRootDoctorKind::Ok
    );
}
