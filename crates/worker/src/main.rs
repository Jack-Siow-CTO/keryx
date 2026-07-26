//! Keryx worker binary — composition root for domain, app, and adapters (ADR 0008).
//!
//! Product boot (config, loopback bind, graceful shutdown) lands in a later ticket.

fn main() {
    println!(
        "keryx worker skeleton (domain={} app={} storage={} model={} tools={} api={})",
        keryx_domain::crate_name(),
        keryx_app::crate_name(),
        keryx_storage::crate_name(),
        keryx_model::crate_name(),
        keryx_tools::crate_name(),
        keryx_api::crate_name(),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn worker_wires_adapters() {
        assert_eq!(keryx_domain::crate_name(), "keryx-domain");
        assert_eq!(keryx_app::crate_name(), "keryx-app");
        assert_eq!(keryx_storage::crate_name(), "keryx-storage");
        assert_eq!(keryx_model::crate_name(), "keryx-model");
        assert_eq!(keryx_tools::crate_name(), "keryx-tools");
        assert_eq!(keryx_api::crate_name(), "keryx-api");
    }
}
