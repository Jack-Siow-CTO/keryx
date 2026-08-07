use crate::RunOrigin;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Constraints applied to a Session or Run (tool allowlists and related rules).
///
/// Unknown tools are denied (fail closed). Templates are selected by Run origin;
/// later tickets may freeze a Policy snapshot on Schedules or escalate mid-Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Tool names this Policy permits. Absent names are denied.
    pub allowed_tools: BTreeSet<String>,
}

impl Policy {
    /// Empty allowlist (deny all tools).
    #[must_use]
    pub fn deny_all() -> Self {
        Self {
            allowed_tools: BTreeSet::new(),
        }
    }

    /// Build the default Policy template for a Run origin.
    ///
    /// - `control_plane`: broader trusted tools (workspace read + write)
    /// - `schedule` / `gateway:*`: reduced (read-only workspace by default)
    #[must_use]
    pub fn for_origin(origin: &RunOrigin) -> Self {
        match origin {
            RunOrigin::ControlPlane => Self::control_plane_default(),
            RunOrigin::Schedule | RunOrigin::Gateway { .. } => Self::reduced(),
        }
    }

    /// Default Policy for control-plane origin (trusted operator API).
    ///
    /// Matches tools the Worker can compose today (workspace FS, web, memory,
    /// skills, terminal). Does **not** auto-include discovered MCP tools.
    /// Operators add exact `mcp.<server_id>.<tool>` names via
    /// [`Policy::with_extra_tools`] / Worker `KERYX_POLICY_EXTRA_TOOLS`.
    /// Seam 1 tests opt in with `ControlPlane::with_control_plane_extra_tools`.
    ///
    /// `skill_manage` is allowed here but Approval-gated when skill auto-commit
    /// is OFF (factory default) — see agent loop high-blast rules.
    #[must_use]
    pub fn control_plane_default() -> Self {
        Self {
            allowed_tools: BTreeSet::from([
                "read_file".into(),
                "write_file".into(),
                "apply_patch".into(),
                "search_files".into(),
                "web_search".into(),
                "web_extract".into(),
                "memory_read".into(),
                "memory_write".into(),
                "memory_update".into(),
                "memory_delete".into(),
                "memory_search".into(),
                "session_search".into(),
                "skills_list".into(),
                "skill_load".into(),
                "skill_manage".into(),
                "run_terminal".into(),
            ]),
        }
    }

    /// Reduced Policy for untrusted origins (`gateway:*`, `schedule`).
    ///
    /// Read/search/web/memory-read/skills-list-load + skill_manage **proposal** only.
    /// Memory mutations from reduced origin are denied (fail closed).
    /// `skill_manage` never silent-writes: agent loop always requires Approval
    /// for reduced origin (auto-commit does not apply).
    /// Terminal allowed only with Docker backend (enforced in tool adapter).
    /// **No MCP tools** by default (connect ≠ allow; gateways/cron cannot send mail).
    #[must_use]
    pub fn reduced() -> Self {
        Self {
            allowed_tools: BTreeSet::from([
                "read_file".into(),
                "search_files".into(),
                "web_search".into(),
                "web_extract".into(),
                "memory_read".into(),
                "memory_search".into(),
                "session_search".into(),
                "skills_list".into(),
                "skill_load".into(),
                "skill_manage".into(), // proposal only; Approval before write
                "run_terminal".into(), // docker-only for reduced origin
                                       // no memory_write, no mcp.*
            ]),
        }
    }

    /// Fail-closed tool allowlist check.
    #[must_use]
    pub fn allows_tool(&self, name: &str) -> bool {
        self.allowed_tools.contains(name)
    }

    /// Whether `name` is an MCP client tool (`mcp.<server_id>.…`).
    #[must_use]
    pub fn is_mcp_tool_name(name: &str) -> bool {
        name.starts_with("mcp.")
    }

    /// Union exact tool names into this Policy (operator extras / MCP allowlist).
    #[must_use]
    pub fn with_extra_tools(mut self, extra: impl IntoIterator<Item = String>) -> Self {
        self.allowed_tools.extend(extra);
        self
    }

    /// Intersect allowlists so a Child Run cannot exceed parent Policy authority.
    #[must_use]
    pub fn subset_of(&self, parent: &Policy) -> Self {
        Self {
            allowed_tools: self
                .allowed_tools
                .intersection(&parent.allowed_tools)
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_plane_allows_read_write_patch_search_web() {
        let p = Policy::for_origin(&RunOrigin::control_plane());
        assert!(p.allows_tool("read_file"));
        assert!(p.allows_tool("write_file"));
        assert!(p.allows_tool("apply_patch"));
        assert!(p.allows_tool("search_files"));
        assert!(p.allows_tool("web_search"));
        assert!(p.allows_tool("web_extract"));
        assert!(p.allows_tool("skills_list"));
        assert!(p.allows_tool("skill_load"));
        assert!(p.allows_tool("skill_manage"));
        assert!(!p.allows_tool("shell_exec"));
    }

    #[test]
    fn reduced_origins_deny_write_and_unknown() {
        for origin in [
            RunOrigin::schedule(),
            RunOrigin::gateway("telegram"),
            RunOrigin::gateway("discord"),
        ] {
            let p = Policy::for_origin(&origin);
            assert!(p.allows_tool("read_file"), "{origin}");
            assert!(p.allows_tool("search_files"), "{origin}");
            assert!(p.allows_tool("web_search"), "{origin}");
            assert!(p.allows_tool("web_extract"), "{origin}");
            assert!(!p.allows_tool("write_file"), "{origin}");
            assert!(!p.allows_tool("apply_patch"), "{origin}");
            assert!(!p.allows_tool("memory_write"), "{origin}");
            assert!(!p.allows_tool("memory_update"), "{origin}");
            assert!(!p.allows_tool("memory_delete"), "{origin}");
            assert!(p.allows_tool("memory_read"), "{origin}");
            assert!(p.allows_tool("memory_search"), "{origin}");
            assert!(p.allows_tool("session_search"), "{origin}");
            assert!(p.allows_tool("skills_list"), "{origin}");
            assert!(p.allows_tool("skill_load"), "{origin}");
            assert!(p.allows_tool("skill_manage"), "{origin}");
            assert!(!p.allows_tool("shell_exec"), "{origin}");
        }
    }

    #[test]
    fn deny_all_fails_closed() {
        let p = Policy::deny_all();
        assert!(!p.allows_tool("read_file"));
    }

    #[test]
    fn reduced_origins_have_no_mcp_tools_by_default() {
        for origin in [
            RunOrigin::schedule(),
            RunOrigin::gateway("telegram"),
            RunOrigin::gateway("discord"),
        ] {
            let p = Policy::for_origin(&origin);
            assert!(
                !p.allowed_tools.iter().any(|t| Policy::is_mcp_tool_name(t)),
                "reduced origin {origin} must not include MCP tools"
            );
            assert!(!p.allows_tool("mcp.demo.echo"), "{origin}");
            assert!(!p.allows_tool("mcp.gmail.send"), "{origin}");
        }
    }

    #[test]
    fn control_plane_does_not_auto_include_mcp() {
        let p = Policy::control_plane_default();
        // Production hole closed: no fixture or product MCP tools by default.
        assert!(!p.allows_tool("mcp.demo.echo"));
        assert!(!p.allows_tool("mcp.gmail.send"));
        assert!(!p.allowed_tools.iter().any(|t| Policy::is_mcp_tool_name(t)));
        let extended = p.with_extra_tools(["mcp.gmail.send".into(), "mcp.demo.echo".into()]);
        assert!(extended.allows_tool("mcp.gmail.send"));
        assert!(extended.allows_tool("mcp.demo.echo"));
    }

    #[test]
    fn child_subset_cannot_gain_mcp() {
        let parent = Policy::control_plane_default().with_extra_tools(["mcp.mail.search".into()]);
        let child_want =
            Policy::deny_all().with_extra_tools(["mcp.mail.search".into(), "mcp.mail.send".into()]);
        let child = child_want.subset_of(&parent);
        assert!(child.allows_tool("mcp.mail.search"));
        assert!(!child.allows_tool("mcp.mail.send"));
    }
}
