//! Built-in tools that let the agent install/reload MCP servers on its own
//! ("agent as installer"). Registered at app startup with the live manager
//! captured, so the model can YOLO-install a server in one tool call.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use deepseek_client::types::{FunctionSpec, ToolSpec};
use deepseek_tools::{Tool, ToolContext, ToolError};
use serde_json::{json, Value};

use crate::config::McpConfig;
use crate::manager::McpManager;
use crate::types::ServerStatus;

/// `mcp_install` — write a server config to mcp.json, connect it, report back.
pub struct McpInstallTool {
    manager: Weak<McpManager>,
}

impl McpInstallTool {
    pub fn new(manager: &Arc<McpManager>) -> Self {
        Self {
            manager: Arc::downgrade(manager),
        }
    }
}

#[async_trait]
impl Tool for McpInstallTool {
    fn name(&self) -> &str {
        "mcp_install"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_parallel_safe(&self) -> bool {
        false
    }

    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60)
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return Err("`name` is required".into());
        }
        if !input.get("config").map(|v| v.is_object()).unwrap_or(false) {
            return Err("`config` must be an object (the MCP server config)".into());
        }
        Ok(())
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs {
                tool: "mcp_install".into(),
                message: "`name` is required".into(),
            })?
            .to_string();
        let config = args
            .get("config")
            .cloned()
            .ok_or_else(|| ToolError::InvalidArgs {
                tool: "mcp_install".into(),
                message: "`config` is required".into(),
            })?;

        let mgr = self
            .manager
            .upgrade()
            .ok_or_else(|| ToolError::ExecutionFailed("MCP manager unavailable".into()))?;

        // Write to the manager's authoritative mcp.json path (validates config
        // shape too), so install + reload always agree on the same file.
        McpConfig::add_server_at(&mgr.config_path(), &name, config)
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to write mcp.json: {e}")))?;
        mgr.reload_from_disk().await;

        // Wait briefly for the new server to connect.
        let mut status = mgr.status(&name);
        for _ in 0..40 {
            if matches!(
                status,
                Some(ServerStatus::Connected) | Some(ServerStatus::Failed)
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            status = mgr.status(&name);
        }

        match status {
            Some(ServerStatus::Connected) => {
                let conn = mgr.connection(&name);
                let n = conn.as_ref().map(|c| c.tools().len()).unwrap_or(0);
                let names: Vec<String> = conn
                    .map(|c| c.tools().iter().map(|t| format!("mcp__{name}__{}", t.name)).collect())
                    .unwrap_or_default();
                Ok(format!(
                    "Installed MCP server '{name}' — connected with {n} tool(s): {}",
                    names.join(", ")
                ))
            }
            Some(ServerStatus::Failed) => Err(ToolError::ExecutionFailed(format!(
                "MCP server '{name}' was written to config but failed to connect. Check the command/url."
            ))),
            _ => Ok(format!(
                "MCP server '{name}' added to config; still connecting (status: {status:?})."
            )),
        }
    }

    fn spec(&self) -> Option<ToolSpec> {
        Some(ToolSpec {
            tool_type: "function".into(),
            function: FunctionSpec {
                name: "mcp_install".into(),
                description: "Install an MCP server: writes its config to the global mcp.json and connects it immediately. Use when the user asks to install/add an MCP server. After this succeeds, the server's tools become available as mcp__<server>__<tool>.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Server name (letters, digits, -, _)" },
                        "config": {
                            "type": "object",
                            "description": "MCP server config, e.g. {\"command\":\"npx\",\"args\":[\"-y\",\"pkg\"]} or {\"type\":\"http\",\"url\":\"https://...\"}"
                        }
                    },
                    "required": ["name", "config"]
                }),
            },
        })
    }
}

/// `mcp_reload` — re-read mcp.json and reconnect (after a manual edit).
pub struct McpReloadTool {
    manager: Weak<McpManager>,
}

impl McpReloadTool {
    pub fn new(manager: &Arc<McpManager>) -> Self {
        Self {
            manager: Arc::downgrade(manager),
        }
    }
}

#[async_trait]
impl Tool for McpReloadTool {
    fn name(&self) -> &str {
        "mcp_reload"
    }

    fn is_read_only(&self) -> bool {
        false
    }

    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
        let mgr = self
            .manager
            .upgrade()
            .ok_or_else(|| ToolError::ExecutionFailed("MCP manager unavailable".into()))?;
        mgr.reload_from_disk().await;
        let servers = mgr.list_servers();
        Ok(format!(
            "Reloaded MCP config — {} server(s) configured.",
            servers.len()
        ))
    }

    fn spec(&self) -> Option<ToolSpec> {
        Some(ToolSpec {
            tool_type: "function".into(),
            function: FunctionSpec {
                name: "mcp_reload".into(),
                description: "Reload MCP servers from the global mcp.json. Use after you have manually edited the config file.".into(),
                parameters: json!({ "type": "object", "properties": {} }),
            },
        })
    }
}

/// Register the install tools onto the registry with the manager captured.
pub fn register_install_tools(registry: &deepseek_tools::ToolRegistry, manager: &Arc<McpManager>) {
    registry.register_dynamic(Arc::new(McpInstallTool::new(manager)));
    registry.register_dynamic(Arc::new(McpReloadTool::new(manager)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn install_validates_input() {
        let mgr = Arc::new(McpManager::new());
        let tool = McpInstallTool::new(&mgr);
        assert!(tool
            .validate_input(&json!({ "name": "", "config": {} }))
            .await
            .is_err());
        assert!(tool.validate_input(&json!({ "name": "x" })).await.is_err());
        assert!(tool
            .validate_input(&json!({ "name": "x", "config": {"command":"y"} }))
            .await
            .is_ok());
    }

    #[test]
    fn install_tool_has_spec() {
        let mgr = Arc::new(McpManager::new());
        let tool = McpInstallTool::new(&mgr);
        let spec = tool.spec().unwrap();
        assert_eq!(spec.function.name, "mcp_install");
    }
}
