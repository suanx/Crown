//! Single MCP server connection: handshake, capability fetch, tool calls.
//!
//! Wraps the `rmcp` client. A connection owns a `RunningService` (the live
//! client peer) once connected, and caches the server's tools / resources /
//! prompts for fast listing without a round-trip.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::transport::ConfigureCommandExt;
use rmcp::ServiceExt;
use tokio::sync::Mutex;

use crate::config::ServerConfig;
use crate::types::{McpPromptInfo, McpResourceInfo, McpToolInfo, ServerStatus};

/// Handshake timeout for a server connection.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Per tool-call timeout.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Liveness-probe timeout. A wedged server that never answers `list_tools`
/// must not hang the health-check loop forever — bound the probe so a
/// non-responsive server is declared dead and reconnected.
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Exponential backoff (ms) for reconnect attempt `attempt`, capped at 30s.
pub fn reconnect_delay_ms(attempt: u32) -> u64 {
    (500u64 << attempt.min(6)).min(30_000)
}

type RunningClient = rmcp::service::RunningService<rmcp::RoleClient, ()>;

/// A single MCP server connection + cached capabilities.
pub struct McpConnection {
    pub name: String,
    config: ServerConfig,
    status: RwLock<ServerStatus>,
    tools: RwLock<Vec<McpToolInfo>>,
    resources: RwLock<Vec<McpResourceInfo>>,
    prompts: RwLock<Vec<McpPromptInfo>>,
    client: Mutex<Option<RunningClient>>,
}

impl McpConnection {
    pub fn new(name: impl Into<String>, config: ServerConfig) -> Arc<Self> {
        let initial = if config.is_disabled() {
            ServerStatus::Disabled
        } else {
            ServerStatus::Pending
        };
        Arc::new(Self {
            name: name.into(),
            config,
            status: RwLock::new(initial),
            tools: RwLock::new(Vec::new()),
            resources: RwLock::new(Vec::new()),
            prompts: RwLock::new(Vec::new()),
            client: Mutex::new(None),
        })
    }

    pub fn status(&self) -> ServerStatus {
        *self.status.read()
    }

    fn set_status(&self, s: ServerStatus) {
        *self.status.write() = s;
    }

    pub fn tools(&self) -> Vec<McpToolInfo> {
        self.tools.read().clone()
    }

    pub fn resources(&self) -> Vec<McpResourceInfo> {
        self.resources.read().clone()
    }

    pub fn prompts(&self) -> Vec<McpPromptInfo> {
        self.prompts.read().clone()
    }

    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Connect to the server, perform the handshake, and cache its
    /// tools/resources/prompts. Sets status to Connected on success.
    pub async fn connect(&self) -> Result<()> {
        if self.config.is_disabled() {
            self.set_status(ServerStatus::Disabled);
            return Ok(());
        }
        self.set_status(ServerStatus::Pending);

        let connect_fut = self.establish();
        let client = match tokio::time::timeout(HANDSHAKE_TIMEOUT, connect_fut).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                self.set_status(ServerStatus::Failed);
                return Err(e);
            }
            Err(_) => {
                self.set_status(ServerStatus::Failed);
                return Err(anyhow!("handshake timed out after {HANDSHAKE_TIMEOUT:?}"));
            }
        };

        // Fetch capabilities (best-effort per category).
        if let Ok(tools) = client.list_all_tools().await {
            *self.tools.write() = tools
                .into_iter()
                .map(|t| McpToolInfo {
                    name: t.name.to_string(),
                    description: t.description.map(|c| c.to_string()).unwrap_or_default(),
                    input_schema: serde_json::Value::Object((*t.input_schema).clone()),
                })
                .collect();
        }
        if let Ok(resources) = client.list_all_resources().await {
            *self.resources.write() = resources
                .into_iter()
                .map(|r| McpResourceInfo {
                    uri: r.raw.uri,
                    name: r.raw.name,
                    description: r.raw.description,
                })
                .collect();
        }
        if let Ok(prompts) = client.list_all_prompts().await {
            *self.prompts.write() = prompts
                .into_iter()
                .map(|p| McpPromptInfo {
                    name: p.name,
                    description: p.description.map(|c| c.to_string()),
                })
                .collect();
        }

        *self.client.lock().await = Some(client);
        self.set_status(ServerStatus::Connected);
        Ok(())
    }

    /// Build the transport per config type and serve a client over it.
    async fn establish(&self) -> Result<RunningClient> {
        match &self.config {
            ServerConfig::Stdio {
                command, args, env, ..
            } => {
                let (program, full_args) = resolve_command(command, args);
                let cmd = tokio::process::Command::new(&program).configure(|c| {
                    c.args(&full_args);
                    for (k, v) in env {
                        c.env(k, v);
                    }
                });
                let transport = TokioChildProcess::new(cmd)?;
                let client = ().serve(transport).await?;
                Ok(client)
            }
            ServerConfig::Http { url, headers, .. } | ServerConfig::Sse { url, headers, .. } => {
                let auth_header = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                    .map(|(_, v)| v.clone());
                let mut config = StreamableHttpClientTransportConfig::with_uri(url.as_str());
                config.auth_header = auth_header;
                let transport = StreamableHttpClientTransport::from_config(config);
                let client = ().serve(transport).await?;
                Ok(client)
            }
        }
    }

    /// Call a tool on the connected server.
    pub async fn call_tool(&self, tool: &str, args: serde_json::Value) -> Result<String> {
        let guard = self.client.lock().await;
        let client = guard.as_ref().ok_or_else(|| anyhow!("not connected"))?;
        let mut params = CallToolRequestParams::new(tool.to_string());
        match args {
            serde_json::Value::Object(m) => {
                params = params.with_arguments(m);
            }
            serde_json::Value::Null => {}
            other => return Err(anyhow!("tool arguments must be a JSON object, got {other}")),
        }
        let result = tokio::time::timeout(CALL_TIMEOUT, client.call_tool(params))
            .await
            .map_err(|_| anyhow!("tool call timed out"))??;
        Ok(extract_text(&result.content))
    }

    /// Read a resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<String> {
        let guard = self.client.lock().await;
        let client = guard.as_ref().ok_or_else(|| anyhow!("not connected"))?;
        let params: rmcp::model::ReadResourceRequestParams =
            serde_json::from_value(serde_json::json!({ "uri": uri }))?;
        let result = client.read_resource(params).await?;
        let mut out = String::new();
        for c in result.contents {
            if let rmcp::model::ResourceContents::TextResourceContents { text, .. } = c {
                out.push_str(&text);
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// Get a prompt's rendered messages as text.
    pub async fn get_prompt(&self, name: &str, args: serde_json::Value) -> Result<String> {
        let guard = self.client.lock().await;
        let client = guard.as_ref().ok_or_else(|| anyhow!("not connected"))?;
        let mut body = serde_json::json!({ "name": name });
        if let serde_json::Value::Object(m) = &args {
            // GetPrompt arguments are string→string per the MCP spec.
            let strmap: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), serde_json::Value::String(s))
                })
                .collect();
            body["arguments"] = serde_json::Value::Object(strmap);
        }
        let params: rmcp::model::GetPromptRequestParams = serde_json::from_value(body)?;
        let result = client.get_prompt(params).await?;
        let mut out = String::new();
        for msg in result.messages {
            if let rmcp::model::PromptMessageContent::Text { text } = msg.content {
                out.push_str(&text);
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// Lightweight liveness probe: re-list tools. Returns false if the
    /// client is gone, the call errors (server died), or the probe times out
    /// (a wedged server that never responds is treated as dead so the health
    /// loop can move on to reconnect instead of hanging forever).
    pub async fn is_alive(&self) -> bool {
        let guard = self.client.lock().await;
        match guard.as_ref() {
            Some(client) => {
                matches!(
                    tokio::time::timeout(HEALTH_PROBE_TIMEOUT, client.list_all_tools()).await,
                    Ok(Ok(_))
                )
            }
            None => false,
        }
    }

    /// Shut down the connection (drops the client; stdio child is killed on drop).
    pub async fn shutdown(&self) {
        if let Some(client) = self.client.lock().await.take() {
            let _ = client.cancel().await;
        }
        self.set_status(ServerStatus::Disabled);
    }
}

/// Resolve a command for the current platform. On Windows, `npx`/`npm`/`node`
/// and other commands are often `.cmd`/`.bat` shims that `CreateProcess`
/// won't find without going through the shell. Wrap such commands via
/// `cmd /c` so the shim resolves (this is how most npx-based MCP servers are
/// launched). On Unix the command is used as-is.
fn resolve_command(command: &str, args: &[String]) -> (String, Vec<String>) {
    if cfg!(windows) {
        let needs_shell = command.ends_with(".cmd")
            || command.ends_with(".bat")
            || !command.contains(['/', '\\']) && !command.to_ascii_lowercase().ends_with(".exe");
        if needs_shell {
            let mut full = vec!["/c".to_string(), command.to_string()];
            full.extend(args.iter().cloned());
            return ("cmd".to_string(), full);
        }
    }
    (command.to_string(), args.to_vec())
}

/// Extract concatenated text from a tool result's content blocks.
fn extract_text(content: &[rmcp::model::Content]) -> String {
    let mut out = String::new();
    for c in content {
        if let Some(text) = c.as_text() {
            out.push_str(&text.text);
            out.push('\n');
        }
    }
    if out.is_empty() {
        // Non-text content (image/resource) — represent structurally.
        out = format!("[{} non-text content block(s)]", content.len());
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(reconnect_delay_ms(0), 500);
        assert_eq!(reconnect_delay_ms(1), 1000);
        assert_eq!(reconnect_delay_ms(2), 2000);
        assert!(reconnect_delay_ms(10) <= 30_000);
        assert_eq!(reconnect_delay_ms(10), 30_000);
    }

    #[test]
    fn disabled_config_starts_disabled() {
        let cfg = ServerConfig::Stdio {
            command: "x".into(),
            args: vec![],
            env: Default::default(),
            disabled: true,
        };
        let conn = McpConnection::new("s", cfg);
        assert_eq!(conn.status(), ServerStatus::Disabled);
    }

    #[test]
    fn enabled_config_starts_pending() {
        let cfg = ServerConfig::Stdio {
            command: "x".into(),
            args: vec![],
            env: Default::default(),
            disabled: false,
        };
        let conn = McpConnection::new("s", cfg);
        assert_eq!(conn.status(), ServerStatus::Pending);
    }
}
