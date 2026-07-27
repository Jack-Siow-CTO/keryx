use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse, ToolCall};
use serde_json::json;
use std::sync::Mutex;
use std::time::Duration;

/// Scripted Model provider for Seam 1 control-plane tests (no live network).
#[derive(Debug)]
pub struct FakeModelProvider {
    fixed_content: Option<String>,
    deltas: Option<Vec<String>>,
    tool_calls: Option<Vec<ToolCall>>,
    delay: Option<Duration>,
    script: Mutex<Vec<ModelResponse>>,
    /// Last tool names offered in a `complete` catalog (`request.tools`), for Policy ∩ catalog tests.
    last_tools: Mutex<Vec<String>>,
}

impl FakeModelProvider {
    fn empty_last_tools() -> Mutex<Vec<String>> {
        Mutex::new(Vec::new())
    }

    #[must_use]
    pub fn greeting() -> Self {
        Self {
            fixed_content: None,
            deltas: None,
            tool_calls: None,
            delay: None,
            script: Mutex::new(Vec::new()),
            last_tools: Self::empty_last_tools(),
        }
    }

    #[must_use]
    pub fn with_fixed_content(content: impl Into<String>) -> Self {
        Self {
            fixed_content: Some(content.into()),
            deltas: None,
            tool_calls: None,
            delay: None,
            script: Mutex::new(Vec::new()),
            last_tools: Self::empty_last_tools(),
        }
    }

    #[must_use]
    pub fn with_deltas(deltas: Vec<impl Into<String>>) -> Self {
        let deltas: Vec<String> = deltas.into_iter().map(Into::into).collect();
        Self {
            fixed_content: Some(deltas.concat()),
            deltas: Some(deltas),
            tool_calls: None,
            delay: None,
            script: Mutex::new(Vec::new()),
            last_tools: Self::empty_last_tools(),
        }
    }

    /// Delay before returning (for cancel / time-budget tests).
    #[must_use]
    pub fn with_delay(delay: Duration, content: impl Into<String>) -> Self {
        Self {
            fixed_content: Some(content.into()),
            deltas: None,
            tool_calls: None,
            delay: Some(delay),
            script: Mutex::new(Vec::new()),
            last_tools: Self::empty_last_tools(),
        }
    }

    /// Request tool calls so tool-call budgets can be exercised without real tools.
    #[must_use]
    pub fn with_tool_calls(content: impl Into<String>, tool_names: Vec<impl Into<String>>) -> Self {
        let tool_calls = tool_names
            .into_iter()
            .map(|n| ToolCall {
                name: n.into(),
                arguments: json!({}),
            })
            .collect();
        Self {
            fixed_content: Some(content.into()),
            deltas: None,
            tool_calls: Some(tool_calls),
            delay: None,
            script: Mutex::new(Vec::new()),
            last_tools: Self::empty_last_tools(),
        }
    }

    #[must_use]
    pub fn with_script(responses: Vec<ModelResponse>) -> Self {
        Self {
            fixed_content: None,
            deltas: None,
            tool_calls: None,
            delay: None,
            script: Mutex::new(responses),
            last_tools: Self::empty_last_tools(),
        }
    }

    /// Tool names from the most recent `complete` request catalog (canonical internal names).
    #[must_use]
    pub fn last_tool_names(&self) -> Vec<String> {
        self.last_tools
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn record_tools(&self, request: &ModelRequest) {
        if let Ok(mut guard) = self.last_tools.lock() {
            *guard = request.tools.iter().map(|t| t.name.clone()).collect();
        }
    }
}

#[async_trait]
impl ModelProvider for FakeModelProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        self.record_tools(&request);

        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        {
            let mut script = self
                .script
                .lock()
                .map_err(|e| ModelError::new(e.to_string()))?;
            if !script.is_empty() {
                return Ok(script.remove(0));
            }
        }

        if let Some(content) = &self.fixed_content {
            let mut response = ModelResponse {
                content: content.clone(),
                deltas: self.deltas.clone().unwrap_or_default(),
                tool_calls: self.tool_calls.clone().unwrap_or_default(),
                tokens_used: content.chars().count() as u64,
            };
            if response.tokens_used == 0 {
                response.tokens_used = 1;
            }
            return Ok(response);
        }

        // Default greeting includes transcript length so multi-turn Seam 1 tests can observe continuity.
        Ok(ModelResponse::text(format!(
            "fake-model: goal={} transcript_msgs={}",
            request.goal,
            request.transcript.len()
        )))
    }
}
