//! MCP (Model Context Protocol) client subsystem.
//!
//! Connects to external MCP servers (stdio / streamable-HTTP) via the
//! official `rmcp` SDK, bridges their tools into the agent's `ToolRegistry`,
//! and manages connection lifecycle (connect / reconnect / health check /
//! orphan cleanup). See `docs/superpowers/specs/2026-05-30-mcp-skill-design.md`.

pub mod bridge;
pub mod config;
pub mod connection;
pub mod install_tools;
pub mod manager;
pub mod naming;
pub mod types;

use std::sync::Arc;

/// Rebuild the registry's MCP tool proxies from the manager's current
/// connected tools. Removes all existing `mcp__` tools then registers a
/// proxy for each `(server, tool)` currently available. Call this on every
/// `ToolsChanged` event.
pub fn sync_registry_tools(
    registry: &deepseek_tools::ToolRegistry,
    manager: &Arc<manager::McpManager>,
) {
    registry.unregister_prefix("mcp__");
    let weak = Arc::downgrade(manager);
    for (server, info) in manager.all_tools() {
        let proxy = bridge::McpToolProxy::new(&server, info, weak.clone());
        registry.register_dynamic(Arc::new(proxy));
    }
}
