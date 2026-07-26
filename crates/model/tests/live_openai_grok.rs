//! L5 — opt-in live OpenAI and Grok verification (never a merge gate).
//!
//! Run only when:
//!   KERYX_LIVE_MODELS=1
//!   and OPENAI_API_KEY / XAI_API_KEY (or *_FILE) are set
//!
//!   KERYX_LIVE_MODELS=1 cargo test -p keryx-model --test live_openai_grok -- --ignored --nocapture

use keryx_app::{ModelProvider, ModelRequest};
use keryx_model::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use std::env;
use std::path::Path;

fn live_enabled() -> bool {
    matches!(
        env::var("KERYX_LIVE_MODELS").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn read_secret(env_key: &str, file_key: &str) -> Option<String> {
    if let Ok(v) = env::var(env_key) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Ok(path) = env::var(file_key) {
        if let Ok(contents) = std::fs::read_to_string(Path::new(&path)) {
            let trimmed = contents.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

#[tokio::test]
#[ignore = "opt-in live network: set KERYX_LIVE_MODELS=1 and API keys"]
async fn live_openai_completion() {
    if !live_enabled() {
        eprintln!("skip: KERYX_LIVE_MODELS not set");
        return;
    }
    let Some(api_key) = read_secret("OPENAI_API_KEY", "OPENAI_API_KEY_FILE") else {
        eprintln!("skip: OPENAI_API_KEY not set");
        return;
    };
    let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let mut cfg = OpenAiCompatibleConfig::openai(api_key, model);
    if let Ok(base) = env::var("OPENAI_BASE_URL") {
        cfg = cfg.with_base_url(base);
    }
    let provider = OpenAiCompatibleProvider::new(cfg).expect("openai client");
    let response = provider
        .complete(ModelRequest {
            goal: "Reply with exactly the word pong.".into(),
            transcript: vec![],
            provider: Some("openai".into()),
        })
        .await
        .expect("live openai completion");
    assert!(
        !response.content.is_empty(),
        "openai returned empty content"
    );
    eprintln!("openai live ok: {} chars", response.content.len());
}

#[tokio::test]
#[ignore = "opt-in live network: set KERYX_LIVE_MODELS=1 and API keys"]
async fn live_grok_completion() {
    if !live_enabled() {
        eprintln!("skip: KERYX_LIVE_MODELS not set");
        return;
    }
    let Some(api_key) = read_secret("XAI_API_KEY", "XAI_API_KEY_FILE") else {
        eprintln!("skip: XAI_API_KEY not set");
        return;
    };
    let model = env::var("XAI_MODEL").unwrap_or_else(|_| "grok-3".into());
    let mut cfg = OpenAiCompatibleConfig::grok(api_key, model);
    if let Ok(base) = env::var("XAI_BASE_URL") {
        cfg = cfg.with_base_url(base);
    }
    let provider = OpenAiCompatibleProvider::new(cfg).expect("grok client");
    let response = provider
        .complete(ModelRequest {
            goal: "Reply with exactly the word pong.".into(),
            transcript: vec![],
            provider: Some("grok".into()),
        })
        .await
        .expect("live grok completion");
    assert!(!response.content.is_empty(), "grok returned empty content");
    eprintln!("grok live ok: {} chars", response.content.len());
}
