//! Skills as versioned document packages under a skills root.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use keryx_domain::RunOrigin;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// In-memory draft proposals from the learning loop (not auto-applied for reduced origin).
#[derive(Debug, Default)]
pub struct SkillDraftStore {
    drafts: Mutex<Vec<String>>,
}

impl SkillDraftStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, draft: String) {
        if let Ok(mut d) = self.drafts.lock() {
            d.push(draft);
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.drafts
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default()
    }
}

/// Skills tools: list/view/load + gated learning drafts.
pub struct SkillsTools {
    allowed: HashSet<String>,
    skills_root: PathBuf,
    origin: RunOrigin,
    drafts: std::sync::Arc<SkillDraftStore>,
    /// When true and origin is control_plane, skill_manage may write packages.
    auto_apply_trusted: bool,
}

impl SkillsTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        skills_root: PathBuf,
        origin: RunOrigin,
        drafts: std::sync::Arc<SkillDraftStore>,
    ) -> Self {
        Self {
            allowed,
            skills_root,
            origin,
            drafts,
            auto_apply_trusted: true,
        }
    }
}

#[async_trait]
impl ToolRuntime for SkillsTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "skills_list" => self.list_skills().await,
            "skill_view" | "skill_load" => self.view_skill(&call.arguments).await,
            "skill_draft" => self.draft_skill(&call.arguments).await,
            "skill_manage" => self.manage_skill(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}

impl SkillsTools {
    async fn list_skills(&self) -> Result<ToolResult, ToolError> {
        let root = &self.skills_root;
        if !root.exists() {
            return Ok(ToolResult {
                content: "no skills root".into(),
                summary: "skills_list hits=0".into(),
            });
        }
        let mut names = Vec::new();
        let mut entries = tokio::fs::read_dir(root)
            .await
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ToolError::Failed(e.to_string()))?
        {
            if entry
                .file_type()
                .await
                .map(|t| t.is_dir())
                .unwrap_or(false)
            {
                names.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        names.sort();
        Ok(ToolResult {
            summary: format!("skills_list hits={}", names.len()),
            content: if names.is_empty() {
                "no skills".into()
            } else {
                names.join("\n")
            },
        })
    }

    async fn view_skill(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing name".into()))?;
        if name.contains("..") || name.contains('/') || name.contains('\\') {
            return Err(ToolError::PathJail("invalid skill name".into()));
        }
        let path = self.skills_root.join(name).join("SKILL.md");
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::Failed(format!("skill not found: {e}")))?;
        // Progressive disclosure: return content; do not dump all skills.
        Ok(ToolResult {
            summary: format!("skill_view name={name} chars={}", content.len()),
            content,
        })
    }

    async fn draft_skill(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("untitled");
        let body = args
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("");
        let draft = format!("# draft skill {name}\n{body}");
        self.drafts.push(draft.clone());
        // Gateway/reduced: draft only, never auto-apply.
        if self.origin.is_reduced_trust() {
            return Ok(ToolResult {
                content: format!("draft recorded (reduced origin; not applied): {name}"),
                summary: format!("skill_draft name={name} applied=false"),
            });
        }
        Ok(ToolResult {
            content: format!("draft recorded: {name}"),
            summary: format!("skill_draft name={name}"),
        })
    }

    async fn manage_skill(&self, args: &Value) -> Result<ToolResult, ToolError> {
        // High-blast: auto-apply only trusted origin + Policy (caller also may Approval-gate).
        if self.origin.is_reduced_trust() {
            return Err(ToolError::Denied(
                "skill_manage denied for reduced origin (draft only)".into(),
            ));
        }
        if !self.auto_apply_trusted {
            return Err(ToolError::Denied(
                "skill_manage auto-apply disabled".into(),
            ));
        }
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing name".into()))?;
        if name.contains("..") || name.contains('/') {
            return Err(ToolError::PathJail("invalid skill name".into()));
        }
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing content".into()))?;
        let dir = self.skills_root.join(name);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        let path = dir.join("SKILL.md");
        tokio::fs::write(&path, content.as_bytes())
            .await
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        Ok(ToolResult {
            content: format!("skill applied: {name}"),
            summary: format!("skill_manage name={name} applied=true"),
        })
    }
}

/// Ensure skills root exists (operator setup helper).
pub fn ensure_skills_root(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)
}
