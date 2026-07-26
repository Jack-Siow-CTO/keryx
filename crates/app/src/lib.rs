//! Keryx app: agent loop orchestration, budgets, Active Run exclusivity, global cap, cancel.
//!
//! Depends on domain ports/types only—not concrete adapters (ADR 0008).

/// Workspace smoke: app crate depends on domain and is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-app"
}

/// Confirms the domain dependency is wired (dependency direction: domain ← app).
#[must_use]
pub fn domain_crate_name() -> &'static str {
    keryx_domain::crate_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_smoke() {
        assert_eq!(crate_name(), "keryx-app");
        assert_eq!(domain_crate_name(), "keryx-domain");
    }
}
