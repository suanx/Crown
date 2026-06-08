//! Crown tool subsystem.
//!
//! This crate defines the [`Tool`] trait that every executable tool must
//! implement, the shared data types in [`types`] (tool calls, results and
//! errors), and the [`registry::ToolRegistry`] used by the agent engine to
//! dispatch calls by name.
//!
//! Concrete tool implementations live in [`filesystem`], [`shell`], and
//! [`specs`]; they are added in subsequent tasks of the P2 plan and are kept
//! as empty modules here so that downstream code can already import the
//! crate and the trait surface is stable.

pub mod ask_user_question;
pub mod edit_match;
pub mod file_rule_matching;
pub mod file_state;
pub mod filesystem;
pub mod glob_tool;
pub mod grep;
pub mod permission;
pub mod registry;
pub mod rule_parser;
pub mod safety;
pub mod shell;
pub mod shell_rule_matching;
pub mod skill_tool;
pub mod specs;
pub mod task_tool;
pub mod todo;
pub mod types;
pub mod web;

pub mod context;

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

pub use context::{FileHistorySink, SubagentLauncher, ToolContext, TurnDiffRecorder};
pub use registry::ToolRegistry;
pub use types::{ToolCall, ToolError, ToolResult};

pub use ask_user_question::{
    AnswerItem, AskUserQuestionTool, Question, QuestionAnswers, QuestionGate, QuestionGateError,
    QuestionOption, QuestionOutcome, QuestionRequest,
};

/// Contract implemented by every tool the agent can invoke.
///
/// Implementations are expected to be cheap to clone or wrap in [`std::sync::Arc`];
/// the registry stores them as trait objects and may dispatch calls
/// concurrently for read-only tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Stable identifier exposed to the model in tool specs. Names should be
    /// unique within a [`ToolRegistry`] and stable across releases since the
    /// model uses them verbatim when issuing calls.
    fn name(&self) -> &str;

    /// `true` when the tool only observes the environment and never mutates
    /// state. Read-only tools are eligible for parallel dispatch and skip the
    /// approval flow.
    fn is_read_only(&self) -> bool;

    /// `true` when this tool can safely run alongside other tool calls in the
    /// same batch. Defaults to [`Self::is_read_only`] because mutating tools
    /// are sequenced by default.
    fn is_parallel_safe(&self) -> bool {
        self.is_read_only()
    }

    /// Per-call parallel-safety, given the call's arguments. Defaults to the
    /// tool-level [`Self::is_parallel_safe`]. Override when safety depends on
    /// the specific arguments — e.g. the `task` tool is parallel-safe only for
    /// read-only sub-agent types (`explore`/`plan`), not the writable
    /// `general-purpose` agent.
    fn is_call_parallel_safe(&self, _args: &serde_json::Value) -> bool {
        self.is_parallel_safe()
    }

    /// Maximum wall-clock time the runner should allow this tool to execute
    /// before raising [`ToolError::Timeout`]. Defaults to 30 seconds.
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Execute the tool with the supplied JSON arguments and return its
    /// textual output on success. Implementations must validate `args` and
    /// return [`ToolError::InvalidArgs`] for malformed input rather than
    /// panicking.
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError>;

    /// Pre-flight input validation. Called BEFORE permission checks.
    ///
    /// Returns `Ok(())` on valid input; `Err(message)` on invalid.
    /// The error message is returned to the model as a tool error.
    ///
    /// Default: always valid (no validation).
    async fn validate_input(&self, _input: &Value) -> Result<(), String> {
        Ok(())
    }

    // ─── P4 additions ────────────────────────────────────────────────────
    //
    // The following methods extend the trait with permission/interrupt
    // metadata used by the core decision flow. All have safe default
    // implementations so existing tools continue to compile unchanged.

    /// Tool's own permission logic. Receives the active mode and the tool's
    /// input. The full [`crate::permission::PermissionResult`] lookup (rules,
    /// additional dirs, etc.) lives in the core decision flow; tools only
    /// need to see the [`crate::permission::PermissionMode`] to make
    /// mode-specific calls (e.g. write tools return `Ask` in plan mode).
    ///
    /// The default returns
    /// [`crate::permission::PermissionResult::Passthrough`], meaning the tool
    /// abstains and the decision falls through to rules or the mode default.
    async fn check_permissions(
        &self,
        _input: &Value,
        _mode: crate::permission::PermissionMode,
    ) -> crate::permission::PermissionResult {
        crate::permission::PermissionResult::Passthrough {
            message: format!("Permission required to use {}", self.name()),
        }
    }

    /// Whether this invocation is destructive (delete / overwrite /
    /// otherwise unrecoverable). Used by the UI to flag high-risk approvals.
    /// Defaults to `false`.
    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    /// What happens to a running tool when the user submits a new message.
    /// Defaults to [`InterruptBehavior::Block`] — most tools should finish
    /// before the next message is handled.
    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Block
    }

    /// Path the tool operates on (if any), for path-aware deny rules.
    /// Defaults to [`None`]; filesystem tools override to return their
    /// target path.
    fn get_path(&self, _input: &Value) -> Option<String> {
        None
    }

    /// Force user interaction even in bypass / accept-edits modes (e.g.
    /// `ExitPlanMode`). Defaults to `false`.
    fn requires_user_interaction(&self) -> bool {
        false
    }

    /// Optional self-describing tool spec. Built-in tools return `None`
    /// (their specs live in `specs::build_tool_specs`); dynamically
    /// registered tools (e.g. MCP proxies) override this to supply their
    /// schema so `build_tool_specs_from_registry` can advertise them.
    fn spec(&self) -> Option<deepseek_client::types::ToolSpec> {
        None
    }
}

/// What happens to a running tool when the user submits a new message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptBehavior {
    /// Stop immediately and discard the in-flight result.
    Cancel,
    /// Keep running; the new user message waits for the tool to finish.
    Block,
}

#[cfg(test)]
mod tool_trait_tests {
    use super::*;
    use crate::permission::{PermissionMode, PermissionResult};
    use crate::types::ToolError;
    use serde_json::{json, Value};

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn is_read_only(&self) -> bool {
            false
        }
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
            Ok("ok".into())
        }
    }

    #[test]
    fn default_tool_methods_are_safe_defaults() {
        let t = DummyTool;
        assert!(!t.is_destructive(&json!({})), "default false");
        assert_eq!(t.interrupt_behavior(), InterruptBehavior::Block);
        assert!(t.get_path(&json!({})).is_none());
        assert!(!t.requires_user_interaction());
    }

    #[tokio::test]
    async fn default_check_permissions_returns_passthrough() {
        let t = DummyTool;
        let r = t
            .check_permissions(&json!({}), PermissionMode::Default)
            .await;
        assert!(matches!(r, PermissionResult::Passthrough { .. }));
    }
}
