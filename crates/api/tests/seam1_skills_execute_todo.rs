//! Seam 1 — Skills, execute_code fence, todo + clarify.

use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
};
use keryx_domain::{MessageRole, Principal, RunOrigin};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use keryx_tools::{
    ClarifyQueue, ExecuteCodeTools, OperatorTools, SkillDraftStore, SkillsTools,
    TodoState, WorkspaceFsTools,
};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const PRINCIPAL: &str = "operator-main";

#[tokio::test]
async fn skills_list_view_and_gateway_draft_only() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("hello")).unwrap();
    std::fs::write(root.path().join("hello/SKILL.md"), "# Hello\nSay hi.").unwrap();
    let drafts = Arc::new(SkillDraftStore::new());
    let skills = Arc::new(SkillsTools::new(
        HashSet::from([
            "skills_list".into(),
            "skill_view".into(),
            "skill_draft".into(),
            "skill_manage".into(),
        ]),
        root.path().to_path_buf(),
        RunOrigin::gateway("telegram"),
        Arc::clone(&drafts),
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "skills_list".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    name: "skill_view".into(),
                    arguments: json!({ "name": "hello" }),
                },
                ToolCall {
                    name: "skill_draft".into(),
                    arguments: json!({ "name": "new", "content": "steps" }),
                },
                ToolCall {
                    name: "skill_manage".into(),
                    arguments: json!({ "name": "evil", "content": "nope" }),
                },
            ],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        skills,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "skills".into(),
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
    assert!(!root.path().join("evil/SKILL.md").exists());
    assert!(!drafts.list().is_empty());
    let t = store.get_transcript(session.id).await.unwrap();
    assert!(t.messages.iter().any(|m| m.content.contains("hello")));
    assert!(t.messages.iter().any(|m| {
        m.role == MessageRole::Tool
            && m.content.contains("skill_manage")
            && m.content.contains("denied")
    }));
}

#[tokio::test]
async fn execute_code_rpc_and_fence() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.txt"), "alpha").unwrap();
    let fs = Arc::new(WorkspaceFsTools::new(
        vec![root.path().to_path_buf()],
        HashSet::from(["read_file".into()]),
    ));
    let exec = Arc::new(ExecuteCodeTools::new(
        HashSet::from(["execute_code".into()]),
        RunOrigin::ControlPlane,
        fs,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "execute_code".into(),
                    arguments: json!({
                        "code": "tool read_file {\"path\":\"a.txt\"}\nprint ok"
                    }),
                },
                ToolCall {
                    name: "execute_code".into(),
                    arguments: json!({ "code": "std::process::Command::new(\"id\")" }),
                },
            ],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        exec,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run(principal, session.id, "code".into(), None, None)
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
        t.messages
            .iter()
            .any(|m| m.content.contains("read_file") || m.content.contains("alpha")),
        "{:?}",
        t.messages
    );
    assert!(
        t.messages
            .iter()
            .any(|m| m.content.contains("fence") || m.content.contains("banned")),
        "{:?}",
        t.messages
    );
}

#[tokio::test]
async fn execute_code_denied_for_gateway() {
    let exec = Arc::new(ExecuteCodeTools::new(
        HashSet::from(["execute_code".into()]),
        RunOrigin::gateway("telegram"),
        Arc::new(keryx_app::DenyAllTools),
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "execute_code".into(),
                arguments: json!({ "code": "print hi" }),
            }],
        ),
        ModelResponse::text("x"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        exec,
    ));
    let principal = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let session = control.create_session(principal.clone()).await.unwrap();
    let run = control
        .start_run_with_origin(
            principal,
            session.id,
            "g".into(),
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
    assert!(t.messages.iter().any(|m| m.content.contains("denied")));
}

#[tokio::test]
async fn todo_and_clarify_round_trip() {
    let todos = Arc::new(TodoState::new());
    let clarify = Arc::new(ClarifyQueue::new());
    let clarify_bg = Arc::clone(&clarify);
    // Answer clarify after a short delay (simulates operator/API).
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let pending = clarify_bg.list_pending();
        if let Some(p) = pending.first() {
            let _ = clarify_bg.answer(&p.id, "use blue".into());
        }
    });
    let tools = Arc::new(OperatorTools::new(
        HashSet::from(["todo".into(), "clarify".into()]),
        Arc::clone(&todos),
        Arc::clone(&clarify),
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "todo".into(),
                    arguments: json!({ "action": "add", "item": "pick color" }),
                },
                ToolCall {
                    name: "clarify".into(),
                    arguments: json!({ "question": "which color?" }),
                },
            ],
        ),
        ModelResponse::text("got it"),
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
        .start_run(principal, session.id, "todo".into(), None, None)
        .await
        .unwrap();
    for _ in 0..80 {
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
    assert!(todos.snapshot().contains("pick color"));
    let t = store.get_transcript(session.id).await.unwrap();
    assert!(
        t.messages
            .iter()
            .any(|m| m.content.contains("blue") || m.content.contains("answered")),
        "{:?}",
        t.messages
    );
}
