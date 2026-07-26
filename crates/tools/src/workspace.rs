use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Workspace filesystem Tools with path confinement under allowlisted roots.
pub struct WorkspaceFsTools {
    roots: Vec<PathBuf>,
    allowed: HashSet<String>,
}

impl WorkspaceFsTools {
    #[must_use]
    pub fn new(roots: Vec<PathBuf>, allowed: HashSet<String>) -> Self {
        Self { roots, allowed }
    }
}

#[async_trait]
impl ToolRuntime for WorkspaceFsTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }

        match call.name.as_str() {
            "read_file" => {
                let path = arg_string(&call.arguments, "path")?;
                let resolved = resolve_in_workspace(&self.roots, &path)?;
                let content = tokio::fs::read_to_string(&resolved)
                    .await
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                let summary = format!(
                    "read_file path={} bytes={}",
                    sanitize_path_for_event(&path),
                    content.len()
                );
                Ok(ToolResult { content, summary })
            }
            "write_file" => {
                let path = arg_string(&call.arguments, "path")?;
                let content = arg_string(&call.arguments, "content")?;
                let resolved = resolve_in_workspace(&self.roots, &path)?;
                if let Some(parent) = resolved.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| ToolError::Failed(e.to_string()))?;
                }
                tokio::fs::write(&resolved, content.as_bytes())
                    .await
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                let summary = format!(
                    "write_file path={} bytes={}",
                    sanitize_path_for_event(&path),
                    content.len()
                );
                Ok(ToolResult {
                    content: format!("wrote {} bytes", content.len()),
                    summary,
                })
            }
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}

fn arg_string(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed(format!("missing string argument '{key}'")))
}

fn sanitize_path_for_event(path: &str) -> String {
    // Keep event payloads short; never dump file bodies here.
    if path.chars().count() > 64 {
        let truncated: String = path.chars().take(64).collect();
        format!("{truncated}…")
    } else {
        path.to_string()
    }
}

/// Resolve `user_path` under one of the allowlisted Workspace roots.
///
/// Denies absolute paths outside roots, `..` escape, and non-normalized escapes.
pub fn resolve_in_workspace(roots: &[PathBuf], user_path: &str) -> Result<PathBuf, ToolError> {
    if roots.is_empty() {
        return Err(ToolError::PathJail("no workspace roots configured".into()));
    }
    if user_path.is_empty() {
        return Err(ToolError::PathJail("empty path".into()));
    }

    let candidate = Path::new(user_path);
    // Reject null bytes and obvious escape attempts before join.
    if user_path.contains('\0') {
        return Err(ToolError::PathJail("invalid path".into()));
    }

    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|e| ToolError::PathJail(format!("workspace root unavailable: {e}")))?;

        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            // Manually reject `..` components that would leave the root before canonicalize.
            let mut base = root.clone();
            for comp in candidate.components() {
                match comp {
                    Component::ParentDir => {
                        if !base.starts_with(&root) || base == root {
                            return Err(ToolError::PathJail("path escapes workspace root".into()));
                        }
                        base.pop();
                    }
                    Component::CurDir => {}
                    Component::Normal(s) => base.push(s),
                    Component::RootDir | Component::Prefix(_) => {
                        return Err(ToolError::PathJail(
                            "absolute path components not allowed in relative path".into(),
                        ));
                    }
                }
            }
            base
        };

        // For absolute user paths, require they already sit under a root.
        let resolved = if joined.exists() {
            joined
                .canonicalize()
                .map_err(|e| ToolError::PathJail(e.to_string()))?
        } else {
            // Allow write to non-existent files: canonicalize parent, then rejoin name.
            let parent = joined
                .parent()
                .ok_or_else(|| ToolError::PathJail("path has no parent directory".into()))?;
            let file_name = joined
                .file_name()
                .ok_or_else(|| ToolError::PathJail("path has no file name".into()))?;
            if parent.as_os_str().is_empty() {
                return Err(ToolError::PathJail("invalid parent".into()));
            }
            let parent_canon = if parent.exists() {
                parent
                    .canonicalize()
                    .map_err(|e| ToolError::PathJail(e.to_string()))?
            } else if parent.starts_with(&root) {
                // Parent may not exist yet for nested writes; walk up to existing ancestor under root.
                let mut walk = parent.to_path_buf();
                let mut suffix = Vec::new();
                while !walk.exists() {
                    if walk == root {
                        break;
                    }
                    let name = walk
                        .file_name()
                        .ok_or_else(|| ToolError::PathJail("invalid path".into()))?
                        .to_os_string();
                    suffix.push(name);
                    if !walk.pop() {
                        break;
                    }
                }
                let mut base = walk
                    .canonicalize()
                    .map_err(|e| ToolError::PathJail(e.to_string()))?;
                for part in suffix.into_iter().rev() {
                    base.push(part);
                }
                base
            } else {
                return Err(ToolError::PathJail("path escapes workspace root".into()));
            };
            if !parent_canon.starts_with(&root) {
                return Err(ToolError::PathJail("path escapes workspace root".into()));
            }
            parent_canon.join(file_name)
        };

        if resolved.starts_with(&root) {
            return Ok(resolved);
        }
    }

    Err(ToolError::PathJail(
        "path is outside allowlisted workspace roots".into(),
    ))
}
