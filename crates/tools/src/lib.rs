//! Keryx tool adapters (workspace file read/write under Policy and path jail).
//!
//! Concrete tools land in a later ticket; this crate is the composition slot.

/// Workspace smoke: tools adapter is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-tools"
}

/// Confirms adapter dependency direction: domain ← app ← tools.
#[must_use]
pub fn upstream_crate_names() -> (&'static str, &'static str) {
    (keryx_domain::crate_name(), keryx_app::crate_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_smoke() {
        assert_eq!(crate_name(), "keryx-tools");
        assert_eq!(upstream_crate_names(), ("keryx-domain", "keryx-app"));
    }
}
