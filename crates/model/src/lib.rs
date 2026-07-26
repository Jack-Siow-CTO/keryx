//! Keryx model provider adapters (OpenAI, Grok) and test fakes.

mod fake;

pub use fake::FakeModelProvider;

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
