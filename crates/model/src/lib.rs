//! Keryx model provider adapters (OpenAI, Grok, consumer web sessions) and test fakes.

mod consumer;
mod fake;
mod multi;
mod openai_compatible;

pub use consumer::{
    load_secret, load_secret_pair, read_headers_file, redact_secrets, ChatGptCodexProvider,
    ChatGptWebProvider, ConsumerWebAuth, ConsumerWebConfig, GrokWebProvider,
};
pub use fake::FakeModelProvider;
pub use multi::MultiModelProvider;
pub use openai_compatible::{
    grok_provider, openai_provider, GrokProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
    OpenAiProvider,
};

/// Workspace smoke: model adapter is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-model"
}

/// Confirms adapter dependency direction: domain ← app ← model.
#[must_use]
pub fn upstream_crate_names() -> (&'static str, &'static str) {
    (keryx_domain::crate_name(), keryx_app::crate_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_smoke() {
        assert_eq!(crate_name(), "keryx-model");
        assert_eq!(upstream_crate_names(), ("keryx-domain", "keryx-app"));
    }
}
