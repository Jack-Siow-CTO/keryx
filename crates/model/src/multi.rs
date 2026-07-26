use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// Routes completions to a named Model provider (`openai`, `grok`, `fake`, …).
pub struct MultiModelProvider {
    providers: HashMap<String, Arc<dyn ModelProvider>>,
    default: String,
}

impl MultiModelProvider {
    #[must_use]
    pub fn new(
        default: impl Into<String>,
        providers: HashMap<String, Arc<dyn ModelProvider>>,
    ) -> Self {
        Self {
            providers,
            default: default.into(),
        }
    }

    #[must_use]
    pub fn single(name: impl Into<String>, provider: Arc<dyn ModelProvider>) -> Self {
        let name = name.into();
        let mut providers = HashMap::new();
        providers.insert(name.clone(), provider);
        Self {
            providers,
            default: name,
        }
    }
}

#[async_trait]
impl ModelProvider for MultiModelProvider {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        let name = request.provider.as_deref().unwrap_or(self.default.as_str());
        let provider = self
            .providers
            .get(name)
            .ok_or_else(|| ModelError::new(format!("unknown model provider '{name}'")))?;
        provider.complete(request).await
    }
}
