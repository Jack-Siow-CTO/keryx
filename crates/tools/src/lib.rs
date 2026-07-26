//! Keryx tool adapters (workspace file read/write under Policy and path jail).

mod workspace;

pub use workspace::{resolve_in_workspace, WorkspaceFsTools};

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
    use keryx_app::{ToolCall, ToolRuntime};
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn tools_smoke() {
        assert_eq!(crate_name(), "keryx-tools");
        assert_eq!(upstream_crate_names(), ("keryx-domain", "keryx-app"));
    }

    #[tokio::test]
    async fn path_jail_denies_escape_and_allows_inside_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("hello.txt"), "hi").unwrap();

        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["read_file".into(), "write_file".into()]),
        );

        let ok = tools
            .invoke(ToolCall {
                name: "read_file".into(),
                arguments: json!({ "path": "hello.txt" }),
            })
            .await
            .unwrap();
        assert_eq!(ok.content, "hi");

        let escape = tools
            .invoke(ToolCall {
                name: "read_file".into(),
                arguments: json!({ "path": "../secret" }),
            })
            .await
            .unwrap_err();
        assert!(escape.to_string().contains("path jail"), "{escape}");

        let absolute = tools
            .invoke(ToolCall {
                name: "read_file".into(),
                arguments: json!({ "path": "/etc/passwd" }),
            })
            .await
            .unwrap_err();
        assert!(absolute.to_string().contains("path jail"), "{absolute}");
    }

    #[tokio::test]
    async fn unknown_tool_default_denied() {
        let root = tempfile::tempdir().unwrap();
        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["read_file".into()]),
        );
        let err = tools
            .invoke(ToolCall {
                name: "shell_exec".into(),
                arguments: json!({}),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("denied"), "{err}");
    }
}
