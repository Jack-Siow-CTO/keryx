//! Soul and workspace Context file loading for Runs.
//!
//! Soul is operator personality/standing instructions.
//! Context files are workspace-scoped project norms.
//! Neither is Memory (curated facts) nor Skill (versioned packages).

use std::path::{Component, Path, PathBuf};

/// How missing Soul / Context files are handled.
///
/// Documented default: **soft** — continue the Run without attachment and record a
/// short system note (or skip silently when path is unset). Never hard-fail a Run
/// solely because Soul is absent (operators may omit Soul during early setup).
///
/// `Closed` is reserved for operator tooling (e.g. doctor) that should **warn/fail
/// config checks** when a configured path is missing; the agent loop still uses Soft
/// so Runs are not blocked solely by an absent Soul document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingContextPolicy {
    /// Continue without the document; optional soft note for observability.
    #[default]
    Soft,
    /// For config checks only: treat missing configured path as a configuration problem.
    Closed,
}

/// Configuration for attaching Soul + Context files to Runs.
#[derive(Debug, Clone, Default)]
pub struct RunContextConfig {
    /// Absolute or process-relative path to the operator Soul document.
    pub soul_path: Option<PathBuf>,
    /// Workspace-relative context file paths (resolved under workspace roots by the loader).
    pub context_files: Vec<String>,
    /// Workspace roots used to resolve context file paths (path jail).
    pub workspace_roots: Vec<PathBuf>,
    pub missing: MissingContextPolicy,
}

/// Loaded attachments ready to inject as system Transcript messages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadedRunContext {
    pub messages: Vec<keryx_domain::TranscriptMessage>,
    /// Canonical (when possible) paths that must not be agent-written without Approval.
    pub protected_paths: Vec<PathBuf>,
}

/// Load Soul and Context files from disk (sync I/O; call from blocking or startup).
///
/// Missing Soul/context under [`MissingContextPolicy::Soft`]: skip or attach a brief note.
/// Distinct from Memory/Skill loaders (not implemented here).
#[must_use]
pub fn load_run_context(config: &RunContextConfig) -> LoadedRunContext {
    let mut out = LoadedRunContext::default();

    if let Some(soul_path) = &config.soul_path {
        // Always protect the configured Soul path (even if empty/missing).
        push_protected(&mut out.protected_paths, soul_path);
        match std::fs::read_to_string(soul_path) {
            Ok(content) if !content.trim().is_empty() => {
                out.messages
                    .push(keryx_domain::TranscriptMessage::system(format!(
                        "[Soul]\n{content}"
                    )));
            }
            Ok(_) | Err(_) => {
                if matches!(config.missing, MissingContextPolicy::Soft) {
                    out.messages.push(keryx_domain::TranscriptMessage::system(
                        "[Soul]\n(not loaded: missing or empty — continuing without Soul)"
                            .to_string(),
                    ));
                }
            }
        }
    }

    for rel in &config.context_files {
        match resolve_context_path_jailed(&config.workspace_roots, rel) {
            Ok(abs) => {
                // Protect even when empty/missing after jail resolve.
                push_protected(&mut out.protected_paths, &abs);
                match std::fs::read_to_string(&abs) {
                    Ok(content) if !content.trim().is_empty() => {
                        out.messages
                            .push(keryx_domain::TranscriptMessage::system(format!(
                                "[Context file: {rel}]\n{content}"
                            )));
                    }
                    Ok(_) | Err(_) => {
                        if matches!(config.missing, MissingContextPolicy::Soft) {
                            out.messages
                                .push(keryx_domain::TranscriptMessage::system(format!(
                                    "[Context file: {rel}]\n(not loaded: missing or empty)"
                                )));
                        }
                    }
                }
            }
            Err(_) => {
                if matches!(config.missing, MissingContextPolicy::Soft) {
                    out.messages
                        .push(keryx_domain::TranscriptMessage::system(format!(
                            "[Context file: {rel}]\n(not loaded: path outside workspace or invalid)"
                        )));
                }
            }
        }
    }

    out
}

fn push_protected(out: &mut Vec<PathBuf>, path: &Path) {
    if let Ok(canon) = path.canonicalize() {
        out.push(canon);
    } else {
        out.push(path.to_path_buf());
    }
}

/// Resolve a Context file path under workspace roots with fail-closed path jail.
///
/// Rejects empty paths, NUL, `..` escape, and paths whose canonicalize leaves roots.
/// Never falls back to a non-jailed path after a failed containment check.
pub fn resolve_context_path_jailed(roots: &[PathBuf], user_path: &str) -> Result<PathBuf, String> {
    if roots.is_empty() {
        return Err("no workspace roots".into());
    }
    if user_path.is_empty() || user_path.contains('\0') {
        return Err("invalid context path".into());
    }

    let candidate = Path::new(user_path);
    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|e| format!("workspace root unavailable: {e}"))?;

        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            let mut base = root.clone();
            for comp in candidate.components() {
                match comp {
                    Component::ParentDir => {
                        if !base.starts_with(&root) || base == root {
                            return Err("context path escapes workspace root".into());
                        }
                        base.pop();
                    }
                    Component::CurDir => {}
                    Component::Normal(s) => base.push(s),
                    Component::RootDir | Component::Prefix(_) => {
                        return Err(
                            "absolute components not allowed in relative context path".into()
                        );
                    }
                }
            }
            base
        };

        let resolved = if joined.exists() {
            joined
                .canonicalize()
                .map_err(|e| format!("context path resolve failed: {e}"))?
        } else {
            // Non-existent: canonicalize parent when possible, keep file name.
            let parent = joined
                .parent()
                .ok_or_else(|| "context path has no parent".to_string())?;
            let name = joined
                .file_name()
                .ok_or_else(|| "context path has no file name".to_string())?;
            let parent_canon = if parent.exists() {
                parent
                    .canonicalize()
                    .map_err(|e| format!("context parent resolve failed: {e}"))?
            } else if parent.starts_with(&root) {
                parent.to_path_buf()
            } else {
                return Err("context path escapes workspace root".into());
            };
            if !parent_canon.starts_with(&root) {
                return Err("context path escapes workspace root".into());
            }
            parent_canon.join(name)
        };

        if resolved.starts_with(&root) {
            return Ok(resolved);
        }
    }

    Err("context path is outside allowlisted workspace roots".into())
}

/// True if `tool_path` (workspace-relative or absolute) targets a protected Soul/Context path.
#[must_use]
pub fn path_targets_protected(
    tool_path: &str,
    workspace_roots: &[PathBuf],
    protected: &[PathBuf],
) -> bool {
    if protected.is_empty() || tool_path.is_empty() {
        return false;
    }

    // 1) Resolve under workspace when possible; compare canonical paths case-insensitively.
    if let Ok(resolved) = resolve_context_path_jailed(workspace_roots, tool_path) {
        let resolved_key = path_key(&resolved);
        if protected.iter().any(|p| path_key(p) == resolved_key) {
            return true;
        }
        // Same basename as a protected identity file (case-insensitive) under the workspace.
        if let Some(name) = resolved.file_name().and_then(|n| n.to_str()) {
            let name_key = name.to_ascii_lowercase();
            if protected.iter().any(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase() == name_key)
            }) {
                return true;
            }
        }
    }

    // 2) Absolute tool path equal to protected Soul (Soul may live outside workspace roots).
    let abs = PathBuf::from(tool_path);
    if abs.is_absolute() {
        let key = abs
            .canonicalize()
            .map(|p| path_key(&p))
            .unwrap_or_else(|_| path_key(&abs));
        if protected.iter().any(|p| path_key(p) == key) {
            return true;
        }
    }

    // 3) Basename-only match against protected file names (case-insensitive).
    if let Some(name) = Path::new(tool_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
    {
        if protected.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_ascii_lowercase() == name)
        }) {
            return true;
        }
    }

    false
}

fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy();
    // Case-insensitive compare for APFS/Windows; Linux still fine for ASCII paths.
    s.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_soul_and_context_soft_missing() {
        let dir = tempfile::tempdir().unwrap();
        let soul = dir.path().join("SOUL.md");
        std::fs::write(&soul, "Be concise.").unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("CONTEXT.md"), "Use tabs.").unwrap();

        let loaded = load_run_context(&RunContextConfig {
            soul_path: Some(soul),
            context_files: vec!["CONTEXT.md".into(), "missing.md".into()],
            workspace_roots: vec![ws],
            missing: MissingContextPolicy::Soft,
        });
        assert_eq!(loaded.messages.len(), 3);
        assert!(loaded.messages[0].content.contains("[Soul]"));
        assert!(loaded.messages[0].content.contains("Be concise."));
        assert!(loaded.messages[1].content.contains("Use tabs."));
        assert!(loaded.messages[2].content.contains("not loaded"));
        assert!(!loaded.protected_paths.is_empty());
    }

    #[test]
    fn context_path_jail_denies_parent_escape() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(dir.path().join("secret.md"), "nope").unwrap();
        let err = resolve_context_path_jailed(&[ws], "../secret.md").unwrap_err();
        assert!(err.contains("escapes") || err.contains("outside"), "{err}");
    }

    #[test]
    fn path_targets_protected_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let ctx = ws.join("CONTEXT.md");
        std::fs::write(&ctx, "rules").unwrap();
        let protected = vec![ctx.canonicalize().unwrap()];
        assert!(path_targets_protected(
            "CONTEXT.md",
            std::slice::from_ref(&ws),
            &protected
        ));
        assert!(path_targets_protected(
            "context.md",
            std::slice::from_ref(&ws),
            &protected
        ));
        assert!(!path_targets_protected("other.md", &[ws], &protected));
    }
}
