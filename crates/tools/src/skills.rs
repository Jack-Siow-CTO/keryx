//! Skills package tools (agentskills-style layout under skills root).
//!
//! Layout: `{skills_root}/{name}/SKILL.md`
//! Learning-loop writes go through `skill_manage` and are Approval-gated in the
//! agent loop when auto-commit is OFF or origin is reduced trust.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Skill tools backed by a filesystem skills root.
pub struct SkillsTools {
    root: PathBuf,
    allowed: HashSet<String>,
}

impl SkillsTools {
    #[must_use]
    pub fn new(root: PathBuf, allowed: HashSet<String>) -> Self {
        Self { root, allowed }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
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
            "skills_list" => self.skills_list().await,
            "skill_load" => self.skill_load(&call.arguments).await,
            "skill_manage" => self.skill_manage(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        let mut out = Vec::new();
        if self.allowed.contains("skills_list") {
            out.push(ToolSpec::empty_params(
                "skills_list",
                "List Skill packages under the skills root (name only)",
            ));
        }
        if self.allowed.contains("skill_load") {
            out.push(ToolSpec::new(
                "skill_load",
                "Load a Skill package SKILL.md into the Run context",
                json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Skill package directory name" }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }),
            ));
        }
        if self.allowed.contains("skill_manage") {
            out.push(ToolSpec::new(
                "skill_manage",
                "Create or improve a Skill package (high-blast; Approval when auto-commit OFF)",
                json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "improve"],
                            "description": "create new package or improve existing"
                        },
                        "name": { "type": "string", "description": "Skill package name" },
                        "content": {
                            "type": "string",
                            "description": "Full SKILL.md markdown body"
                        }
                    },
                    "required": ["action", "name", "content"],
                    "additionalProperties": false
                }),
            ));
        }
        out
    }
}

impl SkillsTools {
    async fn skills_list(&self) -> Result<ToolResult, ToolError> {
        ensure_root_readable(&self.root)?;
        let names = list_skill_names(&self.root)?;
        let content = if names.is_empty() {
            "(no skills)".to_string()
        } else {
            names.join("\n")
        };
        Ok(ToolResult {
            summary: format!("skills_list count={}", names.len()),
            content,
        })
    }

    async fn skill_load(&self, args: &Value) -> Result<ToolResult, ToolError> {
        ensure_root_readable(&self.root)?;
        let name = arg_string(args, "name")?;
        validate_skill_name(&name)?;
        let path = self.root.join(&name).join("SKILL.md");
        let content = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::Failed(format!("skill not found: {name}"))
            } else {
                ToolError::Failed(format!("skill_load {name}: {e}"))
            }
        })?;
        Ok(ToolResult {
            summary: format!("skill_load name={name}"),
            content: format!("[Skill:{name}]\n{content}"),
        })
    }

    async fn skill_manage(&self, args: &Value) -> Result<ToolResult, ToolError> {
        ensure_root_writable(&self.root)?;
        let action = arg_string(args, "action")?;
        let name = arg_string(args, "name")?;
        let content = arg_string(args, "content")?;
        validate_skill_name(&name)?;
        if content.trim().is_empty() {
            return Err(ToolError::Failed(
                "skill_manage: content must not be empty".into(),
            ));
        }
        let pkg = self.root.join(&name);
        let skill_md = pkg.join("SKILL.md");
        match action.as_str() {
            "create" => {
                if skill_md.is_file() {
                    return Err(ToolError::Failed(format!(
                        "skill_manage create: skill already exists: {name}"
                    )));
                }
            }
            "improve" => {
                if !skill_md.is_file() {
                    return Err(ToolError::Failed(format!(
                        "skill_manage improve: skill not found: {name}"
                    )));
                }
            }
            other => {
                return Err(ToolError::Failed(format!(
                    "skill_manage: unknown action '{other}' (use create|improve)"
                )));
            }
        }
        std::fs::create_dir_all(&pkg)
            .map_err(|e| ToolError::Failed(format!("skill_manage mkdir {name}: {e}")))?;
        std::fs::write(&skill_md, content.as_bytes())
            .map_err(|e| ToolError::Failed(format!("skill_manage write {name}: {e}")))?;
        Ok(ToolResult {
            summary: format!("skill_manage {action} name={name}"),
            content: format!("skill {action} applied: {name}/SKILL.md"),
        })
    }
}

/// Validate package directory name (no path traversal; agentskills-safe).
pub fn validate_skill_name(name: &str) -> Result<(), ToolError> {
    if name.is_empty() || name.len() > 64 {
        return Err(ToolError::Failed(
            "skill name must be 1..=64 characters".into(),
        ));
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ToolError::Failed(
            "skill name must not contain path separators".into(),
        ));
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err(ToolError::Failed("skill name empty".into()));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(ToolError::Failed(
            "skill name must start with alphanumeric".into(),
        ));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ToolError::Failed(
            "skill name may only contain [A-Za-z0-9_-]".into(),
        ));
    }
    Ok(())
}

fn list_skill_names(root: &Path) -> Result<Vec<String>, ToolError> {
    let mut names = Vec::new();
    let rd = std::fs::read_dir(root)
        .map_err(|e| ToolError::Failed(format!("skills root unreadable: {e}")))?;
    for ent in rd.flatten() {
        let path = ent.path();
        if path.is_dir() && path.join("SKILL.md").is_file() {
            let name = ent.file_name().to_string_lossy().to_string();
            if validate_skill_name(&name).is_ok() {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

fn ensure_root_readable(root: &Path) -> Result<(), ToolError> {
    if !root.is_dir() {
        return Err(ToolError::Failed(format!(
            "skills root missing or not a directory: {}",
            root.display()
        )));
    }
    // Prove read access.
    std::fs::read_dir(root).map_err(|e| {
        ToolError::Failed(format!("skills root unreadable ({}): {e}", root.display()))
    })?;
    Ok(())
}

fn ensure_root_writable(root: &Path) -> Result<(), ToolError> {
    ensure_root_readable(root)?;
    let probe = root.join(".keryx-skills-write-probe");
    std::fs::write(&probe, b"ok").map_err(|e| {
        ToolError::Failed(format!(
            "skills root not writable ({}): {e}",
            root.display()
        ))
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

fn arg_string(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed(format!("missing string argument '{key}'")))
}

/// Doctor classification for skills root posture (#69 / #76).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsRootDoctorKind {
    /// Root path does not exist or is not a directory — fail closed.
    Missing,
    /// Root exists but is not readable — fail closed.
    Unreadable,
    /// Root exists and is readable but has no packages — soft-warn OK.
    Empty,
    /// Root has at least one package with SKILL.md.
    Ok,
}

/// Result of skills root doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillsRootDoctorStatus {
    pub kind: SkillsRootDoctorKind,
    pub detail: String,
}

/// Classify skills root for `keryx doctor` (empty OK; missing/unreadable fail).
#[must_use]
pub fn skills_root_doctor_status(root: &Path) -> SkillsRootDoctorStatus {
    if !root.exists() || !root.is_dir() {
        return SkillsRootDoctorStatus {
            kind: SkillsRootDoctorKind::Missing,
            detail: format!("skills root missing ({})", root.display()),
        };
    }
    match std::fs::read_dir(root) {
        Err(e) => SkillsRootDoctorStatus {
            kind: SkillsRootDoctorKind::Unreadable,
            detail: format!("skills root unreadable ({}): {e}", root.display()),
        },
        Ok(rd) => {
            let mut count = 0u32;
            for ent in rd.flatten() {
                if ent.path().is_dir() && ent.path().join("SKILL.md").is_file() {
                    count += 1;
                }
            }
            if count == 0 {
                SkillsRootDoctorStatus {
                    kind: SkillsRootDoctorKind::Empty,
                    detail: format!(
                        "skills root empty ({}) — OK; learning loop may populate",
                        root.display()
                    ),
                }
            } else {
                SkillsRootDoctorStatus {
                    kind: SkillsRootDoctorKind::Ok,
                    detail: format!("skills root {} ({} package(s))", root.display(), count),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_path_traversal_names() {
        assert!(validate_skill_name("../x").is_err());
        assert!(validate_skill_name("a/b").is_err());
        assert!(validate_skill_name("").is_err());
        assert!(validate_skill_name("good-skill_1").is_ok());
    }

    #[tokio::test]
    async fn create_list_load_improve() {
        let dir = tempfile::tempdir().unwrap();
        let tools = SkillsTools::new(
            dir.path().to_path_buf(),
            HashSet::from([
                "skills_list".into(),
                "skill_load".into(),
                "skill_manage".into(),
            ]),
        );
        let created = tools
            .invoke(ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "demo",
                    "content": "# Demo\nsteps\n"
                }),
            })
            .await
            .unwrap();
        assert!(created.summary.contains("create"));
        assert!(dir.path().join("demo").join("SKILL.md").is_file());

        let listed = tools
            .invoke(ToolCall {
                name: "skills_list".into(),
                arguments: json!({}),
            })
            .await
            .unwrap();
        assert!(listed.content.contains("demo"));

        let loaded = tools
            .invoke(ToolCall {
                name: "skill_load".into(),
                arguments: json!({ "name": "demo" }),
            })
            .await
            .unwrap();
        assert!(loaded.content.contains("steps"));

        let improved = tools
            .invoke(ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "improve",
                    "name": "demo",
                    "content": "# Demo\nimproved\n"
                }),
            })
            .await
            .unwrap();
        assert!(improved.summary.contains("improve"));
        assert!(
            std::fs::read_to_string(dir.path().join("demo").join("SKILL.md"))
                .unwrap()
                .contains("improved")
        );
    }

    #[tokio::test]
    async fn create_fails_when_exists_improve_fails_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tools = SkillsTools::new(
            dir.path().to_path_buf(),
            HashSet::from(["skill_manage".into()]),
        );
        tools
            .invoke(ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "x",
                    "content": "a"
                }),
            })
            .await
            .unwrap();
        let dup = tools
            .invoke(ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "create",
                    "name": "x",
                    "content": "b"
                }),
            })
            .await
            .unwrap_err();
        assert!(dup.to_string().contains("already exists"), "{dup}");

        let missing = tools
            .invoke(ToolCall {
                name: "skill_manage".into(),
                arguments: json!({
                    "action": "improve",
                    "name": "nope",
                    "content": "c"
                }),
            })
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("not found"), "{missing}");
    }
}
