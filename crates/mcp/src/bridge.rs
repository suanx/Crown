//! Bridge MCP tools into the agent's `Tool` trait so the model can call them
//! like any built-in tool. Each MCP tool becomes an `McpToolProxy` named
//! `mcp__<server>__<tool>`; calls route back through the `McpManager`.

use std::sync::Weak;
use std::time::Duration;

use async_trait::async_trait;
use deepseek_client::types::{FunctionSpec, ToolSpec};
use deepseek_tools::permission::PermissionResult;
use deepseek_tools::{Tool, ToolContext, ToolError};
use serde_json::Value;

use crate::manager::McpManager;
use crate::naming::mcp_tool_name;
use crate::types::McpToolInfo;

/// A model-facing proxy for a single MCP tool.
pub struct McpToolProxy {
    full_name: String,
    server: String,
    tool: String,
    description: String,
    input_schema: Value,
    manager: Weak<McpManager>,
}

impl McpToolProxy {
    pub fn new(server: &str, info: McpToolInfo, manager: Weak<McpManager>) -> Self {
        Self {
            full_name: mcp_tool_name(server, &info.name),
            server: server.to_string(),
            tool: info.name,
            description: info.description,
            input_schema: info.input_schema,
            manager,
        }
    }
}

#[async_trait]
impl Tool for McpToolProxy {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn is_read_only(&self) -> bool {
        // Conservative: MCP tools may have side effects.
        false
    }

    fn is_parallel_safe(&self) -> bool {
        false
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn check_permissions(
        &self,
        _input: &Value,
        _mode: deepseek_tools::permission::PermissionMode,
    ) -> PermissionResult {
        // MCP tools fall through to mode/rules (YOLO mode auto-allows).
        PermissionResult::Passthrough {
            message: format!("Run MCP tool {}", self.full_name),
        }
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
        let mgr = self
            .manager
            .upgrade()
            .ok_or_else(|| ToolError::ExecutionFailed("MCP manager unavailable".into()))?;
        mgr.call_tool(&self.server, &self.tool, args)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }

    fn spec(&self) -> Option<ToolSpec> {
        // Ensure a valid JSON Schema object for the parameters field.
        let parameters = if self.input_schema.is_object() {
            self.input_schema.clone()
        } else {
            serde_json::json!({ "type": "object", "properties": {} })
        };
        Some(ToolSpec {
            tool_type: "function".to_string(),
            function: FunctionSpec {
                name: self.full_name.clone(),
                description: self.description.clone(),
                parameters,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> McpToolInfo {
        McpToolInfo {
            name: "create_pr".into(),
            description: "Create a PR".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "title": { "type": "string" } }
            }),
        }
    }

    #[test]
    fn proxy_exposes_namespaced_name() {
        let proxy = McpToolProxy::new("github", info(), Weak::new());
        assert_eq!(proxy.name(), "mcp__github__create_pr");
        assert!(!proxy.is_parallel_safe());
    }

    #[test]
    fn proxy_spec_carries_schema() {
        let proxy = McpToolProxy::new("github", info(), Weak::new());
        let spec = proxy.spec().unwrap();
        assert_eq!(spec.function.name, "mcp__github__create_pr");
        assert_eq!(spec.function.description, "Create a PR");
        assert_eq!(
            spec.function.parameters["properties"]["title"]["type"],
            "string"
        );
    }

    #[tokio::test]
    async fn execute_without_manager_errors() {
        let proxy = McpToolProxy::new("github", info(), Weak::new());
        let ctx = ToolContext::standalone();
        let err = proxy
            .execute(serde_json::json!({}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::ExecutionFailed(_)));
    }
}
