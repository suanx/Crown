//! Per-turn tool execution context.
//!
//! Mirrors Claude Code's `ToolUseContext`. Carries the shared file-state
//! cache (read-before-write enforcement), the working directory, and the
//! abort token. Handed by reference to every `Tool::execute` call.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ask_user_question::QuestionGate;
use crate::file_state::FileStateCache;

/// Per-turn file-change tracker handle. Write/Edit tools call
/// `record_create` / `record_modify` / `record_delete` so the engine can
/// emit a structured diff summary at turn end. (Defined as a trait so
/// tools don't need to depend on the `core` crate.)
pub trait TurnDiffRecorder: Send + Sync {
    fn record_create(&self, path: &str);
    fn record_modify(&self, path: &str);
    fn record_delete(&self, path: &str);
}

/// Sink for recording a file's pre-change content (rewind support, P2). The
/// engine/app injects a SQLite-backed impl; tests use a no-op or in-memory
/// one. Defined here (tools crate) so tools don't depend on the state crate.
pub trait FileHistorySink: Send + Sync {
    /// Record that `path` (within `thread_id` at `message_seq`) is about to
    /// change; `before` is its current content, or `None` if it doesn't exist.
    fn record(&self, thread_id: &str, message_seq: i64, path: &str, before: Option<String>);
}

/// Launches a sub-agent run (the `task` tool delegates here). Implemented by
/// the app layer (which owns the engine + Tauri event sink); the trait lives
/// here so `tools` need not depend on `core`. The launcher itself handles
/// streaming sub-agent activity to the UI; this call just returns the final
/// report text plus an optional resumable sub-agent id.
#[async_trait::async_trait]
pub trait SubagentLauncher: Send + Sync {
    /// Run (or resume) a sub-agent. Returns `(report_text, resumable_id)`.
    /// `resumable_id` is `None` for one-shot agent types.
    async fn launch(
        &self,
        agent_type: String,
        prompt: String,
        resume_subagent_id: Option<String>,
        parent_thread_id: String,
        parent_abort: CancellationToken,
    ) -> Result<(String, Option<String>), String>;
}

/// Execution context passed to every tool invocation.
#[derive(Clone)]
pub struct ToolContext {
    /// Shared read-before-write file state. `Arc<Mutex>` because read-only
    /// tools may run concurrently and all consult the same cache.
    pub file_state: Arc<Mutex<FileStateCache>>,
    /// Working directory for the owning thread (resolves relative paths).
    pub cwd: Option<PathBuf>,
    /// Cancellation token for the current turn.
    pub abort: CancellationToken,
    /// Shared per-thread todo list (TodoWrite tool ↔ engine event).
    pub todos: crate::todo::TodoList,
    /// Owning thread id (rewind file-history attribution). `None` outside the engine.
    pub thread_id: Option<String>,
    /// Triggering user-message seq this turn's writes attribute to. `None` outside the engine.
    pub message_seq: Option<i64>,
    /// Sink for pre-change file snapshots. `None` = rewind disabled (tests/standalone).
    pub file_history: Option<Arc<dyn FileHistorySink>>,
    /// Sub-agent launcher (the `task` tool delegates here). `None` outside the engine.
    pub subagent: Option<Arc<dyn SubagentLauncher>>,
    /// 结构化问答 gate（`ask_user_question` 工具委托此处阻塞等待用户答复）。
    /// `None` = 无前端（tests/standalone），工具将拒绝执行。
    pub question_gate: Option<Arc<dyn QuestionGate>>,
    /// 当前正在执行的 `tool_use` id（`ask_user_question` 用它做 gate parking）。
    /// `None` outside the engine.
    pub current_tool_use_id: Option<String>,
    /// Per-turn file-change recorder (Codex-aligned turn diff tracker).
    /// Write/Edit tools call `record_*` on this; the engine logs a diff
    /// summary at turn end. `None` = tracking disabled (tests/standalone).
    pub turn_diff: Option<Arc<dyn TurnDiffRecorder>>,
}

impl ToolContext {
    /// Build a standalone context with an empty file-state cache, no cwd,
    /// and a fresh (uncancelled) token. For tests and direct tool invocation
    /// outside the engine.
    pub fn standalone() -> Self {
        Self {
            file_state: Arc::new(Mutex::new(FileStateCache::new())),
            cwd: None,
            abort: CancellationToken::new(),
            todos: Arc::new(Mutex::new(Vec::new())),
            thread_id: None,
            message_seq: None,
            file_history: None,
            subagent: None,
            question_gate: None,
            current_tool_use_id: None,
            turn_diff: None,
        }
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::standalone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_has_no_question_gate() {
        let ctx = ToolContext::standalone();
        assert!(ctx.question_gate.is_none());
        assert!(ctx.current_tool_use_id.is_none());
    }
}
