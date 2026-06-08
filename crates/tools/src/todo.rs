//! Todo list types + the TodoWrite tool.
//!
//! Mirrors Claude Code's TodoWriteTool — a thin tool whose value is the
//! prompt guidance. The model maintains a structured task list to plan and
//! track multi-step coding work; the list is stored per-thread and surfaced
//! to the UI via an engine event.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::ToolError;
use crate::{Tool, ToolContext};

/// Status of a single todo item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    /// Not yet started.
    Pending,
    /// Currently being worked on (at most one at a time).
    InProgress,
    /// Finished successfully.
    Completed,
}

/// A single task in the session todo list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// Imperative description ("Run tests").
    pub content: String,
    /// Present-continuous form shown while active ("Running tests").
    pub active_form: String,
    /// Current status.
    pub status: TodoStatus,
}

/// Shared per-thread todo list. Stored in `ToolContext` so the tool can
/// mutate it; read by the engine to emit update events.
pub type TodoList = Arc<Mutex<Vec<TodoItem>>>;

#[derive(Debug, Deserialize)]
struct TodoWriteArgs {
    todos: Vec<TodoItem>,
}

/// Replace the session todo list.
pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }
    fn is_read_only(&self) -> bool {
        // Not read-only in the filesystem sense, but it never touches disk and
        // is always safe to auto-allow. Keep false so it isn't batched as a
        // concurrent read, but it has no permission prompt (see check_permissions).
        false
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Cancel
    }

    async fn check_permissions(
        &self,
        _input: &Value,
        _mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        // Todo management never needs approval — it only edits in-memory state.
        crate::permission::PermissionResult::Allow {
            updated_input: _input.clone(),
            decision_reason: None,
            user_modified: None,
        }
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let todos = input.get("todos");
        if todos.is_none() || !todos.unwrap().is_array() {
            return Err("'todos' is required and must be an array".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let parsed: TodoWriteArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
                tool: self.name().into(),
                message: e.to_string(),
            })?;

        let count = parsed.todos.len();
        let in_progress = parsed
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::InProgress)
            .count();

        // Replace the shared list.
        {
            let mut list = ctx.todos.lock();
            *list = parsed.todos;
        }

        let mut msg = format!("Todo list updated ({count} item(s)).");
        if in_progress > 1 {
            msg.push_str(
                " Note: more than one task is in_progress — keep exactly one active at a time.",
            );
        }
        msg.push_str(" Continue using the todo list to track progress.");
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn todo_item_camelcase_roundtrip() {
        let item = TodoItem {
            content: "Run tests".into(),
            active_form: "Running tests".into(),
            status: TodoStatus::InProgress,
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["content"], "Run tests");
        assert_eq!(v["activeForm"], "Running tests");
        assert_eq!(v["status"], "in_progress");
        let back: TodoItem = serde_json::from_value(v).unwrap();
        assert_eq!(back, item);
    }

    #[tokio::test]
    async fn todo_write_updates_context_list() {
        let ctx = ToolContext::standalone();
        let out = TodoWriteTool
            .execute(
                json!({
                    "todos": [
                        {"content": "A", "activeForm": "Doing A", "status": "in_progress"},
                        {"content": "B", "activeForm": "Doing B", "status": "pending"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(out.contains("updated"), "got: {out}");
        let list = ctx.todos.lock();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn todo_write_warns_multiple_in_progress() {
        let ctx = ToolContext::standalone();
        let out = TodoWriteTool
            .execute(
                json!({
                    "todos": [
                        {"content": "A", "activeForm": "Doing A", "status": "in_progress"},
                        {"content": "B", "activeForm": "Doing B", "status": "in_progress"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(out.contains("exactly one"), "got: {out}");
    }

    #[tokio::test]
    async fn todo_write_validate_requires_array() {
        let r = TodoWriteTool
            .validate_input(&json!({"todos": "not array"}))
            .await;
        assert!(r.is_err());
    }
}
