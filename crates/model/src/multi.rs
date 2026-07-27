use crate::registry::ProviderDescriptor;
use async_trait::async_trait;
use keryx_app::{ModelError, ModelProvider, ModelRequest, ModelResponse};
use std::collections::HashMap;
use std::sync::Arc;

/// Routes completions to a named Model provider (`openai`, `grok`, `openai_codex`, …).
pub struct MultiModelProvider {
    providers: HashMap<String, Arc<dyn ModelProvider>>,
    default: String,
    descriptors: Vec<ProviderDescriptor>,
}

impl MultiModelProvider {
    #[must_use]
    pub fn new(
        default: impl Into<String>,
        providers: HashMap<String, Arc<dyn ModelProvider>>,
        descriptors: Vec<ProviderDescriptor>,
    ) -> Self {
        Self {
            providers,
            default: default.into(),
            descriptors,
        }
    }

    #[must_use]
    pub fn single(name: impl Into<String>, provider: Arc<dyn ModelProvider>) -> Self {
        let name = name.into();
        let mut providers = HashMap::new();
        providers.insert(name.clone(), provider);
        let descriptors = vec![ProviderDescriptor {
            name: name.clone(),
            auth_kind: crate::registry::AuthKind::ApiKey,
            display_name: name.clone(),
            default_model: String::new(),
            models: Vec::new(),
            registered: true,
            supports_model_override: true,
        }];
        Self {
            providers,
            default: name,
            descriptors,
        }
    }

    #[must_use]
    pub fn default_provider(&self) -> &str {
        &self.default
    }

    #[must_use]
    pub fn descriptors(&self) -> &[ProviderDescriptor] {
        &self.descriptors
    }

    #[must_use]
    pub fn registered_names(&self) -> Vec<&str> {
        self.providers.keys().map(String::as_str).collect()
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
