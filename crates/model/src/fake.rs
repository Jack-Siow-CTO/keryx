use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use std::sync::Mutex;

/// Scripted Model provider for Seam 1 control-plane tests (no live network).
#[derive(Debug)]
pub struct FakeModelProvider {
    /// When set, every completion returns this content.
    fixed_content: Option<String>,
    /// Optional scripted responses consumed FIFO; falls back to fixed/default.
    script: Mutex<Vec<String>>,
}

impl FakeModelProvider {
    /// Always answer with a deterministic greeting derived from the goal.
    #[must_use]
    pub fn greeting() -> Self {
        Self {
            fixed_content: None,
            script: Mutex::new(Vec::new()),
        }
    }

    /// Always return the same content string.
    #[must_use]
    pub fn with_fixed_content(content: impl Into<String>) -> Self {
        Self {
            fixed_content: Some(content.into()),
            script: Mutex::new(Vec::new()),
        }
    }

    /// Consume scripted contents in order, then fail if exhausted.
    #[must_use]
    pub fn with_script(responses: Vec<String>) -> Self {
        Self {
            fixed_content: None,
            script: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl ModelProvider for FakeModelProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        if let Some(content) = &self.fixed_content {
            return Ok(ModelResponse {
                content: content.clone(),
            });
        }

        let mut script = self
            .script
            .lock()
            .map_err(|e| ModelError::new(e.to_string()))?;
        if !script.is_empty() {
            let content = script.remove(0);
            return Ok(ModelResponse { content });
        }

        Ok(ModelResponse {
            content: format!("fake-model: {}", request.goal),
        })
    }
}
