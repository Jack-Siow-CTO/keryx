//! Keryx tool adapters (workspace file tools, web tools under Policy).

mod mcp;
mod memory;
mod skills;
mod terminal;
mod web;
mod workspace;

pub use mcp::{
    build_mcp_runtimes, load_mcp_config, mock_registry_from_peer, namespaced_tool_name,
    parse_mcp_config_json, parse_namespaced_tool, validate_server_id, McpClientRegistry,
    McpClientTools, McpConfig, McpDoctorReport, McpRuntimeBundle, McpServerConfig, McpServerExport,
    McpServerHealth, McpSession, McpTransportConfig, MockMcpPeer, MCP_NAMESPACE_PREFIX,
};
pub use memory::MemoryTools;
pub use skills::{
    skills_root_doctor_status, validate_skill_name, SkillsRootDoctorKind, SkillsRootDoctorStatus,
    SkillsTools,
};
pub use terminal::{
    ExecBackend, ExecBackendRunner, FixedExecRunner, SystemExecRunner, TerminalTools,
};
pub use web::{
    assert_resolved_public, is_public_ip, validate_public_http_url, CompositeTools,
    FixedWebExtract, FixedWebSearch, HttpWebExtract, SearchHit, UnconfiguredWebSearch,
    WebExtractBackend, WebSearchBackend, WebTools,
};
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

    #[tokio::test]
    async fn apply_patch_precise_edit_and_path_jail() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "hello world").unwrap();

        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["apply_patch".into(), "search_files".into()]),
        );

        let ok = tools
            .invoke(ToolCall {
                name: "apply_patch".into(),
                arguments: json!({
                    "path": "a.txt",
                    "old_string": "world",
                    "new_string": "keryx"
                }),
            })
            .await
            .unwrap();
        assert!(ok.content.contains("1 replacement"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "hello keryx"
        );

        let escape = tools
            .invoke(ToolCall {
                name: "apply_patch".into(),
                arguments: json!({
                    "path": "../escape.txt",
                    "old_string": "x",
                    "new_string": "y"
                }),
            })
            .await
            .unwrap_err();
        assert!(escape.to_string().contains("path jail"), "{escape}");
    }

    #[tokio::test]
    async fn search_files_finds_content_and_stays_in_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("sub")).unwrap();
        std::fs::write(root.path().join("sub/note.txt"), "unique-token-xyz").unwrap();
        std::fs::write(root.path().join("other.txt"), "nothing").unwrap();

        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["search_files".into()]),
        );

        let found = tools
            .invoke(ToolCall {
                name: "search_files".into(),
                arguments: json!({ "query": "unique-token-xyz" }),
            })
            .await
            .unwrap();
        assert!(
            found.content.contains("note.txt"),
            "expected hit: {}",
            found.content
        );
        assert!(found.summary.contains("hits="));

        let outside = tools
            .invoke(ToolCall {
                name: "search_files".into(),
                arguments: json!({ "query": "x", "path": ".." }),
            })
            .await
            .unwrap_err();
        assert!(outside.to_string().contains("path jail"), "{outside}");
    }

    #[tokio::test]
    async fn search_files_survives_directory_symlink_loop() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("note.txt"), "loop-safe-content").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path(), root.path().join("loop")).unwrap();
        }
        #[cfg(not(unix))]
        {
            // Non-unix: still exercise search without loop fixture.
            let tools = WorkspaceFsTools::new(
                vec![root.path().to_path_buf()],
                HashSet::from(["search_files".into()]),
            );
            let found = tools
                .invoke(ToolCall {
                    name: "search_files".into(),
                    arguments: json!({ "query": "loop-safe-content" }),
                })
                .await
                .unwrap();
            assert!(found.content.contains("note.txt"));
            return;
        }

        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["search_files".into()]),
        );
        // Must terminate even when workspace contains `loop -> root`.
        let found = tools
            .invoke(ToolCall {
                name: "search_files".into(),
                arguments: json!({ "query": "loop-safe-content" }),
            })
            .await
            .unwrap();
        assert!(
            found.content.contains("note.txt"),
            "expected content hit without hang: {}",
            found.content
        );
    }

    #[tokio::test]
    async fn symlink_escape_outside_root_denied_on_read() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "top-secret").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("secret.txt"),
                root.path().join("link.txt"),
            )
            .unwrap();
        }
        #[cfg(not(unix))]
        {
            return;
        }

        let tools = WorkspaceFsTools::new(
            vec![root.path().to_path_buf()],
            HashSet::from(["read_file".into(), "apply_patch".into()]),
        );
        let err = tools
            .invoke(ToolCall {
                name: "read_file".into(),
                arguments: json!({ "path": "link.txt" }),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("path jail"), "{err}");
    }
}
