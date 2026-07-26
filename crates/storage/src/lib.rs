//! Keryx storage adapter (`SQLite` durability + in-memory for fast Seam 1 tests).

mod memory;
mod sqlite;

pub use memory::InMemorySessionStore;
pub use sqlite::SqliteSessionStore;

/// Workspace smoke: storage adapter is loadable.
#[must_use]
pub fn crate_name() -> &'static str {
    "keryx-storage"
}

/// Confirms adapter dependency direction: domain ← app ← storage.
#[must_use]
pub fn upstream_crate_names() -> (&'static str, &'static str) {
    (keryx_domain::crate_name(), keryx_app::crate_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_smoke() {
        assert_eq!(crate_name(), "keryx-storage");
        assert_eq!(upstream_crate_names(), ("keryx-domain", "keryx-app"));
    }
}
