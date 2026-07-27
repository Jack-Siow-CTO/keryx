//! Seam 2 — OpenAI / Grok Model provider contract against HTTP fixtures (no live network).

use keryx_app::{ModelProvider, ModelRequest, ToolSpec};
use keryx_model::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn stream_body(chunks: &[&str]) -> String {
    let mut out = String::new();
    for c in chunks {
        let payload = json!({
            "choices": [{ "delta": { "content": c } }]
        });
        out.push_str("data: ");
        out.push_str(&payload.to_string());
        out.push_str("\n\n");
    }
    out.push_str("data: [DONE]\n\n");
    out
}

#[tokio::test]
async fn openai_fixture_shapes_request_and_parses_stream_deltas() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-openai-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["Hel", "lo", "!"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("test-openai-key", "gpt-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "say hi".into(),
            transcript: vec![],
            provider: Some("openai".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    assert_eq!(response.content, "Hello!");
    assert_eq!(response.deltas, vec!["Hel", "lo", "!"]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["stream"], true);
    assert!(!body["messages"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn openai_per_run_model_override() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["ok"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("test-openai-key", "gpt-default")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    provider
        .complete(ModelRequest {
            goal: "hi".into(),
            transcript: vec![],
            provider: Some("openai".into()),
            model: Some("gpt-override".into()),
            tools: vec![],
        })
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "gpt-override");
}

#[tokio::test]
async fn openai_fixture_maps_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("bad", "gpt-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    let err = provider
        .complete(ModelRequest {
            goal: "x".into(),
            transcript: vec![],
            provider: None,
            model: None,
            tools: vec![],
        })
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("HTTP 401"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn grok_fixture_uses_xai_style_base_url_model_and_auth() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer xai-test-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["grok", "-ok"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::grok("xai-test-key", "grok-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "ping".into(),
            transcript: vec![],
            provider: Some("grok".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    assert_eq!(response.content, "grok-ok");
    assert_eq!(response.deltas, vec!["grok", "-ok"]);

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "grok-test");
}

#[tokio::test]
async fn default_config_targets_are_documented_not_hit_in_ci() {
    let openai = OpenAiCompatibleConfig::openai("k", "gpt-5.6-sol").with_reasoning_effort("low");
    assert_eq!(openai.base_url, "https://api.openai.com/v1");
    assert_eq!(openai.provider_name, "openai");
    assert_eq!(openai.model, "gpt-5.6-sol");
    assert_eq!(openai.reasoning_effort.as_deref(), Some("low"));

    let grok = OpenAiCompatibleConfig::grok("k", "grok-4.5").with_reasoning_effort("medium");
    assert_eq!(grok.base_url, "https://api.x.ai/v1");
    assert_eq!(grok.provider_name, "grok");
    assert_eq!(grok.model, "grok-4.5");
    assert_eq!(grok.reasoning_effort.as_deref(), Some("medium"));
}

#[tokio::test]
async fn openai_compatible_sends_reasoning_effort_when_configured() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["ok"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::grok("xai-key", "grok-4.5")
            .with_base_url(format!("{}/v1", server.uri()))
            .with_reasoning_effort("medium"),
    )
    .unwrap();

    provider
        .complete(ModelRequest {
            goal: "hi".into(),
            transcript: vec![],
            provider: Some("grok".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "grok-4.5");
    assert_eq!(body["reasoning_effort"], "medium");
}

fn stream_tool_calls_body() -> String {
    // Streamed tool_calls fragments (OpenAI-style) assembled by the adapter.
    let chunks = [
        json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "function": { "name": "mcp__demo__echo", "arguments": "" }
                    }]
                }
            }]
        }),
        json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": { "arguments": "{\"msg\":" }
                    }]
                }
            }]
        }),
        json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": { "arguments": "\"hi\"}" }
                    }]
                }
            }]
        }),
    ];
    let mut out = String::new();
    for c in chunks {
        out.push_str("data: ");
        out.push_str(&c.to_string());
        out.push_str("\n\n");
    }
    out.push_str("data: [DONE]\n\n");
    out
}

#[tokio::test]
async fn openai_compatible_sends_tools_catalog_when_present() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["ok"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("test-openai-key", "gpt-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    let schema = json!({
        "type": "object",
        "properties": { "q": { "type": "string" } },
        "required": ["q"]
    });
    provider
        .complete(
            ModelRequest::new("use tools").with_tools(vec![
                ToolSpec::new("mcp.mail.search", "search mail", schema.clone()),
                ToolSpec::empty_params("read_file", "read workspace file"),
            ]),
        )
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    let tools = body["tools"].as_array().expect("tools array on wire");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["type"], "function");
    // Wire-safe: dots replaced with `__` (OpenAI function name charset).
    assert_eq!(tools[0]["function"]["name"], "mcp__mail__search");
    assert!(
        !tools[0]["function"]["name"]
            .as_str()
            .unwrap()
            .contains('.'),
        "wire tool name must not contain dots"
    );
    assert_eq!(tools[0]["function"]["description"], "search mail");
    assert_eq!(tools[0]["function"]["parameters"], schema);
    assert_eq!(tools[1]["function"]["name"], "read_file");
    assert_eq!(body["tool_choice"], "auto");
}

#[tokio::test]
async fn openai_compatible_omits_tools_when_catalog_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_body(&["ok"])),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("k", "gpt-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    provider
        .complete(ModelRequest {
            goal: "x".into(),
            transcript: vec![],
            provider: None,
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
}

#[tokio::test]
async fn openai_compatible_assembles_streamed_tool_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream_tool_calls_body()),
        )
        .mount(&server)
        .await;

    let provider = OpenAiCompatibleProvider::new(
        OpenAiCompatibleConfig::openai("test-openai-key", "gpt-test")
            .with_base_url(format!("{}/v1", server.uri())),
    )
    .unwrap();

    let response = provider
        .complete(
            ModelRequest::new("echo").with_tools(vec![ToolSpec::empty_params(
                "mcp.demo.echo",
                "echo",
            )]),
        )
        .await
        .unwrap();

    assert_eq!(response.tool_calls.len(), 1);
    // Parsed ToolCall uses canonical internal name (reverse-mapped from wire).
    assert_eq!(response.tool_calls[0].name, "mcp.demo.echo");
    assert_eq!(response.tool_calls[0].arguments["msg"], "hi");
}
