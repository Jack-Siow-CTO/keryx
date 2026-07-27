//! Builtin model-provider registration from env/secret files (local product path).
//!
//! Real providers only — no runtime `fake`. Used by the Worker composition root and doctor.

use crate::consumer::{
    ChatGptCodexProvider, ChatGptWebProvider, ConsumerWebAuth, ConsumerWebConfig, GrokWebProvider,
};
use crate::openai_compatible::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::MultiModelProvider;
use keryx_app::ModelProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

/// How a provider authenticates to its upstream (never secrets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    OauthAccessToken,
    BrowserSession,
}

impl AuthKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OauthAccessToken => "oauth_access_token",
            Self::BrowserSession => "browser_session",
        }
    }
}

/// Non-secret catalog entry for a model provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub name: String,
    pub auth_kind: AuthKind,
    pub display_name: String,
    pub default_model: String,
    /// Recommended or allowlisted model ids (empty = any model accepted).
    pub models: Vec<String>,
    pub registered: bool,
    pub supports_model_override: bool,
}

/// Result of loading all usable builtin providers from the environment.
#[derive(Clone)]
pub struct RegisteredProviders {
    pub multi: Arc<MultiModelProvider>,
    pub descriptors: Vec<ProviderDescriptor>,
    pub default_provider: String,
}

impl RegisteredProviders {
    #[must_use]
    pub fn provider_names(&self) -> Vec<String> {
        self.descriptors
            .iter()
            .filter(|d| d.registered)
            .map(|d| d.name.clone())
            .collect()
    }
}

/// Load and register all builtin model providers that have usable secrets.
///
/// Fail closed when:
/// - no providers register, or
/// - `KERYX_DEFAULT_PROVIDER` is set but not registered, or
/// - multiple providers register and `KERYX_DEFAULT_PROVIDER` is unset.
pub fn register_from_env() -> Result<RegisteredProviders, String> {
    let mut providers: HashMap<String, Arc<dyn ModelProvider>> = HashMap::new();
    let mut descriptors: Vec<ProviderDescriptor> = Vec::new();

    try_register_openai(&mut providers, &mut descriptors)?;
    try_register_grok(&mut providers, &mut descriptors)?;
    try_register_openai_web(&mut providers, &mut descriptors)?;
    try_register_openai_codex(&mut providers, &mut descriptors)?;
    try_register_grok_web(&mut providers, &mut descriptors)?;

    if providers.is_empty() {
        return Err(
            "no model providers configured. Set one of: OPENAI_API_KEY / OPENAI_API_KEY_FILE, \
             XAI_API_KEY / XAI_API_KEY_FILE, CHATGPT_WEB_ACCESS_TOKEN (Codex sub / openai_codex), \
             CHATGPT_WEB_COOKIE (openai_web), or GROK_WEB_COOKIE (grok_web). \
             See docs/deploy/consumer-web-sessions.md and .env.example."
                .into(),
        );
    }

    let names: Vec<String> = providers.keys().cloned().collect();
    let default_provider = resolve_default_provider(&names)?;

    let multi = Arc::new(MultiModelProvider::new(
        default_provider.clone(),
        providers,
        descriptors.clone(),
    ));

    Ok(RegisteredProviders {
        multi,
        descriptors,
        default_provider,
    })
}

fn resolve_default_provider(registered: &[String]) -> Result<String, String> {
    match env::var("KERYX_DEFAULT_PROVIDER") {
        Ok(raw) => {
            let name = raw.trim().to_string();
            if name.is_empty() {
                return resolve_when_unset(registered);
            }
            if name == "fake" {
                return Err(
                    "KERYX_DEFAULT_PROVIDER=fake is no longer supported; configure a real provider \
                     (openai, grok, openai_codex, openai_web, grok_web)"
                        .into(),
                );
            }
            if !registered.iter().any(|n| n == &name) {
                return Err(format!(
                    "KERYX_DEFAULT_PROVIDER='{name}' is not available (registered: {registered:?})"
                ));
            }
            Ok(name)
        }
        Err(_) => resolve_when_unset(registered),
    }
}

fn resolve_when_unset(registered: &[String]) -> Result<String, String> {
    match registered.len() {
        0 => Err("no model providers registered".into()),
        1 => Ok(registered[0].clone()),
        _ => Err(format!(
            "multiple model providers registered ({registered:?}); set KERYX_DEFAULT_PROVIDER to one of them"
        )),
    }
}

fn parse_model_list(env_key: &str) -> Vec<String> {
    env::var(env_key)
        .ok()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn try_register_openai(
    providers: &mut HashMap<String, Arc<dyn ModelProvider>>,
    descriptors: &mut Vec<ProviderDescriptor>,
) -> Result<(), String> {
    let Some(api_key) =
        crate::consumer::load_secret_pair("OPENAI_API_KEY")?.filter(|s| !s.is_empty())
    else {
        return Ok(());
    };
    // Default: gpt-5.6-sol + low reasoning (OpenAI-related session/API path).
    let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".into());
    let models = parse_model_list("OPENAI_MODELS");
    let reasoning = env::var("OPENAI_REASONING_EFFORT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "low".into());
    let mut cfg = OpenAiCompatibleConfig::openai(&api_key, &model).with_reasoning_effort(reasoning);
    if let Ok(base) = env::var("OPENAI_BASE_URL") {
        if !base.trim().is_empty() {
            cfg = cfg.with_base_url(base);
        }
    }
    if !models.is_empty() {
        cfg = cfg.with_allowed_models(models.clone());
    }
    let provider = OpenAiCompatibleProvider::new(cfg).map_err(|e| e.to_string())?;
    providers.insert("openai".into(), Arc::new(provider));
    descriptors.push(ProviderDescriptor {
        name: "openai".into(),
        auth_kind: AuthKind::ApiKey,
        display_name: "OpenAI Platform API".into(),
        default_model: model,
        models,
        registered: true,
        supports_model_override: true,
    });
    Ok(())
}

fn try_register_grok(
    providers: &mut HashMap<String, Arc<dyn ModelProvider>>,
    descriptors: &mut Vec<ProviderDescriptor>,
) -> Result<(), String> {
    let Some(api_key) = crate::consumer::load_secret_pair("XAI_API_KEY")?.filter(|s| !s.is_empty())
    else {
        return Ok(());
    };
    // Default: grok-4.5 + medium reasoning.
    let model = env::var("XAI_MODEL").unwrap_or_else(|_| "grok-4.5".into());
    let models = parse_model_list("XAI_MODELS");
    let reasoning = env::var("XAI_REASONING_EFFORT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "medium".into());
    let mut cfg = OpenAiCompatibleConfig::grok(&api_key, &model).with_reasoning_effort(reasoning);
    if let Ok(base) = env::var("XAI_BASE_URL") {
        if !base.trim().is_empty() {
            cfg = cfg.with_base_url(base);
        }
    }
    if !models.is_empty() {
        cfg = cfg.with_allowed_models(models.clone());
    }
    let provider = OpenAiCompatibleProvider::new(cfg).map_err(|e| e.to_string())?;
    providers.insert("grok".into(), Arc::new(provider));
    descriptors.push(ProviderDescriptor {
        name: "grok".into(),
        auth_kind: AuthKind::ApiKey,
        display_name: "Grok (xAI) API".into(),
        default_model: model,
        models,
        registered: true,
        supports_model_override: true,
    });
    Ok(())
}

fn try_register_openai_web(
    providers: &mut HashMap<String, Arc<dyn ModelProvider>>,
    descriptors: &mut Vec<ProviderDescriptor>,
) -> Result<(), String> {
    let token = crate::consumer::load_secret_pair("CHATGPT_WEB_ACCESS_TOKEN")?;
    let cookie = crate::consumer::load_secret_pair("CHATGPT_WEB_COOKIE")?;
    let auth = ConsumerWebAuth {
        cookie_header: cookie,
        bearer_token: token,
        extra_headers: crate::consumer::read_headers_file("CHATGPT_WEB_HEADERS_FILE")?,
    };
    // Prefer not to register openai_web when only token is present if codex will also use it;
    // still register when usable — operator can choose provider per Run.
    if !auth.is_usable() {
        return Ok(());
    }
    // Only register openai_web when cookie is present OR explicit CHATGPT_WEB_PATH / enable.
    // Token-only is the Codex subscription path; cookie (optionally + token) is browser session.
    let has_cookie = auth.cookie_header.as_ref().is_some_and(|c| !c.is_empty());
    if !has_cookie {
        // Token-only → leave for openai_codex unless operator forces web via path override presence
        // and CHATGPT_WEB_ENABLE=1, or cookie empty but they set CHATGPT_WEB_FORCE=1.
        let force = matches!(
            env::var("CHATGPT_WEB_FORCE").as_deref(),
            Ok("1") | Ok("true") | Ok("yes")
        );
        if !force {
            return Ok(());
        }
    }

    let model = env::var("CHATGPT_WEB_MODEL").unwrap_or_else(|_| "gpt-5.6-sol".into());
    let models = parse_model_list("CHATGPT_WEB_MODELS");
    let config = ConsumerWebConfig {
        provider_name: "openai_web".into(),
        base_url: env::var("CHATGPT_WEB_BASE_URL").unwrap_or_else(|_| "https://chatgpt.com".into()),
        path: env::var("CHATGPT_WEB_PATH").unwrap_or_else(|_| "/backend-api/conversation".into()),
        model: model.clone(),
        auth,
        user_agent: env::var("CHATGPT_WEB_USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                .into()
        }),
        allowed_models: models.clone(),
    };
    let provider = ChatGptWebProvider::new(config).map_err(|e| e.to_string())?;
    providers.insert("openai_web".into(), Arc::new(provider));
    descriptors.push(ProviderDescriptor {
        name: "openai_web".into(),
        auth_kind: AuthKind::BrowserSession,
        display_name: "ChatGPT browser session".into(),
        default_model: model,
        models,
        registered: true,
        supports_model_override: true,
    });
    Ok(())
}

fn try_register_openai_codex(
    providers: &mut HashMap<String, Arc<dyn ModelProvider>>,
    descriptors: &mut Vec<ProviderDescriptor>,
) -> Result<(), String> {
    // Prefer CHATGPT_CODEX_ACCESS_TOKEN alias, fall back to shared CHATGPT_WEB_ACCESS_TOKEN.
    let token = crate::consumer::load_secret_pair("CHATGPT_CODEX_ACCESS_TOKEN")?
        .or(crate::consumer::load_secret_pair(
            "CHATGPT_WEB_ACCESS_TOKEN",
        )?)
        .filter(|t| !t.is_empty());
    let Some(token) = token else {
        return Ok(());
    };

    match ChatGptCodexProvider::from_access_token(token) {
        Ok(provider) => {
            let default_model = provider.default_model().to_string();
            let models = parse_model_list("CHATGPT_CODEX_MODELS");
            providers.insert("openai_codex".into(), Arc::new(provider));
            descriptors.push(ProviderDescriptor {
                name: "openai_codex".into(),
                auth_kind: AuthKind::OauthAccessToken,
                display_name: "ChatGPT subscription (Codex)".into(),
                default_model,
                models,
                registered: true,
                supports_model_override: true,
            });
            Ok(())
        }
        Err(e) => Err(format!("openai_codex config: {e}")),
    }
}

fn try_register_grok_web(
    providers: &mut HashMap<String, Arc<dyn ModelProvider>>,
    descriptors: &mut Vec<ProviderDescriptor>,
) -> Result<(), String> {
    let cookie = crate::consumer::load_secret_pair("GROK_WEB_COOKIE")?.filter(|c| !c.is_empty());
    let Some(cookie) = cookie else {
        return Ok(());
    };
    let model = env::var("GROK_WEB_MODEL").unwrap_or_else(|_| "grok-4.5".into());
    let models = parse_model_list("GROK_WEB_MODELS");
    let reasoning_effort = env::var("GROK_WEB_REASONING_EFFORT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "medium".into());
    let config = ConsumerWebConfig {
        provider_name: "grok_web".into(),
        base_url: env::var("GROK_WEB_BASE_URL").unwrap_or_else(|_| "https://grok.com".into()),
        path: env::var("GROK_WEB_PATH")
            .unwrap_or_else(|_| "/rest/app-chat/conversations/new".into()),
        model: model.clone(),
        auth: ConsumerWebAuth {
            cookie_header: Some(cookie),
            bearer_token: None,
            extra_headers: crate::consumer::read_headers_file("GROK_WEB_HEADERS_FILE")?,
        },
        user_agent: env::var("GROK_WEB_USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (compatible; KeryxWorker/0.1; +https://github.com/Jack-Siow-CTO/keryx)"
                .into()
        }),
        allowed_models: models.clone(),
    };
    let provider = GrokWebProvider::new_with_reasoning(config, Some(reasoning_effort))
        .map_err(|e| e.to_string())?;
    providers.insert("grok_web".into(), Arc::new(provider));
    descriptors.push(ProviderDescriptor {
        name: "grok_web".into(),
        auth_kind: AuthKind::BrowserSession,
        display_name: "Grok web session".into(),
        default_model: model,
        models,
        registered: true,
        supports_model_override: true,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_single_provider_without_env() {
        let name = resolve_when_unset(&["openai".into()]).unwrap();
        assert_eq!(name, "openai");
    }

    #[test]
    fn resolve_multiple_requires_explicit() {
        let err = resolve_when_unset(&["openai".into(), "grok".into()]).unwrap_err();
        assert!(err.contains("KERYX_DEFAULT_PROVIDER"), "{err}");
    }

    #[test]
    fn rejects_fake_name() {
        // SAFETY: unit test; process-local env
        std::env::set_var("KERYX_DEFAULT_PROVIDER", "fake");
        let err = resolve_default_provider(&["openai".into()]).unwrap_err();
        std::env::remove_var("KERYX_DEFAULT_PROVIDER");
        assert!(err.contains("fake"), "{err}");
    }
}
