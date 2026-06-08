//! Multi-server connection pool: lifecycle, reconnect, health check, events.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::config::{McpConfig, ServerConfig};
use crate::connection::{reconnect_delay_ms, McpConnection};
use crate::types::{McpToolInfo, ServerStatus};

/// Max reconnect attempts before giving up (server stays Failed).
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
/// Health-check interval for connected servers.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Events broadcast by the manager so the app layer can update UI / registry.
#[derive(Clone, Debug)]
pub enum McpEvent {
    StatusChanged {
        name: String,
        status: ServerStatus,
        error: Option<String>,
    },
    /// The set of available tools changed (a server connected/disconnected).
    ToolsChanged,
}

/// Owns all MCP server connections and their background tasks.
pub struct McpManager {
    connections: RwLock<BTreeMap<String, Arc<McpConnection>>>,
    tasks: RwLock<BTreeMap<String, tokio::task::JoinHandle<()>>>,
    event_tx: broadcast::Sender<McpEvent>,
    /// Authoritative path to the global `mcp.json`. Set once by the app from
    /// CrownPaths so every mutation/reload uses the SAME file (P1-16) instead
    /// of recomputing `dirs::data_dir()` independently. `None` falls back to
    /// [`McpConfig::default_path`].
    config_path: RwLock<Option<std::path::PathBuf>>,
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            connections: RwLock::new(BTreeMap::new()),
            tasks: RwLock::new(BTreeMap::new()),
            event_tx,
            config_path: RwLock::new(None),
        }
    }

    /// Set the authoritative `mcp.json` path (called once by the app layer
    /// with the CrownPaths-derived path). All subsequent reloads/mutations
    /// route through [`Self::config_path`].
    pub fn set_config_path(&self, path: std::path::PathBuf) {
        *self.config_path.write() = Some(path);
    }

    /// The authoritative `mcp.json` path, or the default if unset.
    pub fn config_path(&self) -> std::path::PathBuf {
        self.config_path
            .read()
            .clone()
            .unwrap_or_else(McpConfig::default_path)
    }

    /// Load the trusted config from [`Self::config_path`] and (re)connect.
    /// Single entry point so command handlers don't recompute the path.
    pub async fn reload_from_disk(self: &Arc<Self>) {
        let cfg = McpConfig::load_trusted(&self.config_path());
        self.reload(cfg).await;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<McpEvent> {
        self.event_tx.subscribe()
    }

    fn emit(&self, ev: McpEvent) {
        let _ = self.event_tx.send(ev);
    }

    /// Load all servers from config and start connecting the enabled ones.
    pub async fn load_from_config(self: &Arc<Self>, cfg: McpConfig) {
        for (name, server_cfg) in cfg.servers {
            self.add_connection(&name, server_cfg).await;
        }
    }

    /// Add a single server connection and (if enabled) spawn its connect loop.
    pub async fn add_connection(self: &Arc<Self>, name: &str, cfg: ServerConfig) {
        let conn = McpConnection::new(name, cfg.clone());
        self.connections
            .write()
            .insert(name.to_string(), conn.clone());

        if cfg.is_disabled() {
            self.emit(McpEvent::StatusChanged {
                name: name.to_string(),
                status: ServerStatus::Disabled,
                error: None,
            });
            return;
        }

        let mgr = Arc::clone(self);
        let name_owned = name.to_string();
        let handle = tokio::spawn(async move {
            mgr.connect_with_retry(name_owned, conn).await;
        });
        self.tasks.write().insert(name.to_string(), handle);
    }

    /// Connect loop with exponential backoff, then health-check loop.
    async fn connect_with_retry(self: Arc<Self>, name: String, conn: Arc<McpConnection>) {
        let mut attempt = 0u32;
        loop {
            self.emit(McpEvent::StatusChanged {
                name: name.clone(),
                status: ServerStatus::Pending,
                error: None,
            });
            match conn.connect().await {
                Ok(()) => {
                    self.emit(McpEvent::StatusChanged {
                        name: name.clone(),
                        status: ServerStatus::Connected,
                        error: None,
                    });
                    self.emit(McpEvent::ToolsChanged);
                    // Enter health-check loop; on failure, break to reconnect.
                    self.health_check_loop(&name, &conn).await;
                    attempt = 0; // reset after a healthy session ends
                }
                Err(e) => {
                    self.emit(McpEvent::StatusChanged {
                        name: name.clone(),
                        status: ServerStatus::Failed,
                        error: Some(e.to_string()),
                    });
                }
            }

            attempt += 1;
            if attempt > MAX_RECONNECT_ATTEMPTS {
                tracing::warn!(server = %name, "MCP server exceeded reconnect attempts; giving up");
                return;
            }
            tokio::time::sleep(Duration::from_millis(reconnect_delay_ms(attempt - 1))).await;
        }
    }

    /// Periodically verify the connection is alive. Returns when it dies.
    async fn health_check_loop(&self, name: &str, conn: &Arc<McpConnection>) {
        loop {
            tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
            if conn.status() != ServerStatus::Connected {
                return;
            }
            // Use a lightweight tool call (list) as ping by re-reading cached
            // state; a real liveness probe re-lists tools.
            if !conn.is_alive().await {
                self.emit(McpEvent::StatusChanged {
                    name: name.to_string(),
                    status: ServerStatus::Failed,
                    error: Some("health check failed".into()),
                });
                self.emit(McpEvent::ToolsChanged);
                return;
            }
        }
    }

    /// Remove a server: abort its task, shut down the connection.
    pub async fn remove_connection(&self, name: &str) {
        if let Some(handle) = self.tasks.write().remove(name) {
            handle.abort();
        }
        let conn = self.connections.write().remove(name);
        if let Some(conn) = conn {
            conn.shutdown().await;
        }
        self.emit(McpEvent::ToolsChanged);
    }

    pub fn status(&self, name: &str) -> Option<ServerStatus> {
        self.connections.read().get(name).map(|c| c.status())
    }

    /// List (name, status) for all configured servers.
    pub fn list_servers(&self) -> Vec<(String, ServerStatus)> {
        self.connections
            .read()
            .iter()
            .map(|(n, c)| (n.clone(), c.status()))
            .collect()
    }

    /// Connection handle by name (for tool calls from the bridge).
    pub fn connection(&self, name: &str) -> Option<Arc<McpConnection>> {
        self.connections.read().get(name).cloned()
    }

    /// All tools across connected servers, as (server, tool).
    pub fn all_tools(&self) -> Vec<(String, McpToolInfo)> {
        let mut out = Vec::new();
        for (name, conn) in self.connections.read().iter() {
            if conn.status() == ServerStatus::Connected {
                for t in conn.tools() {
                    out.push((name.clone(), t));
                }
            }
        }
        out
    }

    /// Call a tool on a named server.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<String> {
        let conn = self
            .connection(server)
            .ok_or_else(|| anyhow::anyhow!("unknown MCP server '{server}'"))?;
        conn.call_tool(tool, args).await
    }

    /// Reload: diff current vs new config, stop removed, start added/changed.
    pub async fn reload(self: &Arc<Self>, cfg: McpConfig) {
        let current: Vec<String> = self.connections.read().keys().cloned().collect();
        let new_names: Vec<String> = cfg.servers.keys().cloned().collect();

        // Remove servers no longer in config.
        for name in &current {
            if !new_names.contains(name) {
                self.remove_connection(name).await;
            }
        }
        // Add or restart servers in config.
        for (name, server_cfg) in cfg.servers {
            // Restart if config changed or not present.
            let changed = self
                .connections
                .read()
                .get(&name)
                .map(|c| c.config() != &server_cfg)
                .unwrap_or(true);
            if changed {
                if current.contains(&name) {
                    self.remove_connection(&name).await;
                }
                self.add_connection(&name, server_cfg).await;
            }
        }
        self.emit(McpEvent::ToolsChanged);
    }

    /// Shut down all connections (app exit).
    pub async fn shutdown_all(&self) {
        let names: Vec<String> = self.connections.read().keys().cloned().collect();
        for name in names {
            self.remove_connection(&name).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manager_starts_empty() {
        let m = McpManager::new();
        assert_eq!(m.list_servers().len(), 0);
    }

    #[tokio::test]
    async fn disabled_server_is_not_connected() {
        let m = Arc::new(McpManager::new());
        let cfg = ServerConfig::Stdio {
            command: "x".into(),
            args: vec![],
            env: Default::default(),
            disabled: true,
        };
        m.add_connection("s", cfg).await;
        assert_eq!(m.status("s"), Some(ServerStatus::Disabled));
        assert_eq!(m.all_tools().len(), 0);
    }
}
