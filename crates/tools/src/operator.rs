//! todo + clarify operator UX tools.

use async_trait::async_trait;
use keryx_app::{ToolCall, ToolError, ToolResult, ToolRuntime};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Mutex;
use uuid::Uuid;

/// In-run checklist state.
#[derive(Debug, Default)]
pub struct TodoState {
    items: Mutex<Vec<(String, bool)>>,
}

impl TodoState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> String {
        let items = self.items.lock().map(|i| i.clone()).unwrap_or_default();
        if items.is_empty() {
            return "(empty todo)".into();
        }
        items
            .iter()
            .enumerate()
            .map(|(i, (t, done))| format!("{}. [{}] {t}", i + 1, if *done { "x" } else { " " }))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Pending clarify questions for control-plane answer.
#[derive(Debug, Default)]
pub struct ClarifyQueue {
    pending: Mutex<Vec<ClarifyRequest>>,
}

#[derive(Debug, Clone)]
pub struct ClarifyRequest {
    pub id: String,
    pub question: String,
    pub answer: Option<String>,
}

impl ClarifyQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ask(&self, question: String) -> String {
        let id = Uuid::new_v4().to_string();
        if let Ok(mut p) = self.pending.lock() {
            p.push(ClarifyRequest {
                id: id.clone(),
                question,
                answer: None,
            });
        }
        id
    }

    pub fn answer(&self, id: &str, answer: String) -> Result<(), String> {
        let mut p = self.pending.lock().map_err(|e| e.to_string())?;
        let item = p
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| "clarify id not found".to_string())?;
        item.answer = Some(answer);
        Ok(())
    }

    pub fn list_pending(&self) -> Vec<ClarifyRequest> {
        self.pending
            .lock()
            .map(|p| p.iter().filter(|c| c.answer.is_none()).cloned().collect())
            .unwrap_or_default()
    }

    pub fn take_answer(&self, id: &str) -> Option<String> {
        self.pending
            .lock()
            .ok()
            .and_then(|p| p.iter().find(|c| c.id == id).and_then(|c| c.answer.clone()))
    }
}

pub struct OperatorTools {
    allowed: HashSet<String>,
    todos: std::sync::Arc<TodoState>,
    clarify: std::sync::Arc<ClarifyQueue>,
}

impl OperatorTools {
    #[must_use]
    pub fn new(
        allowed: HashSet<String>,
        todos: std::sync::Arc<TodoState>,
        clarify: std::sync::Arc<ClarifyQueue>,
    ) -> Self {
        Self {
            allowed,
            todos,
            clarify,
        }
    }
}

#[async_trait]
impl ToolRuntime for OperatorTools {
    async fn invoke(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if !self.allowed.contains(&call.name) {
            return Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{}'",
                call.name
            )));
        }
        match call.name.as_str() {
            "todo" => self.todo(&call.arguments).await,
            "clarify" => self.clarify_tool(&call.arguments).await,
            other => Err(ToolError::Denied(format!(
                "unknown or disallowed tool '{other}'"
            ))),
        }
    }
}

impl OperatorTools {
    async fn todo(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let action = args.get("action").and_then(Value::as_str).unwrap_or("list");
        match action {
            "add" => {
                let item = args
                    .get("item")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Failed("missing item".into()))?;
                if let Ok(mut items) = self.todos.items.lock() {
                    items.push((item.to_string(), false));
                }
            }
            "done" => {
                let idx = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Ok(mut items) = self.todos.items.lock() {
                    if let Some((_, done)) = items.get_mut(idx.saturating_sub(1)) {
                        *done = true;
                    }
                }
            }
            "list" => {}
            other => {
                return Err(ToolError::Failed(format!("unknown todo action '{other}'")));
            }
        }
        let snap = self.todos.snapshot();
        Ok(ToolResult {
            content: snap.clone(),
            summary: format!("todo action={action}"),
        })
    }

    async fn clarify_tool(&self, args: &Value) -> Result<ToolResult, ToolError> {
        let question = args
            .get("question")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Failed("missing question".into()))?;
        let id = self.clarify.ask(question.to_string());
        // Wait briefly for operator answer (Seam 1 can answer via queue).
        for _ in 0..50 {
            if let Some(ans) = self.clarify.take_answer(&id) {
                return Ok(ToolResult {
                    content: format!("clarify answered: {ans}"),
                    summary: format!("clarify id={id} status=answered"),
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        // Pause signal: unanswered — fail closed with observable id for API.
        Ok(ToolResult {
            content: format!("clarify waiting id={id} question={question}"),
            summary: format!("clarify id={id} status=waiting"),
        })
    }
}
