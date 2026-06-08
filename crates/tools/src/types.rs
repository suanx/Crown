//! Core data types shared by the tool subsystem.
//!
//! These types form the contract between the agent engine and any
//! [`crate::Tool`] implementation: the engine builds [`ToolCall`] values from
//! the model's output, hands them to a tool, and turns the outcome into a
//! [`ToolResult`] that is fed back into the next reasoning step. Errors raised
//! by tools surface as [`ToolError`].

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Outcome of a single tool invocation.
///
/// Whether or not execution succeeded, the engine always produces a
/// `ToolResult` so the model can observe the outcome on the next turn. The
/// `is_error` flag distinguishes graceful tool failures (e.g. file not found)
/// from successful executions; transport-level failures are represented by
/// [`ToolError`] and converted into an error `ToolResult` by the runner.
///
/// ## Shell-specific metadata
///
/// For `run_command`, `exit_code` and `failure_stage` are populated so the
/// engine can distinguish normal non-zero exits from permission denials,
/// sandbox blocks, timeouts, and user aborts without string-parsing the
/// output. Codex `exec_command` and Claude Code `BashTool` both maintain this
/// kind of structured shell metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Identifier echoed back from the originating [`ToolCall::id`] so the
    /// model can correlate results with its tool calls.
    pub tool_use_id: String,

    /// Name of the tool that produced this result.
    pub tool_name: String,

    /// `true` when the tool reported a logical failure. The error message is
    /// stored in `content`.
    pub is_error: bool,

    /// Human-readable payload returned to the model. For successful calls
    /// this is the tool's textual output; for errors it is a description of
    /// what went wrong.
    pub content: String,

    /// Wall-clock time spent executing the tool, in milliseconds.
    pub duration_ms: u64,

    /// Shell exit code, if this tool is `run_command`. `None` for all other
    /// tools. Negative values represent signal kills.
    pub exit_code: Option<i32>,

    /// Where the shell command failed, if it did. `None` for non-shell tools
    /// and successful shell runs.
    pub failure_stage: Option<ShellFailureStage>,
}

/// Where a shell command failed — mirrors Codex's exec failure taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellFailureStage {
    /// The command could not be spawned (binary missing, permission denied
    /// on the executable, etc.).
    Spawn,
    /// The command was killed by timeout before it could finish.
    Timeout,
    /// The command exited with a non-zero code — the text output has details.
    NonZeroExit,
    /// The command was aborted by user cancellation.
    Aborted,
    /// The command was blocked by a sandbox / security policy.
    SandboxDenied,
    /// Permission denied at the OS level (cannot read/write/execute).
    PermissionDenied,
}

/// A request from the model to invoke a specific tool.
///
/// `id` is the opaque identifier the model attaches to the call; it must be
/// echoed back in the corresponding [`ToolResult::tool_use_id`]. `arguments`
/// is the raw JSON payload supplied by the model and is validated by the tool
/// implementation.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Opaque identifier supplied by the model for this call.
    pub id: String,

    /// Name of the tool being invoked.
    pub name: String,

    /// JSON arguments as produced by the model.
    pub arguments: Value,
}

/// Errors raised while dispatching or executing a tool.
///
/// Tool authors should map domain failures (e.g. "file not found") onto
/// [`ToolError::ExecutionFailed`] when they are recoverable, and onto
/// [`ToolError::InvalidArgs`] when the model supplied a malformed request.
/// The remaining variants represent infrastructure-level conditions handled
/// by the runner.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// No tool with the requested name is registered.
    #[error("tool not found: {0}")]
    NotFound(String),

    /// The model supplied arguments that do not match the tool's schema.
    #[error("invalid arguments for {tool}: {message}")]
    InvalidArgs {
        /// Name of the tool that rejected the arguments.
        tool: String,
        /// Human-readable explanation of what was wrong with the arguments.
        message: String,
    },

    /// The tool ran but reported a failure.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// The user explicitly denied an approval prompt for this call.
    #[error("denied by user")]
    DeniedByUser,

    /// The tool refused to run because of a permission or policy check.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// The tool exceeded its configured execution budget.
    #[error("tool timed out after {0:?}")]
    Timeout(Duration),

    /// The current turn was aborted by the user mid-execution.
    #[error("aborted by user")]
    Aborted,

    /// The tool produced more output than the runner is willing to forward.
    #[error("output too large: produced {actual} bytes, limit is {limit} bytes")]
    OutputTooLarge {
        /// Number of bytes actually produced.
        actual: usize,
        /// Configured upper bound on tool output.
        limit: usize,
    },

    /// Wraps an underlying I/O failure.
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}
