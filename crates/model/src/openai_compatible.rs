use async_trait::async_trait;
use futures_util::StreamExt;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse, ToolCall, ToolSpec};
use keryx_domain::MessageRole;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Configuration for an OpenAI-compatible Chat Completions HTTP API.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    /// Base URL including `/v1` (e.g. `https://api.openai.com/v1`).
    pub base_url: String,
    /// Bearer API key from env/secret file (never browser session cookies).
    pub api_key: String,
    /// Default model id.
    pub model: String,
    /// Provider name for errors/logs (`openai`, `grok`).
    pub provider_name: String,
    /// When non-empty, only these model ids are accepted.
    pub allowed_models: Vec<String>,
    /// Optional reasoning effort (`low` \| `medium` \| `high`) for models that support it.
    pub reasoning_effort: Option<String>,
}

impl OpenAiCompatibleConfig {
    #[must_use]
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            provider_name: "openai".into(),
            allowed_models: Vec::new(),
            reasoning_effort: None,
        }
    }

    #[must_use]
    pub fn grok(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.x.ai/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            provider_name: "grok".into(),
            allowed_models: Vec::new(),
            reasoning_effort: None,
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    #[must_use]
    pub fn with_allowed_models(mut self, models: Vec<String>) -> Self {
        self.allowed_models = models;
        self
    }

    #[must_use]
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        let effort = effort.into();
        self.reasoning_effort = if effort.trim().is_empty() {
            None
        } else {
            Some(effort)
        };
        self
    }

    /// Resolve model: request override → config default, then optional allowlist.
    pub fn resolve_model(&self, override_model: Option<&str>) -> Result<String, ModelError> {
        let model = override_model
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(self.model.as_str())
            .to_string();
        if !self.allowed_models.is_empty() && !self.allowed_models.iter().any(|m| m == &model) {
            return Err(ModelError::new(format!(
                "{}: model '{model}' not in allowlist {:?}",
                self.provider_name, self.allowed_models
            )));
        }
        Ok(model)
    }
}

/// OpenAI function name charset: `^[a-zA-Z0-9_-]{1,64}$` — dots are rejected.
const OPENAI_TOOL_NAME_MAX: usize = 64;

/// Map internal canonical tool name → OpenAI-safe wire name.
///
/// MCP names use dots (`mcp.<server>.<tool>`); wire form replaces `.` with `__`
/// (e.g. `mcp.demo.echo` → `mcp__demo__echo`). Non-dotted names pass through.
/// Policy / Transcript / invoke always use canonical names.
#[must_use]
pub fn to_openai_tool_name(canonical: &str) -> String {
    if !canonical.contains('.') {
        return canonical.to_string();
    }
    let mut wire = canonical.replace('.', "__");
    if wire.len() > OPENAI_TOOL_NAME_MAX {
        wire.truncate(OPENAI_TOOL_NAME_MAX);
    }
    wire
}

/// Reverse wire-safe OpenAI tool name → internal canonical name.
///
/// Only reverses the MCP mapping (`mcp__…` → `mcp.…`); other names unchanged.
#[must_use]
pub fn from_openai_tool_name(wire: &str) -> String {
    if wire.starts_with("mcp__") {
        wire.replace("__", ".")
    } else {
        wire.to_string()
    }
}

/// Serialize a tool catalog entry to OpenAI-compatible function/tool wire format.
#[must_use]
pub fn tool_spec_to_openai(spec: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": to_openai_tool_name(&spec.name),
            "description": spec.description,
            "parameters": spec.parameters,
        }
    })
}

/// Parse structured tool_calls from a non-streaming OpenAI-style message payload.
///
/// Arguments may be a JSON object or a JSON string (OpenAI wire format).
/// Wire names are reverse-mapped to canonical internal names.
pub fn parse_tool_calls(value: &Value) -> Result<Vec<ToolCall>, ModelError> {
    let Some(arr) = value.as_array() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let wire_name = item
            .pointer("/function/name")
            .and_then(Value::as_str)
            .or_else(|| item.get("name").and_then(Value::as_str))
            .ok_or_else(|| ModelError::new("tool_call missing function.name"))?;
        let name = from_openai_tool_name(wire_name);
        let args_val = item
            .pointer("/function/arguments")
            .or_else(|| item.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let arguments = match args_val {
            Value::String(s) => {
                if s.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&s)
                        .map_err(|e| ModelError::new(format!("tool_call arguments JSON: {e}")))?
                }
            }
            other => other,
        };
        out.push(ToolCall { name, arguments });
    }
    Ok(out)
}

/// Shared OpenAI-compatible HTTP client used by OpenAI and Grok adapters.
pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, ModelError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| ModelError::new(e.to_string()))?;
        Ok(Self { config, client })
    }

    fn chat_url(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{base}/chat/completions")
    }

    fn build_messages(&self, request: &ModelRequest) -> Vec<Value> {
        let mut messages = Vec::new();
        for msg in &request.transcript {
            let role = match msg.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "user", // fold tool observations into user channel for v1
            };
            messages.push(json!({
                "role": role,
                "content": msg.content,
            }));
        }
        // Ensure the current goal is present if transcript is empty.
        if messages.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": request.goal,
            }));
        }
        messages
    }

    fn build_tools_body(tools: &[ToolSpec]) -> Option<Value> {
        if tools.is_empty() {
            return None;
        }
        Some(Value::Array(
            tools.iter().map(tool_spec_to_openai).collect(),
        ))
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let model = self.config.resolve_model(request.model.as_deref())?;
        let url = self.chat_url();
        let had_tools = !request.tools.is_empty();
        let mut body = json!({
            "model": model,
            "messages": self.build_messages(&request),
            "stream": true,
        });
        if let Some(tools) = Self::build_tools_body(&request.tools) {
            body["tools"] = tools;
            // Encourage the model to use tools when a catalog is present.
            body["tool_choice"] = json!("auto");
        }
        if let Some(effort) = &self.config.reasoning_effort {
            // OpenAI-style reasoning models accept top-level reasoning_effort.
            body["reasoning_effort"] = json!(effort);
        }

        let response = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ModelError::new(format!("{} request failed: {e}", self.config.provider_name))
            })?;

        let status = response.status();
        if !status.is_success() {
            // Do not echo full upstream bodies (may contain sensitive request context).
            return Err(ModelError::new(format!(
                "{}: upstream HTTP {status}",
                self.config.provider_name
            )));
        }

        let mut deltas = Vec::new();
        let mut content = String::new();
        // Accumulate streamed tool_call fragments by index.
        let mut tool_acc: BTreeMap<u32, StreamToolCallAcc> = BTreeMap::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ModelError::new(e.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].trim_end_matches('\r').to_string();
                buffer = buffer[idx + 1..].to_string();
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(choice) = parsed.choices.first() {
                            if let Some(delta) = choice.delta.content.as_ref() {
                                if !delta.is_empty() {
                                    deltas.push(delta.clone());
                                    content.push_str(delta);
                                }
                            }
                            if let Some(tcs) = choice.delta.tool_calls.as_ref() {
                                for tc in tcs {
                                    let idx = tc.index.unwrap_or(0);
                                    let entry = tool_acc.entry(idx).or_default();
                                    if let Some(id) = tc.id.as_ref() {
                                        entry.id = Some(id.clone());
                                    }
                                    if let Some(func) = tc.function.as_ref() {
                                        if let Some(name) = func.name.as_ref() {
                                            entry.name.push_str(name);
                                        }
                                        if let Some(args) = func.arguments.as_ref() {
                                            entry.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let tool_calls = finalize_streamed_tool_calls(&tool_acc)?;

        if content.is_empty() && deltas.is_empty() && tool_calls.is_empty() {
            return Err(ModelError::new(format!(
                "{} stream produced no content",
                self.config.provider_name
            )));
        }

        // Contract: when tools were requested and the model returned tool_calls-shaped
        // fragments that failed to yield names, treat as parse failure (Seam 2).
        if had_tools && !tool_acc.is_empty() && tool_calls.is_empty() {
            return Err(ModelError::new(format!(
                "{}: empty tool_calls after streaming fragments (contract failure)",
                self.config.provider_name
            )));
        }

        let tokens_used = content.chars().count() as u64;
        Ok(ModelResponse {
            content,
            deltas,
            tool_calls,
            tokens_used: tokens_used.max(1),
        })
    }
}

#[derive(Debug, Default)]
struct StreamToolCallAcc {
    id: Option<String>,
    name: String,
    arguments: String,
}

fn finalize_streamed_tool_calls(
    acc: &BTreeMap<u32, StreamToolCallAcc>,
) -> Result<Vec<ToolCall>, ModelError> {
    let mut out = Vec::new();
    for entry in acc.values() {
        if entry.name.is_empty() {
            continue;
        }
        let arguments = if entry.arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&entry.arguments).unwrap_or_else(|_| {
                // If partial JSON, wrap as raw string object for agent visibility.
                json!({ "_raw": entry.arguments })
            })
        };
        out.push(ToolCall {
            name: from_openai_tool_name(&entry.name),
            arguments,
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    index: Option<u32>,
    id: Option<String>,
    function: Option<StreamFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

/// OpenAI Model provider (API credentials only).
pub type OpenAiProvider = OpenAiCompatibleProvider;

/// Grok (xAI) Model provider reusing the OpenAI-compatible client shape.
pub type GrokProvider = OpenAiCompatibleProvider;

/// Construct a configured OpenAI provider.
pub fn openai_provider(
    api_key: impl Into<String>,
    model: impl Into<String>,
) -> Result<OpenAiProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig::openai(api_key, model))
}

/// Construct a configured Grok/xAI provider.
pub fn grok_provider(
    api_key: impl Into<String>,
    model: impl Into<String>,
) -> Result<GrokProvider, ModelError> {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig::grok(api_key, model))
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use keryx_app::ToolSpec;

    #[test]
    fn tool_spec_wire_shape_uses_wire_safe_mcp_names() {
        let spec = ToolSpec::new(
            "mcp.demo.echo",
            "echo",
            json!({"type":"object","properties":{}}),
        );
        let wire = tool_spec_to_openai(&spec);
        assert_eq!(wire["type"], "function");
        // OpenAI rejects dots in function names; wire form uses `__`.
        assert_eq!(wire["function"]["name"], "mcp__demo__echo");
        assert!(!wire["function"]["name"].as_str().unwrap().contains('.'));
        assert!(wire["function"]["parameters"].is_object());
        // Non-MCP names unchanged.
        let plain = tool_spec_to_openai(&ToolSpec::empty_params("read_file", "read"));
        assert_eq!(plain["function"]["name"], "read_file");
    }

    #[test]
    fn parse_tool_calls_reverse_maps_wire_to_canonical() {
        let v = json!([{
            "id": "call_1",
            "type": "function",
            "function": {
                "name": "mcp__demo__echo",
                "arguments": "{\"q\":\"hi\"}"
            }
        }]);
        let calls = parse_tool_calls(&v).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "mcp.demo.echo");
        assert_eq!(calls[0].arguments["q"], "hi");
    }

    #[test]
    fn openai_tool_name_round_trip() {
        assert_eq!(to_openai_tool_name("mcp.mail.search"), "mcp__mail__search");
        assert_eq!(
            from_openai_tool_name("mcp__mail__search"),
            "mcp.mail.search"
        );
        assert_eq!(to_openai_tool_name("read_file"), "read_file");
        assert_eq!(from_openai_tool_name("read_file"), "read_file");
        assert!(to_openai_tool_name("mcp.a.b").len() <= 64);
    }
}
