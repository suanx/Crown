//! MCP capability + status DTOs.

use serde::{Deserialize, Serialize};

/// Connection status of an MCP server. Mirrors Claude Code's
/// `MCPServerConnection` union (minus auth-flow internals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    /// Configured but explicitly disabled — not connected.
    Disabled,
    /// Connecting / reconnecting.
    Pending,
    /// Handshake succeeded; tools/resources/prompts available.
    Connected,
    /// Connection failed; eligible for reconnect.
    Failed,
    /// Remote server returned 401/auth-required; not auto-reconnected.
    NeedsAuth,
}

/// A tool exposed by a connected MCP server.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A resource exposed by a connected MCP server.
#[derive(Debug, Clone)]
pub struct McpResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
}

/// A prompt exposed by a connected MCP server (surfaced as a skill).
#[derive(Debug, Clone)]
pub struct McpPromptInfo {
    pub name: String,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(ServerStatus::Connected).unwrap(),
            serde_json::json!("connected")
        );
        assert_eq!(
            serde_json::to_value(ServerStatus::NeedsAuth).unwrap(),
            serde_json::json!("needs_auth")
        );
        assert_eq!(
            serde_json::to_value(ServerStatus::Disabled).unwrap(),
            serde_json::json!("disabled")
        );
    }
}
