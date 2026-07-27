//! Media: vision intake hooks, TTS stub, pluggable image gen (credentials gated).

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use serde_json::Value;
use std::collections::HashSet;

/// Optional image generation when API key present.
#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub image_gen_api_key: Option<String>,
    pub tts_enabled: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            image_gen_api_key: std::env::var("KERYX_IMAGE_GEN_API_KEY").ok().filter(|s| !s.is_empty()),
            tts_enabled: std::env::var("KERYX_TTS_ENABLED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}

pub struct MediaTools {
    allowed: HashSet<String>,
    config: MediaConfig,
}

impl MediaTools {
    #[must_use]
    pub fn new(allowed: HashSet<String>, config: MediaConfig) -> Self {
        Self { allowed, config }
    }

    /// Tool names that should register given credentials.
    #[must_use]
    pub fn registerable_tools(config: &MediaConfig) -> HashSet<String> {
        let mut s = HashSet::from([
            "vision_describe".into(),
            "tts_synthesize".into(),
        ]);
        if config.image_gen_api_key.is_some() {
            s.insert("image_gen".into());
        }
        s
    }
}

#[async_trait]
impl ToolRuntime for MediaTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "vision_describe" => {
                let source = call
                    .arguments
                    .get("source")
                    .and_then(Value::as_str)
                    .unwrap_or("image");
                // Never log secrets; only metadata.
                Ok(ToolResult {
                    content: format!(
                        "vision stub: accepted {source} for vision-capable model path (no live vendor)"
                    ),
                    summary: format!("vision_describe source={source}"),
                })
            }
            "tts_synthesize" => {
                if !self.config.tts_enabled {
                    return Err(ToolError::Failed(
                        "tts not configured (set KERYX_TTS_ENABLED=1)".into(),
                    ));
                }
                let text = call
                    .arguments
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                Ok(ToolResult {
                    content: format!("tts stub voice-note for {} chars (telegram-first path)", text.len()),
                    summary: format!("tts_synthesize chars={}", text.len()),
                })
            }
            "image_gen" => {
                if self.config.image_gen_api_key.is_none() {
                    return Err(ToolError::Denied(
                        "image_gen not registered without credentials".into(),
                    ));
                }
                let prompt = call
                    .arguments
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // Do not echo or log API key.
                Ok(ToolResult {
                    content: format!("image_gen stub ok prompt_chars={}", prompt.len()),
                    summary: "image_gen stub".into(),
                })
            }
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}
