//! Seam 1 — browser/computer doubles, MCP mock peer, media stubs (no live vendors).

use keryx_app::{
    ControlPlane, ControlPlaneService, ModelResponse, RunLimits, SessionStore, ToolCall,
    ToolRuntime,
};
use keryx_domain::{MessageRole, Principal, RunOrigin};
use keryx_model::FakeModelProvider;
use keryx_storage::InMemorySessionStore;
use keryx_tools::{
    mock_registry_from_peer, BrowserTools, CompositeTools, ComputerUseTools, IsolatedBrowserState,
    IsolatedDesktop, McpClientRegistry, McpClientTools, McpServerExport, MediaConfig, MediaTools,
    MockMcpPeer,
};
use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const PRINCIPAL: &str = "op";

async fn wait_done(store: &InMemorySessionStore, id: keryx_domain::RunId) {
    for _ in 0..50 {
        if store.get_run(id).await.unwrap().unwrap().status.is_terminal() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn browser_isolated_navigate_and_snapshot() {
    let state = Arc::new(IsolatedBrowserState::new(HashSet::from([
        "example.com".into(),
    ])));
    let tools = Arc::new(BrowserTools::new(
        HashSet::from([
            "browser_navigate".into(),
            "browser_snapshot".into(),
            "browser_tabs".into(),
        ]),
        RunOrigin::ControlPlane,
        state,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "browser_navigate".into(),
                    arguments: json!({ "url": "https://example.com/" }),
                },
                ToolCall {
                    name: "browser_snapshot".into(),
                    arguments: json!({}),
                },
            ],
        ),
        ModelResponse::text("ok"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "browse".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(t.messages.iter().any(|m| m.content.contains("isolated")));
    assert!(t.messages.iter().any(|m| m.content.contains("example.com")));
}

#[tokio::test]
async fn computer_use_isolated_and_reduced_denied() {
    let desk = Arc::new(IsolatedDesktop::new());
    assert!(!desk.personal_attach_enabled());
    let tools = Arc::new(ComputerUseTools::new(
        HashSet::from(["computer_screenshot".into(), "computer_click".into()]),
        RunOrigin::ControlPlane,
        desk,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "computer_screenshot".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    name: "computer_click".into(),
                    arguments: json!({ "attach_personal_desktop": true }),
                },
            ],
        ),
        ModelResponse::text("done"),
    ]);
    // Note: computer_click will wait for Approval (high-blast) then deny personal attach.
    // First screenshot may also wait Approval — approve all pending in background...
    // Simpler: only screenshot without high_blast wait if we remove computer from high-blast
    // for screenshot. Currently computer_* requires approval for control_plane.
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    // Reduced origin path for deny
    let run = control
        .start_run_with_origin(
            p,
            s.id,
            "cu".into(),
            RunOrigin::gateway("telegram"),
            None,
            None,
        )
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| m.content.contains("denied") || m.content.contains("reduced")),
        "{:?}",
        t.messages
    );
}

#[tokio::test]
async fn mcp_mock_peer_and_auth_serve() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("echo", "pong"));
    let client = Arc::new(McpClientTools::new(
        HashSet::from(["mcp.demo.echo".into()]),
        Arc::clone(&peer),
        "mcp.demo.",
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({}),
            }],
        ),
        ModelResponse::text("ok"),
    ]);
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            client,
        )
        .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "mcp".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(t.messages.iter().any(|m| m.content.contains("pong")));

    let serve = McpServerExport::new(HashSet::from(["read_file".into()]), peer);
    assert!(serve
        .invoke_exported(false, "read_file", &json!({}))
        .is_err());
    assert!(serve
        .invoke_exported(true, "read_file", &json!({}))
        .unwrap()
        .contains("exported"));
}

#[tokio::test]
async fn media_vision_tts_image_gen_gating() {
    let tools = Arc::new(MediaTools::new(
        HashSet::from([
            "vision_describe".into(),
            "tts_synthesize".into(),
            "image_gen".into(),
        ]),
        MediaConfig {
            image_gen_api_key: None,
            tts_enabled: true,
        },
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "vision_describe".into(),
                    arguments: json!({ "source": "telegram_photo" }),
                },
                ToolCall {
                    name: "tts_synthesize".into(),
                    arguments: json!({ "text": "hello" }),
                },
                ToolCall {
                    name: "image_gen".into(),
                    arguments: json!({ "prompt": "cat", "api_key": "should-not-log" }),
                },
            ],
        ),
        ModelResponse::text("media"),
    ]);
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "media".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(t.messages.iter().any(|m| m.content.contains("vision")));
    assert!(t.messages.iter().any(|m| m.content.contains("tts") || m.content.contains("voice")));
    assert!(t.messages.iter().any(|m| {
        m.role == MessageRole::Tool
            && m.content.contains("image_gen")
            && (m.content.contains("denied") || m.content.contains("not registered"))
    }));
    // Secrets never in transcript body as raw key leakage from our summarizer path is ok to check events separately
    assert!(!t.messages.iter().any(|m| m.content.contains("should-not-log")));
}

/// Config-shaped mock registry → namespaced tools + successful control_plane invoke when Policy allows.
#[tokio::test]
async fn mcp_config_mock_register_and_control_plane_invoke() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("echo", "pong-cfg"));
    let reg = Arc::new(mock_registry_from_peer(
        "demo",
        Arc::clone(&peer),
        &["echo".into()],
        &[],
    ));
    assert!(reg.registered_names().contains("mcp.demo.echo"));
    assert!(reg
        .catalog()
        .iter()
        .any(|t| t.name == "mcp.demo.echo"));

    let tools = Arc::new(
        CompositeTools::new().with(reg.registered_names(), Arc::clone(&reg) as Arc<dyn ToolRuntime>),
    );
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({}),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            tools,
        )
        .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "mcp cfg".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| m.content.contains("pong-cfg")),
        "{:?}",
        t.messages
    );
}

/// Connect ≠ allow: registered MCP tool not on Policy is denied.
#[tokio::test]
async fn mcp_connect_not_allow_denies_without_policy() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("search", "hits"));
    // Registered but policy_allowlist empty → control_plane default has no mcp.gmail.search
    let reg = Arc::new(McpClientRegistry::from_mock("gmail", peer, &[], &[]));
    let tools = Arc::new(CompositeTools::new().with(
        reg.registered_names(),
        Arc::clone(&reg) as Arc<dyn ToolRuntime>,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.gmail.search".into(),
                arguments: json!({ "q": "inbox" }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    // No with_control_plane_extra_tools → Policy denies mcp.gmail.search
    let control = Arc::new(ControlPlane::with_tools(
        Arc::clone(&store),
        Arc::new(model),
        RunLimits::default(),
        tools,
    ));
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "search".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && m.content.contains("mcp.gmail.search")
                && (m.content.contains("denied") || m.content.contains("Policy"))
        }),
        "{:?}",
        t.messages
    );
}

/// Gateway/schedule reduced origin denies MCP by default even if registered + control_plane extras set.
#[tokio::test]
async fn mcp_reduced_origin_denies_by_default() {
    for origin in [
        RunOrigin::gateway("telegram"),
        RunOrigin::Schedule,
    ] {
        let peer = Arc::new(MockMcpPeer::default().with_tool("echo", "pong"));
        let reg = Arc::new(mock_registry_from_peer(
            "demo",
            peer,
            &["echo".into()],
            &[],
        ));
        let tools = Arc::new(CompositeTools::new().with(
            reg.registered_names(),
            Arc::clone(&reg) as Arc<dyn ToolRuntime>,
        ));
        let store = Arc::new(InMemorySessionStore::new());
        let model = FakeModelProvider::with_script(vec![
            ModelResponse::with_tool_calls(
                "",
                vec![ToolCall {
                    name: "mcp.demo.echo".into(),
                    arguments: json!({}),
                }],
            ),
            ModelResponse::text("done"),
        ]);
        let control = Arc::new(
            ControlPlane::with_tools(
                Arc::clone(&store),
                Arc::new(model),
                RunLimits::default(),
                tools,
            )
            .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
        );
        let p = Principal {
            id: keryx_domain::PrincipalId::new(PRINCIPAL),
        };
        let s = control.create_session(p.clone()).await.unwrap();
        let run = control
            .start_run_with_origin(
                p,
                s.id,
                "reduced mcp".into(),
                origin.clone(),
                None,
                None,
            )
            .await
            .unwrap();
        wait_done(&store, run.id).await;
        let t = store.get_transcript(s.id).await.unwrap();
        assert!(
            t.messages.iter().any(|m| {
                m.role == MessageRole::Tool
                    && m.content.contains("mcp.demo.echo")
                    && (m.content.contains("denied") || m.content.contains("Policy"))
            }),
            "origin={origin:?} messages={:?}",
            t.messages
        );
    }
}

/// High-blast config → Approval path: approve succeeds; deny fails closed.
#[tokio::test]
async fn mcp_high_blast_approval_approve_and_deny() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("send", "sent-ok"));
    let reg = Arc::new(mock_registry_from_peer(
        "mail",
        peer,
        &["send".into()],
        &["send".into()],
    ));
    let tools = Arc::new(CompositeTools::new().with(
        reg.registered_names(),
        Arc::clone(&reg) as Arc<dyn ToolRuntime>,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.mail.send".into(),
                arguments: json!({ "to": "a@b.c", "token": "secret-token-value" }),
            }],
        ),
        ModelResponse::text("after-approve"),
    ]);
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            tools,
        )
        .with_control_plane_extra_tools(["mcp.mail.send".into()])
        .with_high_blast_tools(["mcp.mail.send".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p.clone(), s.id, "send mail".into(), None, None)
        .await
        .unwrap();

    // Wait for pending Approval, then approve.
    let approval_id = {
        let mut found = None;
        for _ in 0..100 {
            let list = control.list_approvals(true).await.unwrap();
            if let Some(a) = list.first() {
                found = Some(a.id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        found.expect("pending MCP high-blast Approval")
    };
    let approval = control.get_approval(approval_id).await.unwrap();
    assert!(
        approval.summary.contains("[REDACTED]") || !approval.summary.contains("secret-token-value"),
        "Approval summary must redact token: {}",
        approval.summary
    );
    assert!(!approval.summary.contains("secret-token-value"));
    control.approve(p.clone(), approval_id).await.unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| m.content.contains("sent-ok")),
        "{:?}",
        t.messages
    );

    // Deny path fails closed.
    let peer2 = Arc::new(MockMcpPeer::default().with_tool("send", "should-not"));
    let reg2 = Arc::new(mock_registry_from_peer(
        "mail",
        peer2,
        &["send".into()],
        &["send".into()],
    ));
    let tools2 = Arc::new(CompositeTools::new().with(
        reg2.registered_names(),
        Arc::clone(&reg2) as Arc<dyn ToolRuntime>,
    ));
    let store2 = Arc::new(InMemorySessionStore::new());
    let model2 = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.mail.send".into(),
                arguments: json!({ "token": "another-secret" }),
            }],
        ),
        ModelResponse::text("after-deny"),
    ]);
    let control2 = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store2),
            Arc::new(model2),
            RunLimits::default(),
            tools2,
        )
        .with_control_plane_extra_tools(["mcp.mail.send".into()])
        .with_high_blast_tools(["mcp.mail.send".into()]),
    );
    let s2 = control2.create_session(p.clone()).await.unwrap();
    let run2 = control2
        .start_run(p.clone(), s2.id, "send deny".into(), None, None)
        .await
        .unwrap();
    let deny_id = {
        let mut found = None;
        for _ in 0..100 {
            let list = control2.list_approvals(true).await.unwrap();
            if let Some(a) = list.first() {
                found = Some(a.id);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        found.expect("pending deny Approval")
    };
    control2.deny(p, deny_id).await.unwrap();
    wait_done(&store2, run2.id).await;
    let t2 = store2.get_transcript(s2.id).await.unwrap();
    assert!(
        t2.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("denied") || m.content.contains("Approval"))
        }),
        "{:?}",
        t2.messages
    );
    assert!(!t2.messages.iter().any(|m| m.content.contains("should-not")));
}

/// Disconnect fails subsequent invoke closed.
#[tokio::test]
async fn mcp_disconnect_fails_invoke_closed() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("echo", "pong"));
    let reg = Arc::new(mock_registry_from_peer(
        "demo",
        Arc::clone(&peer),
        &["echo".into()],
        &[],
    ));
    reg.disconnect_server("demo");
    let err = reg
        .invoke(ToolCall {
            name: "mcp.demo.echo".into(),
            arguments: json!({}),
        })
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("fail closed") || err.to_string().contains("disconnected"),
        "{err}"
    );

    let tools = Arc::new(CompositeTools::new().with(
        HashSet::from(["mcp.demo.echo".into()]),
        Arc::clone(&reg) as Arc<dyn ToolRuntime>,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({}),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            tools,
        )
        .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "disc".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && (m.content.contains("fail closed")
                    || m.content.contains("disconnected")
                    || m.content.contains("error"))
        }),
        "{:?}",
        t.messages
    );
}

/// Secrets absent from tool event summaries (token-like keys → [REDACTED]),
/// including camelCase `apiKey` after key normalization.
#[tokio::test]
async fn mcp_secrets_redacted_in_tool_events() {
    let peer = Arc::new(MockMcpPeer::default().with_tool("echo", "ok"));
    let client = Arc::new(McpClientTools::for_server(
        "demo",
        HashSet::from(["mcp.demo.echo".into()]),
        peer,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![ToolCall {
                name: "mcp.demo.echo".into(),
                arguments: json!({
                    "message": "hello",
                    "api_token": "super-secret-mcp-token",
                    "password": "hunter2",
                    "apiKey": "camelCase-api-key-value",
                    "nested": {
                        "authorization": "Bearer nest-secret",
                        "ok_field": "visible"
                    }
                }),
            }],
        ),
        ModelResponse::text("done"),
    ]);
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::new(model),
            RunLimits::default(),
            client,
        )
        .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "redact".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;

    let (history, _) = control.subscribe_run_events(run.id).await.unwrap();
    let joined: String = history
        .iter()
        .map(|e| format!("{:?}", e.kind))
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        joined.contains("[REDACTED]"),
        "expected redacted secrets in events: {joined}"
    );
    assert!(
        !joined.contains("super-secret-mcp-token"),
        "token leaked: {joined}"
    );
    assert!(!joined.contains("hunter2"), "password leaked: {joined}");
    assert!(
        !joined.contains("camelCase-api-key-value"),
        "apiKey camelCase leaked: {joined}"
    );
    assert!(
        !joined.contains("nest-secret") && !joined.contains("Bearer nest"),
        "nested authorization leaked: {joined}"
    );
}

/// Catalog ∩ Policy: model tool_calls for allowed MCP succeeds; Policy deny still records failure.
/// Also asserts FakeModelProvider observed allowlisted MCP names only (not non-allowlisted).
#[tokio::test]
async fn mcp_catalog_policy_intersection_via_fake_model() {
    let peer = Arc::new(
        MockMcpPeer::default()
            .with_tool("echo", "pong")
            .with_tool("send", "sent"),
    );
    let reg = Arc::new(mock_registry_from_peer(
        "demo",
        peer,
        &["echo".into()], // only echo on control_plane extras
        &[],
    ));
    // Both tools registered in runtime catalog.
    assert!(reg.registered_names().contains("mcp.demo.echo"));
    assert!(reg.registered_names().contains("mcp.demo.send"));

    let tools = Arc::new(CompositeTools::new().with(
        reg.registered_names(),
        Arc::clone(&reg) as Arc<dyn ToolRuntime>,
    ));
    let store = Arc::new(InMemorySessionStore::new());
    let model = Arc::new(FakeModelProvider::with_script(vec![
        ModelResponse::with_tool_calls(
            "",
            vec![
                ToolCall {
                    name: "mcp.demo.echo".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    name: "mcp.demo.send".into(),
                    arguments: json!({}),
                },
            ],
        ),
        ModelResponse::text("done"),
    ]));
    let control = Arc::new(
        ControlPlane::with_tools(
            Arc::clone(&store),
            Arc::clone(&model),
            RunLimits::default(),
            tools,
        )
        .with_control_plane_extra_tools(["mcp.demo.echo".into()]),
    );
    let p = Principal {
        id: keryx_domain::PrincipalId::new(PRINCIPAL),
    };
    let s = control.create_session(p.clone()).await.unwrap();
    let run = control
        .start_run(p, s.id, "catalog".into(), None, None)
        .await
        .unwrap();
    wait_done(&store, run.id).await;
    let t = store.get_transcript(s.id).await.unwrap();
    assert!(
        t.messages.iter().any(|m| m.content.contains("pong")),
        "allowlisted echo should succeed: {:?}",
        t.messages
    );
    assert!(
        t.messages.iter().any(|m| {
            m.role == MessageRole::Tool
                && m.content.contains("mcp.demo.send")
                && (m.content.contains("denied") || m.content.contains("Policy"))
        }),
        "non-allowlisted send should fail closed: {:?}",
        t.messages
    );

    // Catalog offered to the model must be registered ∩ Policy.
    let offered = model.last_tool_names();
    assert!(
        offered.iter().any(|n| n == "mcp.demo.echo"),
        "allowlisted MCP must appear in model catalog: {offered:?}"
    );
    assert!(
        !offered.iter().any(|n| n == "mcp.demo.send"),
        "non-allowlisted MCP must not appear in model catalog: {offered:?}"
    );
}
