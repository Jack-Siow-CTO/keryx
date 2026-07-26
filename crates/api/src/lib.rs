//! Keryx control-plane API adapter (HTTP/JSON + SSE).
//!
//! Transport only: maps domain/app errors at the boundary (ADR 0008).

/// Workspace smoke: API adapter is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-api"
}

/// Confirms adapter dependency direction: domain ← app ← api.
#[must_use]
pub fn upstream_crate_names() -> (&'static str, &'static str) {
    (keryx_domain::crate_name(), keryx_app::crate_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_smoke() {
        assert_eq!(crate_name(), "keryx-api");
        assert_eq!(upstream_crate_names(), ("keryx-domain", "keryx-app"));
    }
}
