//! MCP commands — backed by the live `McpManager`.
//!
//! Servers are configured in the global `mcp.json`. Adding/removing/toggling
//! a server rewrites that file and reloads the manager so the change takes
//! effect without a restart (hot reload). User-supplied parameters drop their
//! leading underscore so Tauri v2 can bind the camelCase invoke keys.

use deepseek_mcp::config::{McpConfig, ServerConfig};
use serde::Serialize;

use crate::dto::McpServerDto;
use crate::AppState;

/// List all configured MCP servers with live status + tool counts.
#[tauri::command]
pub async fn list_mcp_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpServerDto>, String> {
    let mgr = &state.mcp;
    let mut out = Vec::new();
    for (name, status) in mgr.list_servers() {
        let conn = mgr.connection(&name);
        let (command, args, enabled, transport) = match conn.as_ref().map(|c| c.config()) {
            Some(ServerConfig::Stdio {
                command,
                args,
                disabled,
                ..
            }) => (command.clone(), args.clone(), !*disabled, "stdio"),
            Some(ServerConfig::Http { url, disabled, .. }) => {
                (url.clone(), vec![], !*disabled, "http")
            }
            Some(ServerConfig::Sse { url, disabled, .. }) => {
                (url.clone(), vec![], !*disabled, "sse")
            }
            None => (String::new(), vec![], true, "stdio"),
        };
        let tool_count = conn.as_ref().map(|c| c.tools().len() as u64).unwrap_or(0);
        let status_str = serde_json::to_value(status)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "failed".into());
        let _ = transport;
        out.push(McpServerDto {
            name,
            command,
            args,
            status: status_str,
            enabled,
            tool_count,
            error_message: None,
        });
    }
    Ok(out)
}

/// Add (or replace) a server in mcp.json and hot-reload.
#[tauri::command]
pub async fn mcp_add_server(
    state: tauri::State<'_, AppState>,
    name: String,
    config: serde_json::Value,
) -> Result<(), String> {
    McpConfig::add_server_at(&state.mcp.config_path(), &name, config).map_err(|e| e.to_string())?;
    state.mcp.reload_from_disk().await;
    Ok(())
}

/// Remove a server from mcp.json and hot-reload.
#[tauri::command]
pub async fn mcp_remove_server(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    McpConfig::remove_server_at(&state.mcp.config_path(), &name).map_err(|e| e.to_string())?;
    state.mcp.remove_connection(&name).await;
    Ok(())
}

/// Enable/disable a server (rewrites `disabled` flag) and hot-reload.
#[tauri::command]
pub async fn toggle_mcp_server(
    state: tauri::State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    McpConfig::set_disabled_at(&state.mcp.config_path(), &name, !enabled)
        .map_err(|e| e.to_string())?;
    state.mcp.reload_from_disk().await;
    Ok(())
}

/// Force a full reload from mcp.json (manual refresh).
#[tauri::command]
pub async fn restart_mcp_server(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let _ = name; // reload reconnects all servers including this one
    state.mcp.reload_from_disk().await;
    Ok(())
}

/// Reload all MCP servers from config.
#[tauri::command]
pub async fn mcp_reload(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.mcp.reload_from_disk().await;
    Ok(())
}

/// DTO for a single tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDto {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// List tools (with full input schemas) exposed by a specific MCP server.
#[tauri::command]
pub async fn list_mcp_tools(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<Vec<McpToolDto>, String> {
    let mgr = &state.mcp;
    let conn = mgr
        .connection(&name)
        .ok_or_else(|| format!("MCP server '{}' not found", name))?;
    let tools = conn.tools();
    Ok(tools
        .into_iter()
        .map(|t| McpToolDto {
            name: t.name,
            description: t.description,
            input_schema: t.input_schema,
        })
        .collect())
}
