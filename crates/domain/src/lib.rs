//! Keryx domain: pure types and rules for Sessions, Runs, Principals, Policy, Tools, and Run events.
//!
//! This crate has no I/O, HTTP, `SQLite`, or provider SDKs (ADR 0008).

mod ids;
mod principal;
mod run;
mod session;

pub use ids::{RunId, SessionId};
pub use principal::{Principal, PrincipalId};
pub use run::{Run, RunStatus};
pub use session::Session;

/// Workspace smoke: domain crate is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-domain"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_smoke() {
        assert_eq!(crate_name(), "keryx-domain");
    }
}
