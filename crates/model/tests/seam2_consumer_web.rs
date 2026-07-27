//! Seam 2 — consumer web session providers (fixtures only; no live network).

use keryx_app::{ModelProvider, ModelRequest};
use keryx_model::{
    ChatGptCodexProvider, ChatGptWebProvider, ConsumerWebAuth, ConsumerWebConfig, GrokWebProvider,
};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn chatgpt_sse() -> String {
    "\
data: {\"message\":{\"content\":{\"parts\":[\"Hel\"]}}}\n\n\
data: {\"message\":{\"content\":{\"parts\":[\"Hello\"]}}}\n\n\
data: [DONE]\n\n"
        .to_string()
}

fn grok_sse() -> String {
    "\
data: {\"content\":\"grok\"}\n\n\
data: {\"content\":\"-web\"}\n\n\
data: [DONE]\n\n"
        .to_string()
}

#[tokio::test]
async fn openai_web_sends_bearer_and_cookie_and_parses_sse() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/backend-api/conversation"))
        .and(header("authorization", "Bearer chatgpt-access-token"))
        .and(header("cookie", "session=abc"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(chatgpt_sse()),
        )
        .mount(&server)
        .await;

    let provider = ChatGptWebProvider::new(ConsumerWebConfig {
        provider_name: "openai_web".into(),
        base_url: server.uri(),
        path: "/backend-api/conversation".into(),
        model: "auto".into(),
        auth: ConsumerWebAuth {
            cookie_header: Some("session=abc".into()),
            bearer_token: Some("chatgpt-access-token".into()),
            extra_headers: Default::default(),
        },
        user_agent: "keryx-test".into(),
        allowed_models: Vec::new(),
    })
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "hi".into(),
            transcript: vec![],
            provider: Some("openai_web".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    assert_eq!(response.content, "Hello");
    assert_eq!(response.deltas, vec!["Hel".to_string(), "lo".to_string()]);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["action"], "next");
    assert!(!body["messages"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn openai_web_maps_401_without_echoing_secrets() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/backend-api/conversation"))
        .respond_with(ResponseTemplate::new(401).set_body_string("nope session=abc"))
        .mount(&server)
        .await;

    let secret = "session=abc-super-secret";
    let provider = ChatGptWebProvider::new(ConsumerWebConfig {
        provider_name: "openai_web".into(),
        base_url: server.uri(),
        path: "/backend-api/conversation".into(),
        model: "auto".into(),
        auth: ConsumerWebAuth {
            cookie_header: Some(secret.into()),
            bearer_token: None,
            extra_headers: Default::default(),
        },
        user_agent: "keryx-test".into(),
        allowed_models: Vec::new(),
    })
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
    let msg = err.to_string();
    assert!(msg.contains("session expired or rejected"), "{msg}");
    assert!(!msg.contains(secret), "secret leaked: {msg}");
}

#[tokio::test]
async fn grok_web_sends_cookie_and_extra_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/app-chat/conversations/new"))
        .and(header("cookie", "sso=xyz"))
        .and(header("x-challenge", "chal"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(grok_sse()),
        )
        .mount(&server)
        .await;

    let mut extra = std::collections::HashMap::new();
    extra.insert("x-challenge".into(), "chal".into());

    let provider = GrokWebProvider::new(ConsumerWebConfig {
        provider_name: "grok_web".into(),
        base_url: server.uri(),
        path: "/rest/app-chat/conversations/new".into(),
        model: "grok".into(),
        auth: ConsumerWebAuth {
            cookie_header: Some("sso=xyz".into()),
            bearer_token: None,
            extra_headers: extra,
        },
        user_agent: "keryx-test".into(),
        allowed_models: Vec::new(),
    })
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "ping".into(),
            transcript: vec![],
            provider: Some("grok_web".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    assert_eq!(response.content, "grok-web");
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["stream"], true);
    assert!(body["message"].as_str().unwrap().contains("ping"));
}

#[tokio::test]
async fn openai_codex_sends_responses_body_and_parses_output_text_delta() {
    let server = MockServer::start().await;
    let sse = "\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"sub\"}\n\n\
event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"-ok\"}\n\n\
event: response.completed\n\
data: {\"type\":\"response.completed\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/backend-api/codex/responses"))
        .and(header("authorization", "Bearer codex-access-token"))
        .and(header("chatgpt-account-id", "acct-xyz"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&server)
        .await;

    let mut extra = std::collections::HashMap::new();
    extra.insert("chatgpt-account-id".into(), "acct-xyz".into());
    extra.insert("openai-beta".into(), "responses=experimental".into());

    let provider = ChatGptCodexProvider::new_with_reasoning(
        ConsumerWebConfig {
            provider_name: "openai_codex".into(),
            base_url: server.uri(),
            path: "/backend-api/codex/responses".into(),
            model: "gpt-5.6-sol".into(),
            auth: ConsumerWebAuth {
                cookie_header: None,
                bearer_token: Some("codex-access-token".into()),
                extra_headers: extra,
            },
            user_agent: "keryx-test".into(),
        allowed_models: Vec::new(),
        },
        Some("low".into()),
    )
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "ping".into(),
            transcript: vec![],
            provider: Some("openai_codex".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    assert_eq!(response.content, "sub-ok");
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "gpt-5.6-sol");
    assert_eq!(body["stream"], true);
    assert_eq!(body["store"], false);
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["input"][0]["role"], "user");
}

#[tokio::test]
async fn grok_web_defaults_include_reasoning_medium_when_set() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/app-chat/conversations/new"))
        .and(header("cookie", "sso=1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(grok_sse()),
        )
        .mount(&server)
        .await;

    let provider = GrokWebProvider::new_with_reasoning(
        ConsumerWebConfig {
            provider_name: "grok_web".into(),
            base_url: server.uri(),
            path: "/rest/app-chat/conversations/new".into(),
            model: "grok-4.5".into(),
            auth: ConsumerWebAuth {
                cookie_header: Some("sso=1".into()),
                bearer_token: None,
                extra_headers: Default::default(),
            },
            user_agent: "keryx-test".into(),
            allowed_models: Vec::new(),
        },
        Some("medium".into()),
    )
    .unwrap();

    provider
        .complete(ModelRequest {
            goal: "ping".into(),
            transcript: vec![],
            provider: Some("grok_web".into()),
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();

    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["model"], "grok-4.5");
    assert_eq!(body["reasoningEffort"], "medium");
}

#[tokio::test]
async fn grok_web_json_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rest/app-chat/conversations/new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": { "message": "plain-json-answer" }
        })))
        .mount(&server)
        .await;

    let provider = GrokWebProvider::new(ConsumerWebConfig {
        provider_name: "grok_web".into(),
        base_url: server.uri(),
        path: "/rest/app-chat/conversations/new".into(),
        model: "grok".into(),
        auth: ConsumerWebAuth {
            cookie_header: Some("sso=1".into()),
            bearer_token: None,
            extra_headers: Default::default(),
        },
        user_agent: "keryx-test".into(),
        allowed_models: Vec::new(),
    })
    .unwrap();

    let response = provider
        .complete(ModelRequest {
            goal: "q".into(),
            transcript: vec![],
            provider: None,
            model: None,
            tools: vec![],
        })
        .await
        .unwrap();
    assert_eq!(response.content, "plain-json-answer");
}
