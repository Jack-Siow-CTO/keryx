//! Memory and session_search Tools (store-backed).

use async_trait::async_trait;
use keryx_app::{SessionStore, ToolCall, ToolError, ToolResult, ToolRuntime, ToolSpec};
use keryx_domain::{MemoryEntry, MemoryId, PrincipalId, RunId};
use serde_json::Value;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

/// Policy-gated Memory + session_search tools.
pub struct MemoryTools<S> {
    store: Arc<S>,
    allowed: HashSet<String>,
    /// Optional provenance attached on writes.
    run_id: Option<RunId>,
    principal_id: Option<PrincipalId>,
}

impl<S> MemoryTools<S> {
    #[must_use]
    pub fn new(store: Arc<S>, allowed: HashSet<String>) -> Self {
        Self {
            store,
            allowed,
            run_id: None,
            principal_id: None,
        }
    }

    #[must_use]
    pub fn with_provenance(
        mut self,
        run_id: Option<RunId>,
        principal_id: Option<PrincipalId>,
    ) -> Self {
        self.run_id = run_id;
        self.principal_id = principal_id;
        self
    }
}

#[async_trait]
impl<S> ToolRuntime for MemoryTools<S>
where
    S: SessionStore + 'static,
{
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "memory_write" => self.memory_write(&call.arguments).await,
            "memory_update" => self.memory_update(&call.arguments).await,
            "memory_delete" => self.memory_delete(&call.arguments).await,
            "memory_read" => self.memory_read(&call.arguments).await,
            "memory_search" => self.memory_search(&call.arguments).await,
            "session_search" => self.session_search(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }

    fn catalog(&self) -> Vec<ToolSpec> {
        [
            ("memory_read", "Read a Memory entry by id"),
            ("memory_write", "Write a new Memory entry"),
            ("memory_update", "Update a Memory entry"),
            ("memory_delete", "Delete a Memory entry"),
            ("memory_search", "Search Memory entries"),
            ("session_search", "Search Session transcripts"),
        ]
        .into_iter()
        .filter(|(name, _)| self.allowed.contains(*name))
        .map(|(name, desc)| ToolSpec::empty_params(name, desc))
        .collect()
    }
}

impl<S> MemoryTools<S>
where
    S: SessionStore,
{
    async fn memory_write(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let content = arg_string(args, "content")?;
        if content.trim().is_empty() {
            return Err(ToolError::Failed("memory_write: content empty".into()));
        }
        let label = args
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut entry = MemoryEntry::new(content);
        entry.label = label;
        entry.source_run_id = self.run_id;
        entry.source_principal_id = self.principal_id.clone();
        let id = entry.id;
        self.store
            .create_memory(entry)
            .await
            .map_err(|e| ToolError::Failed(e))?;
        Ok(ToolResult {
            content: format!("memory stored id={id}"),
            summary: format!("memory_write id={id}"),
        })
    }

    async fn memory_update(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let id = MemoryId::from_str(&arg_string(args, "id")?)
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        let mut entry = self
            .store
            .get_memory(id)
            .await
            .map_err(|e| ToolError::Failed(e))?
            .ok_or_else(|| ToolError::Failed(format!("memory {id} not found")))?;
        if let Some(content) = args.get("content").and_then(Value::as_str) {
            if content.trim().is_empty() {
                return Err(ToolError::Failed("memory_update: content empty".into()));
            }
            entry.content = content.to_string();
        }
        if let Some(label) = args.get("label").and_then(Value::as_str) {
            entry.label = Some(label.to_string());
        }
        self.store
            .update_memory(entry)
            .await
            .map_err(|e| ToolError::Failed(e))?;
        Ok(ToolResult {
            content: format!("memory updated id={id}"),
            summary: format!("memory_update id={id}"),
        })
    }

    async fn memory_delete(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let id = MemoryId::from_str(&arg_string(args, "id")?)
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        self.store
            .delete_memory(id)
            .await
            .map_err(|e| ToolError::Failed(e))?;
        Ok(ToolResult {
            content: format!("memory deleted id={id}"),
            summary: format!("memory_delete id={id}"),
        })
    }

    async fn memory_read(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let id = MemoryId::from_str(&arg_string(args, "id")?)
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        let entry = self
            .store
            .get_memory(id)
            .await
            .map_err(|e| ToolError::Failed(e))?
            .ok_or_else(|| ToolError::Failed(format!("memory {id} not found")))?;
        let body = format!(
            "id={} label={:?}\n{}",
            entry.id,
            entry.label,
            entry.content
        );
        Ok(ToolResult {
            summary: format!("memory_read id={} chars={}", entry.id, entry.content.len()),
            content: body,
        })
    }

    async fn memory_search(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let query = arg_string(args, "query")?;
        let limit = args
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(10)
            .clamp(1, 50) as usize;
        let hits = self
            .store
            .search_memory(&query, limit)
            .await
            .map_err(|e| ToolError::Failed(e))?;
        let content = if hits.is_empty() {
            format!("no memory matches for {query:?}")
        } else {
            hits.iter()
                .map(|e| {
                    format!(
                        "- {} [{}]: {}",
                        e.id,
                        e.label.clone().unwrap_or_default(),
                        truncate(&e.content, 200)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolResult {
            summary: format!("memory_search hits={}", hits.len()),
            content,
        })
    }

    async fn session_search(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let query = arg_string(args, "query")?;
        let limit = args
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(10)
            .clamp(1, 50) as usize;
        let hits = self
            .store
            .search_transcripts(&query, limit)
            .await
            .map_err(|e| ToolError::Failed(e))?;
        let content = if hits.is_empty() {
            format!("no transcript matches for {query:?}")
        } else {
            hits.iter()
                .map(|(sid, msg)| {
                    format!(
                        "- session={} role={:?}: {}",
                        sid,
                        msg.role,
                        truncate(&msg.content, 200)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolResult {
            summary: format!("session_search hits={}", hits.len()),
            content,
        })
    }
}

fn arg_string(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::Failed(format!("missing string argument '{key}'")))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}
