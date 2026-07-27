//! L5 — opt-in live consumer / Codex session verification (never a merge gate).
//!
//!   KERYX_LIVE_MODELS=1 \
//!   CHATGPT_WEB_ACCESS_TOKEN=... (Codex) and/or CHATGPT_WEB_COOKIE=... \
//!   GROK_WEB_COOKIE=... \
//!   cargo test -p keryx-model --test live_consumer_web -- --ignored --nocapture
//!
//! See docs/deploy/consumer-web-sessions.md and ADR 0010.

use keryx_app::{ModelProvider, ModelRequest};
use keryx_model::{ChatGptCodexProvider, ChatGptWebProvider, GrokWebProvider};
use std::env;

fn live_enabled() -> bool {
    matches!(
        env::var("KERYX_LIVE_MODELS").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

#[tokio::test]
#[ignore = "opt-in live consumer network: set KERYX_LIVE_MODELS=1 and Codex access token"]
async fn live_openai_codex_completion() {
    if !live_enabled() {
        eprintln!("skip: KERYX_LIVE_MODELS not set");
        return;
    }
    let Some(provider) = ChatGptCodexProvider::from_env().expect("openai_codex config") else {
        eprintln!("skip: CHATGPT_WEB_ACCESS_TOKEN / CHATGPT_CODEX_ACCESS_TOKEN not set");
        return;
    };
    let response = provider
        .complete(ModelRequest {
            goal: "Reply with exactly the word pong.".into(),
            transcript: vec![],
            provider: Some("openai_codex".into()),
            model: None,
            tools: vec![],
        })
        .await
        .expect("live openai_codex completion");
    assert!(!response.content.is_empty());
    eprintln!("openai_codex live ok: {} chars", response.content.len());
}

#[tokio::test]
#[ignore = "opt-in live consumer network: set KERYX_LIVE_MODELS=1 and CHATGPT_WEB_* secrets"]
async fn live_openai_web_completion() {
    if !live_enabled() {
        eprintln!("skip: KERYX_LIVE_MODELS not set");
        return;
    }
    let Some(provider) = ChatGptWebProvider::from_env().expect("openai_web config") else {
        eprintln!("skip: CHATGPT_WEB_ACCESS_TOKEN / CHATGPT_WEB_COOKIE not set");
        return;
    };
    let response = provider
        .complete(ModelRequest {
            goal: "Reply with exactly the word pong.".into(),
            transcript: vec![],
            provider: Some("openai_web".into()),
            model: None,
            tools: vec![],
        })
        .await
        .expect("live openai_web completion");
    assert!(!response.content.is_empty());
    eprintln!("openai_web live ok: {} chars", response.content.len());
}

#[tokio::test]
#[ignore = "opt-in live consumer network: set KERYX_LIVE_MODELS=1 and GROK_WEB_* secrets"]
async fn live_grok_web_completion() {
    if !live_enabled() {
        eprintln!("skip: KERYX_LIVE_MODELS not set");
        return;
    }
    let Some(provider) = GrokWebProvider::from_env().expect("grok_web config") else {
        eprintln!("skip: GROK_WEB_COOKIE not set");
        return;
    };
    let response = provider
        .complete(ModelRequest {
            goal: "Reply with exactly the word pong.".into(),
            transcript: vec![],
            provider: Some("grok_web".into()),
            model: None,
            tools: vec![],
        })
        .await
        .expect("live grok_web completion");
    assert!(!response.content.is_empty());
    eprintln!("grok_web live ok: {} chars", response.content.len());
}
