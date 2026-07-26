//! Seam 2 — OpenAI / Grok Model provider contract against HTTP fixtures (no live network).

use keryx_app::{ModelProvider, ModelRequest};
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
    let openai = OpenAiCompatibleConfig::openai("k", "gpt-4o-mini");
    assert_eq!(openai.base_url, "https://api.openai.com/v1");
    assert_eq!(openai.provider_name, "openai");

    let grok = OpenAiCompatibleConfig::grok("k", "grok-3");
    assert_eq!(grok.base_url, "https://api.x.ai/v1");
    assert_eq!(grok.provider_name, "grok");
}
