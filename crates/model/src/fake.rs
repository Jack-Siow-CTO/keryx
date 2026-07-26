use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use std::sync::Mutex;

/// Scripted Model provider for Seam 1 control-plane tests (no live network).
#[derive(Debug)]
pub struct FakeModelProvider {
    /// When set, every completion returns this full content (no deltas unless `deltas` set).
    fixed_content: Option<String>,
    /// Token/text deltas for SSE; when set, `content` is their concatenation unless fixed_content set.
    deltas: Option<Vec<String>>,
    /// Optional scripted full responses consumed FIFO.
    script: Mutex<Vec<ModelResponse>>,
}

impl FakeModelProvider {
    /// Always answer with a deterministic greeting derived from the goal.
    #[must_use]
    pub fn greeting() -> Self {
        Self {
            fixed_content: None,
            deltas: None,
            script: Mutex::new(Vec::new()),
        }
    }

    /// Always return the same content string (no deltas).
    #[must_use]
    pub fn with_fixed_content(content: impl Into<String>) -> Self {
        Self {
            fixed_content: Some(content.into()),
            deltas: None,
            script: Mutex::new(Vec::new()),
        }
    }

    /// Emit model.delta for each chunk; content is the concatenation.
    #[must_use]
    pub fn with_deltas(deltas: Vec<impl Into<String>>) -> Self {
        let deltas: Vec<String> = deltas.into_iter().map(Into::into).collect();
        Self {
            fixed_content: Some(deltas.concat()),
            deltas: Some(deltas),
            script: Mutex::new(Vec::new()),
        }
    }

    /// Consume scripted responses in order, then fall back to greeting.
    #[must_use]
    pub fn with_script(responses: Vec<ModelResponse>) -> Self {
        Self {
            fixed_content: None,
            deltas: None,
            script: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl ModelProvider for FakeModelProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
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
            return Ok(ModelResponse {
                content: content.clone(),
                deltas: self.deltas.clone().unwrap_or_default(),
            });
        }

        Ok(ModelResponse::text(format!("fake-model: {}", request.goal)))
    }
}
