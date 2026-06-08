//! Registry of tools available to the agent.
//!
//! The [`ToolRegistry`] indexes [`crate::Tool`] implementations by name and
//! is the entry point used by the agent engine to dispatch a [`ToolCall`]
//! produced by the model. Dispatch enforces the per-tool timeout returned by
//! [`crate::Tool::timeout`] and turns every outcome — success, logical
//! failure, or timeout — into a [`ToolResult`] so the model always sees a
//! response on the next turn.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use tokio::time::timeout;

use crate::types::{ShellFailureStage, ToolCall, ToolResult};
use crate::Tool;
use crate::ToolContext;
use crate::ToolError;

/// Container of [`crate::Tool`] implementations indexed by name.
///
/// Tools are stored as `Arc<dyn Tool>` behind an [`RwLock`] so the registry
/// supports runtime add/remove (MCP tools connect/disconnect at runtime)
/// while still being shared as `Arc<ToolRegistry>` by the engine.
#[derive(Default)]
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool under its [`crate::Tool::name`]. Replaces any existing
    /// tool with the same name. Takes `&self` (interior mutability) so the
    /// registry can be shared as `Arc` and mutated at runtime.
    pub fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.write().insert(name, tool);
    }

    /// Alias for [`register`] used by the MCP layer for runtime tools.
    pub fn register_dynamic(&self, tool: Arc<dyn Tool>) {
        self.register(tool);
    }

    /// Remove a tool by exact name.
    pub fn unregister(&self, name: &str) {
        self.tools.write().remove(name);
    }

    /// Remove all tools whose name starts with `prefix` (e.g. `"mcp__server__"`).
    pub fn unregister_prefix(&self, prefix: &str) {
        self.tools.write().retain(|k, _| !k.starts_with(prefix));
    }

    /// Look up a tool by name, returning an owned `Arc` (cloned out of the
    /// lock) so callers don't hold the registry lock.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().get(name).cloned()
    }

    /// Snapshot of all registered tools.
    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.read().values().cloned().collect()
    }

    /// Build a NEW registry containing a restricted subset of this one's
    /// tools, for sub-agents (P4). `allow` is a whitelist of tool names
    /// (empty = all); `exclude` names are always dropped (e.g. `"task"` to
    /// prevent unbounded sub-agent recursion).
    pub fn subset(&self, allow: &[&str], exclude: &[&str]) -> ToolRegistry {
        let out = ToolRegistry::new();
        {
            let src = self.tools.read();
            let mut dst = out.tools.write();
            for (name, tool) in src.iter() {
                if exclude.contains(&name.as_str()) {
                    continue;
                }
                if allow.is_empty() || allow.contains(&name.as_str()) {
                    dst.insert(name.clone(), tool.clone());
                }
            }
        }
        out
    }

    /// Return the names of all registered tools, sorted alphabetically.
    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.read().keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.read().len()
    }

    /// Whether the registry has no registered tools.
    pub fn is_empty(&self) -> bool {
        self.tools.read().is_empty()
    }

    /// Dispatch a [`ToolCall`] to the matching tool with timeout enforcement.
    ///
    /// The call always resolves to a [`ToolResult`]; transport-level errors
    /// (unknown tool, timeout) and logical errors raised by the tool itself
    /// are both surfaced as `is_error: true` results so the agent loop can
    /// feed them back to the model. The wall-clock time spent in the tool is
    /// reported in `duration_ms`; for unknown tools it is `0` because no
    /// dispatch occurred.
    pub async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> ToolResult {
        let tool = match self.get(&call.name) {
            Some(tool) => tool,
            None => {
                tracing::warn!(tool = %call.name, "tool not found");
                return ToolResult {
                    tool_use_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    is_error: true,
                    content: format!("Unknown tool: {}", call.name),
                    duration_ms: 0,
                    exit_code: None,
                    failure_stage: None,
                };
            }
        };

        let tool_timeout = tool.timeout();
        let start = Instant::now();
        let outcome = timeout(tool_timeout, tool.execute(call.arguments.clone(), ctx)).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match outcome {
            Ok(Ok(output)) => {
                tracing::debug!(
                    tool = %call.name,
                    duration_ms,
                    status = "ok",
                    "tool executed",
                );
                // Shell tools: parse exit code from output preamble for
                // structured metadata (Codex-aligned exec taxonomy).
                let (exit_code, failure_stage) = if call.name == "run_command" {
                    shell_metadata(&output)
                } else {
                    (None, None)
                };
                ToolResult {
                    tool_use_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    is_error: false,
                    content: output,
                    duration_ms,
                    exit_code,
                    failure_stage,
                }
            }
            Ok(Err(err)) => {
                let message = format!("{}", err);
                tracing::warn!(
                    tool = %call.name,
                    duration_ms,
                    status = "error",
                    error = %message,
                    "tool returned error",
                );
                let (exit_code, failure_stage) = if call.name == "run_command" {
                    shell_error_metadata(&err)
                } else {
                    (None, None)
                };
                ToolResult {
                    tool_use_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    is_error: true,
                    content: message,
                    duration_ms,
                    exit_code,
                    failure_stage,
                }
            }
            Err(_elapsed) => {
                tracing::warn!(
                    tool = %call.name,
                    duration_ms,
                    timeout_ms = tool_timeout.as_millis() as u64,
                    status = "timeout",
                    "tool timed out",
                );
                let failure_stage = if call.name == "run_command" {
                    Some(ShellFailureStage::Timeout)
                } else {
                    None
                };
                ToolResult {
                    tool_use_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    is_error: true,
                    content: format!("Tool '{}' timed out after {:?}", call.name, tool_timeout),
                    duration_ms,
                    exit_code: None,
                    failure_stage,
                }
            }
        }
    }
}

/// Extract shell metadata from a successful run_command output.
fn shell_metadata(output: &str) -> (Option<i32>, Option<ShellFailureStage>) {
    let exit_code = parse_exit_code(output);
    let stage = match exit_code {
        Some(0) | None => None,
        Some(_) => Some(ShellFailureStage::NonZeroExit),
    };
    (exit_code, stage)
}

/// Extract shell metadata from a ToolError returned by run_command.
fn shell_error_metadata(err: &ToolError) -> (Option<i32>, Option<ShellFailureStage>) {
    match err {
        ToolError::Aborted => (None, Some(ShellFailureStage::Aborted)),
        ToolError::PermissionDenied(_) => (None, Some(ShellFailureStage::PermissionDenied)),
        _ => (None, None),
    }
}

/// Parse the exit code from run_command's formatted output preamble.
fn parse_exit_code(content: &str) -> Option<i32> {
    content
        .lines()
        .next()?
        .trim()
        .strip_prefix("Exit code:")
        .and_then(|v| v.trim().parse::<i32>().ok())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::*;
    use crate::types::ToolError;

    /// How a [`MockTool`] should respond when invoked.
    enum MockBehavior {
        /// Return `Ok(String)` immediately.
        Success(String),
        /// Return `Err(ToolError::ExecutionFailed(String))` immediately.
        Failure(String),
        /// Sleep for the given duration and then return `Ok("slept")`.
        Sleep(Duration),
    }

    /// Configurable tool used to drive registry tests.
    struct MockTool {
        name: String,
        timeout: Duration,
        behavior: MockBehavior,
    }

    impl MockTool {
        fn success(name: &str, output: &str) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                timeout: Duration::from_secs(5),
                behavior: MockBehavior::Success(output.to_string()),
            })
        }

        fn failure(name: &str, message: &str) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                timeout: Duration::from_secs(5),
                behavior: MockBehavior::Failure(message.to_string()),
            })
        }

        fn slow(name: &str, timeout: Duration, sleep: Duration) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                timeout,
                behavior: MockBehavior::Sleep(sleep),
            })
        }
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn is_read_only(&self) -> bool {
            true
        }

        fn timeout(&self) -> Duration {
            self.timeout
        }

        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
            match &self.behavior {
                MockBehavior::Success(output) => Ok(output.clone()),
                MockBehavior::Failure(message) => Err(ToolError::ExecutionFailed(message.clone())),
                MockBehavior::Sleep(duration) => {
                    tokio::time::sleep(*duration).await;
                    Ok("slept".to_string())
                }
            }
        }
    }

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: format!("call_{name}"),
            name: name.to_string(),
            arguments: json!({}),
        }
    }

    #[tokio::test]
    async fn register_and_lookup() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());

        let tool = MockTool::success("echo", "hi");
        registry.register(tool);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let fetched = registry.get("echo").expect("tool should be registered");
        assert_eq!(fetched.name(), "echo");
        assert!(registry.list_names().contains(&"echo".to_string()));
        assert!(registry.get("missing").is_none());
    }

    #[tokio::test]
    async fn execute_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry
            .execute(&call("ghost"), &ToolContext::standalone())
            .await;

        assert!(result.is_error);
        assert_eq!(result.duration_ms, 0);
        assert_eq!(result.content, "Unknown tool: ghost");
        assert_eq!(result.tool_name, "ghost");
        assert_eq!(result.tool_use_id, "call_ghost");
    }

    #[tokio::test]
    async fn execute_success() {
        let registry = ToolRegistry::new();
        registry.register(MockTool::success("echo", "ok"));

        let result = registry
            .execute(&call("echo"), &ToolContext::standalone())
            .await;

        assert!(!result.is_error);
        assert_eq!(result.content, "ok");
        assert_eq!(result.tool_name, "echo");
    }

    #[tokio::test]
    async fn execute_failure() {
        let registry = ToolRegistry::new();
        registry.register(MockTool::failure("broken", "x"));

        let result = registry
            .execute(&call("broken"), &ToolContext::standalone())
            .await;

        assert!(result.is_error);
        assert!(
            result.content.contains('x'),
            "expected error message to contain 'x', got {:?}",
            result.content
        );
    }

    #[tokio::test]
    async fn execute_timeout() {
        let registry = ToolRegistry::new();
        registry.register(MockTool::slow(
            "slow",
            Duration::from_millis(50),
            Duration::from_millis(200),
        ));

        let result = registry
            .execute(&call("slow"), &ToolContext::standalone())
            .await;

        assert!(result.is_error);
        assert!(
            result.content.contains("timed out"),
            "expected timeout message, got {:?}",
            result.content
        );
    }

    #[tokio::test]
    async fn list_names_sorted() {
        let registry = ToolRegistry::new();
        registry.register(MockTool::success("charlie", ""));
        registry.register(MockTool::success("alpha", ""));
        registry.register(MockTool::success("bravo", ""));

        let names = registry.list_names();

        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }

    #[tokio::test]
    async fn dynamic_register_and_unregister() {
        let reg = ToolRegistry::new();
        reg.register_dynamic(MockTool::success("dyn", "ok"));
        assert!(reg.get("dyn").is_some());
        reg.unregister("dyn");
        assert!(reg.get("dyn").is_none());
    }

    #[tokio::test]
    async fn unregister_prefix_removes_matching() {
        let reg = ToolRegistry::new();
        reg.register_dynamic(MockTool::success("mcp__s__a", "ok"));
        reg.register_dynamic(MockTool::success("mcp__s__b", "ok"));
        reg.register_dynamic(MockTool::success("read_file", "ok"));
        reg.unregister_prefix("mcp__s__");
        assert!(reg.get("mcp__s__a").is_none());
        assert!(reg.get("mcp__s__b").is_none());
        assert!(reg.get("read_file").is_some(), "non-matching tool kept");
    }

    #[tokio::test]
    async fn subset_excludes_and_whitelists() {
        let reg = ToolRegistry::new();
        reg.register(MockTool::success("read_file", "ok"));
        reg.register(MockTool::success("write_file", "ok"));
        reg.register(MockTool::success("task", "ok"));

        // exclude task, allow all others
        let sub = reg.subset(&[], &["task"]);
        assert!(sub.get("read_file").is_some());
        assert!(sub.get("write_file").is_some());
        assert!(sub.get("task").is_none(), "task excluded (no recursion)");

        // whitelist only read_file, still exclude task
        let ro = reg.subset(&["read_file"], &["task"]);
        assert!(ro.get("read_file").is_some());
        assert!(ro.get("write_file").is_none());
        assert!(ro.get("task").is_none());

        // original registry unchanged
        assert_eq!(reg.len(), 3);
    }
}
