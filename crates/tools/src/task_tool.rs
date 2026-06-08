//! The `task` tool — delegates to a sub-agent via the injected
//! [`crate::SubagentLauncher`] (P4). The launcher (app layer) owns the
//! engine + event sink; this tool just forwards the request and returns the
//! sub-agent's final report.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use deepseek_client::types::{FunctionSpec, ToolSpec};

use crate::types::ToolError;
use crate::{Tool, ToolContext};

/// `task` — spawn (or resume) a sub-agent.
pub struct TaskTool;

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        "task"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_parallel_safe(&self) -> bool {
        // Default (argument-agnostic) answer: not safe. Per-call safety is
        // decided by `is_call_parallel_safe` based on `agent_type`.
        false
    }

    fn is_call_parallel_safe(&self, args: &Value) -> bool {
        // Read-only sub-agents (explore / plan) never mutate the workspace, so
        // multiple can run concurrently — this is exactly the "open N explore
        // agents in parallel" use case. The writable `general-purpose` agent
        // stays serial to avoid concurrent-write conflicts.
        matches!(
            args.get("agent_type").and_then(|v| v.as_str()),
            Some("explore") | Some("plan")
        )
    }

    fn timeout(&self) -> Duration {
        // Sub-agent runs are full agent loops — allow up to 10 minutes.
        Duration::from_secs(600)
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        if input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err("`prompt` is required and must be a non-empty string".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let launcher = ctx
            .subagent
            .as_ref()
            .ok_or_else(|| ToolError::ExecutionFailed("sub-agent launcher unavailable".into()))?;

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidArgs {
                tool: "task".into(),
                message: "`prompt` is required".into(),
            })?;
        let agent_type = args
            .get("agent_type")
            .and_then(|v| v.as_str())
            .unwrap_or("general-purpose")
            .to_string();
        let resume = args
            .get("subagent_id")
            .and_then(|v| v.as_str())
            .map(String::from);
        let parent = ctx
            .thread_id
            .clone()
            .ok_or_else(|| ToolError::ExecutionFailed("no parent thread in context".into()))?;

        let (report, sub_id) = launcher
            .launch(agent_type, prompt, resume, parent, ctx.abort.clone())
            .await
            .map_err(ToolError::ExecutionFailed)?;

        match sub_id {
            Some(id) => Ok(format!(
                "{report}\n\n[subagent_id: {id} — call task again with this subagent_id to continue this sub-agent]"
            )),
            None => Ok(report),
        }
    }

    fn spec(&self) -> Option<ToolSpec> {
        Some(ToolSpec {
            tool_type: "function".into(),
            function: FunctionSpec {
                name: "task".into(),
                description: "Delegate a focused task to a sub-agent with its own isolated context. \
Use for large investigations, parallelizable work, or to keep your own context clean. \
agent_type: 'general-purpose' (full tools, resumable), 'explore' (read-only investigation, one-shot), \
'plan' (read-only, produces an implementation plan, one-shot). The sub-agent runs autonomously and \
returns a report. For 'general-purpose', the result includes a subagent_id you can pass back to continue it."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "description": { "type": "string", "description": "Short (3-5 word) description of the task" },
                        "prompt": { "type": "string", "description": "The full task for the sub-agent to perform" },
                        "agent_type": {
                            "type": "string",
                            "enum": ["general-purpose", "explore", "plan"],
                            "description": "Which sub-agent to use (default general-purpose)"
                        },
                        "subagent_id": { "type": "string", "description": "Resume an existing sub-agent (from a prior task result)" }
                    },
                    "required": ["prompt"]
                }),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_requires_prompt() {
        assert!(TaskTool.validate_input(&json!({})).await.is_err());
        assert!(TaskTool
            .validate_input(&json!({ "prompt": "" }))
            .await
            .is_err());
        assert!(TaskTool
            .validate_input(&json!({ "prompt": "do x" }))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn execute_without_launcher_errors() {
        let ctx = ToolContext::standalone();
        let err = TaskTool
            .execute(json!({ "prompt": "do x" }), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }

    #[test]
    fn task_tool_has_spec() {
        let spec = TaskTool.spec().unwrap();
        assert_eq!(spec.function.name, "task");
    }

    #[test]
    fn explore_and_plan_are_call_parallel_safe() {
        assert!(TaskTool.is_call_parallel_safe(&json!({ "agent_type": "explore", "prompt": "x" })));
        assert!(TaskTool.is_call_parallel_safe(&json!({ "agent_type": "plan", "prompt": "x" })));
    }

    #[test]
    fn general_purpose_and_default_are_not_call_parallel_safe() {
        assert!(!TaskTool
            .is_call_parallel_safe(&json!({ "agent_type": "general-purpose", "prompt": "x" })));
        // No agent_type → defaults to general-purpose → serial.
        assert!(!TaskTool.is_call_parallel_safe(&json!({ "prompt": "x" })));
    }
}
