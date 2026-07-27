use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// Max directory depth for `search_files` (relative to the start path).
const SEARCH_MAX_DEPTH: usize = 32;
/// Max filesystem entries visited per `search_files` call (dirs + files).
const SEARCH_MAX_ENTRIES: usize = 10_000;
/// Max file size fully loaded by `apply_patch` / content search.
const MAX_TOOL_FILE_BYTES: u64 = 1_000_000;

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
            "apply_patch" => self.apply_patch(&call.arguments).await,
            "search_files" => self.search_files(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        [
            ("read_file", "Read a file under workspace roots"),
            ("write_file", "Write a file under workspace roots"),
            ("apply_patch", "Apply a unified patch under workspace roots"),
            ("search_files", "Search files under workspace roots"),
        ]
        .into_iter()
        .filter(|(name, _)| self.allowed.contains(*name))
        .map(|(name, desc)| ToolSpec::empty_params(name, desc))
        .collect()
    }
}

impl WorkspaceFsTools {
    /// Apply a precise in-file edit under Workspace roots (no whole-file rewrite required).
    ///
    /// Arguments:
    /// - `path` (required): workspace-relative or in-root absolute path
    /// - `old_string` (required): exact text to find
    /// - `new_string` (required): replacement text (may be empty to delete)
    /// - `replace_all` (optional bool, default false): replace every occurrence
    async fn apply_patch(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let path = arg_string(args, "path")?;
        let old_string = arg_string(args, "old_string")?;
        let new_string = arg_string(args, "new_string")?;
        let replace_all = args
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if old_string.is_empty() {
            return Err(ToolError::Failed(
                "apply_patch: old_string must not be empty".into(),
            ));
        }

        let resolved = resolve_in_workspace(&self.roots, &path)?;
        // Re-check containment after resolve (mitigate symlink swap before open).
        let resolved = revalidate_under_roots(&self.roots, &resolved)?;
        let meta = tokio::fs::metadata(&resolved)
            .await
            .map_err(|e| ToolError::Failed(format!("apply_patch stat: {e}")))?;
        if meta.len() > MAX_TOOL_FILE_BYTES {
            return Err(ToolError::Failed(format!(
                "apply_patch: file exceeds {MAX_TOOL_FILE_BYTES} byte limit"
            )));
        }
        let original = tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| ToolError::Failed(format!("apply_patch read: {e}")))?;

        let occurrences = original.matches(&old_string).count();
        if occurrences == 0 {
            return Err(ToolError::Failed(
                "apply_patch: old_string not found in file".into(),
            ));
        }
        if !replace_all && occurrences > 1 {
            return Err(ToolError::Failed(format!(
                "apply_patch: old_string matches {occurrences} times; set replace_all=true or use a more specific old_string"
            )));
        }

        let updated = if replace_all {
            original.replace(&old_string, &new_string)
        } else {
            original.replacen(&old_string, &new_string, 1)
        };

        // Re-validate path before write (symlink swap between read and write).
        let resolved = revalidate_under_roots(&self.roots, &resolved)?;
        tokio::fs::write(&resolved, updated.as_bytes())
            .await
            .map_err(|e| ToolError::Failed(format!("apply_patch write: {e}")))?;

        let replaced = if replace_all { occurrences } else { 1 };
        let summary = format!(
            "apply_patch path={} replaced={} bytes={}",
            sanitize_path_for_event(&path),
            replaced,
            updated.len()
        );
        Ok(ToolResult {
            content: format!("patched {path}: {replaced} replacement(s)"),
            summary,
        })
    }

    /// Search file names and contents under Workspace roots only.
    ///
    /// Arguments:
    /// - `query` (required): substring match (case-sensitive) in path and/or content
    /// - `path` (optional): subdirectory relative to a workspace root
    /// - `max_results` (optional, default 20, cap 100)
    async fn search_files(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let query = arg_string(args, "query")?;
        if query.is_empty() {
            return Err(ToolError::Failed(
                "search_files: query must not be empty".into(),
            ));
        }
        let subpath = args
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let max_results = args
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 100) as usize;

        let roots = self.roots.clone();
        let query_owned = query.clone();
        let subpath_owned = subpath.clone();

        // Walk filesystem off the async runtime's hot path.
        let hits = tokio::task::spawn_blocking(move || {
            search_under_roots(&roots, &subpath_owned, &query_owned, max_results)
        })
        .await
        .map_err(|e| ToolError::Failed(format!("search_files join: {e}")))??;

        let count = hits.len();
        let content = if hits.is_empty() {
            format!("no matches for {query:?}")
        } else {
            hits.join("\n")
        };
        let summary = format!(
            "search_files query={} hits={} path={}",
            truncate_for_event(&query, 40),
            count,
            sanitize_path_for_event(if subpath.is_empty() { "." } else { &subpath })
        );
        Ok(ToolResult { content, summary })
    }
}

fn search_under_roots(
    roots: &[PathBuf],
    subpath: &str,
    query: &str,
    max_results: usize,
) -> Result<Vec<String>, ToolError> {
    let mut hits = Vec::new();
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut entries_scanned: usize = 0;
    for root in roots {
        if hits.len() >= max_results || entries_scanned >= SEARCH_MAX_ENTRIES {
            break;
        }
        let root_canon = root
            .canonicalize()
            .map_err(|e| ToolError::PathJail(format!("workspace root unavailable: {e}")))?;

        let start = if subpath.is_empty() {
            root_canon.clone()
        } else {
            resolve_in_workspace(std::slice::from_ref(&root_canon), subpath)?
        };

        if !start.exists() {
            continue;
        }
        // Always walk by canonical directory identity (breaks symlink re-entry loops).
        let start_canon = start
            .canonicalize()
            .map_err(|e| ToolError::PathJail(e.to_string()))?;
        if !start_canon.starts_with(&root_canon) {
            return Err(ToolError::PathJail(
                "search start path escapes workspace root".into(),
            ));
        }
        walk_search(
            &root_canon,
            &start_canon,
            query,
            max_results,
            0,
            &mut visited_dirs,
            &mut entries_scanned,
            &mut hits,
        )?;
    }
    Ok(hits)
}

#[allow(clippy::too_many_arguments)]
fn walk_search(
    root: &Path,
    dir_canon: &Path,
    query: &str,
    max_results: usize,
    depth: usize,
    visited_dirs: &mut HashSet<PathBuf>,
    entries_scanned: &mut usize,
    hits: &mut Vec<String>,
) -> Result<(), ToolError> {
    if hits.len() >= max_results
        || *entries_scanned >= SEARCH_MAX_ENTRIES
        || depth > SEARCH_MAX_DEPTH
    {
        return Ok(());
    }
    if !dir_canon.starts_with(root) {
        return Ok(());
    }
    // Symlink loops: same canonical directory must not be re-entered.
    if !visited_dirs.insert(dir_canon.to_path_buf()) {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir_canon).map_err(|e| ToolError::Failed(e.to_string()))?;
    for entry in entries {
        if hits.len() >= max_results || *entries_scanned >= SEARCH_MAX_ENTRIES {
            break;
        }
        *entries_scanned += 1;
        let entry = entry.map_err(|e| ToolError::Failed(e.to_string()))?;
        let path = entry.path();
        // Stay inside root (symlink escape defense).
        let canon = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !canon.starts_with(root) {
            continue;
        }
        let rel = canon
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| canon.display().to_string());

        let name_hit = rel.contains(query)
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(query));

        // Prefer symlink_metadata so we classify the entry itself.
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if ft.is_dir() || (ft.is_symlink() && canon.is_dir()) {
            if name_hit {
                hits.push(format!("{rel}/ [dir name match]"));
            }
            // Recurse on canonical path only (visited set prevents loops).
            walk_search(
                root,
                &canon,
                query,
                max_results,
                depth + 1,
                visited_dirs,
                entries_scanned,
                hits,
            )?;
            continue;
        }

        // Skip very large files for content scan.
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > MAX_TOOL_FILE_BYTES {
            if name_hit {
                hits.push(format!("{rel} [name match; content skipped size]"));
            }
            continue;
        }

        // Read via canonical path after re-check (reduces symlink-swap window).
        let content = match std::fs::read_to_string(&canon) {
            Ok(c) => c,
            Err(_) => {
                if name_hit {
                    hits.push(format!("{rel} [name match; binary/unreadable]"));
                }
                continue;
            }
        };

        if name_hit {
            hits.push(format!("{rel} [name match]"));
            if hits.len() >= max_results {
                break;
            }
        }

        for (idx, line) in content.lines().enumerate() {
            if hits.len() >= max_results {
                break;
            }
            if line.contains(query) {
                let line_no = idx + 1;
                let snippet = truncate_for_event(line.trim(), 120);
                hits.push(format!("{rel}:{line_no}: {snippet}"));
            }
        }
    }
    Ok(())
}

/// Re-canonicalize and require the path still sits under an allowlisted root.
fn revalidate_under_roots(roots: &[PathBuf], path: &Path) -> Result<PathBuf, ToolError> {
    let canon = if path.exists() {
        path.canonicalize()
            .map_err(|e| ToolError::PathJail(e.to_string()))?
    } else {
        return Err(ToolError::PathJail(
            "path disappeared before use (possible symlink race)".into(),
        ));
    };
    for root in roots {
        let root = root
            .canonicalize()
            .map_err(|e| ToolError::PathJail(format!("workspace root unavailable: {e}")))?;
        if canon.starts_with(&root) {
            return Ok(canon);
        }
    }
    Err(ToolError::PathJail(
        "path is outside allowlisted workspace roots".into(),
    ))
}

fn truncate_for_event(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
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
