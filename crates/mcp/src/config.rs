//! `mcp.json` parsing, env-var expansion, and read/write.
//!
//! Standard MCP config format (cross-agent compatible):
//! ```json
//! { "mcpServers": { "name": { "command": "...", "args": [...], "env": {...} } } }
//! ```
//! A server entry with no `type` field defaults to `stdio` (backwards
//! compatible with the common form most servers document).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// A single MCP server configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        disabled: bool,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        disabled: bool,
    },
    Sse {
        url: String,
        headers: BTreeMap<String, String>,
        disabled: bool,
    },
}

impl ServerConfig {
    pub fn is_disabled(&self) -> bool {
        match self {
            ServerConfig::Stdio { disabled, .. }
            | ServerConfig::Http { disabled, .. }
            | ServerConfig::Sse { disabled, .. } => *disabled,
        }
    }

    /// Transport label for UI display.
    pub fn transport(&self) -> &'static str {
        match self {
            ServerConfig::Stdio { .. } => "stdio",
            ServerConfig::Http { .. } => "http",
            ServerConfig::Sse { .. } => "sse",
        }
    }
}

/// Parsed `mcp.json`.
#[derive(Debug, Clone, Default)]
pub struct McpConfig {
    pub servers: BTreeMap<String, ServerConfig>,
}

// ── Raw deserialization shapes ──────────────────────────────────────────────

#[derive(Deserialize)]
struct RawFile {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: BTreeMap<String, serde_json::Value>,
}

impl McpConfig {
    /// Parse `mcp.json` text. Entries with no `type` default to `stdio`.
    pub fn parse(s: &str) -> Result<Self> {
        let raw: RawFile = serde_json::from_str(s).context("invalid mcp.json")?;
        let mut servers = BTreeMap::new();
        for (name, val) in raw.mcp_servers {
            let cfg = parse_server(&val)
                .with_context(|| format!("invalid config for MCP server '{name}'"))?;
            servers.insert(name, cfg);
        }
        Ok(McpConfig { servers })
    }

    /// Expand `${VAR}` references in command/args/env/url/headers using the
    /// process environment. Missing vars are left as-is (logged).
    pub fn expand_env(mut self) -> Self {
        for cfg in self.servers.values_mut() {
            match cfg {
                ServerConfig::Stdio {
                    command, args, env, ..
                } => {
                    *command = expand(command);
                    for a in args.iter_mut() {
                        *a = expand(a);
                    }
                    for v in env.values_mut() {
                        *v = expand(v);
                    }
                }
                ServerConfig::Http { url, headers, .. }
                | ServerConfig::Sse { url, headers, .. } => {
                    *url = expand(url);
                    for v in headers.values_mut() {
                        *v = expand(v);
                    }
                }
            }
        }
        self
    }

    /// Default global config path: `<data_dir>/crown/mcp.json`.
    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("crown")
            .join("mcp.json")
    }


    /// Resolve the global `mcp.json` under an explicit data root (injected
    /// by the app from `CrownPaths`).
    pub fn path_in_root(root: &Path) -> PathBuf {
        root.join("mcp.json")
    }

    /// Load the trusted global config from the default path. Convenience for
    /// call sites that don't inject a custom data root.
    pub fn load_default_trusted() -> Self {
        Self::load_trusted(&Self::default_path())
    }

    /// Load the trusted global `mcp.json` only. Env vars are expanded. A
    /// missing file yields an empty config.
    ///
    /// ## Security (P0-4)
    ///
    /// We deliberately do NOT auto-merge a project-local `<cwd>/.mcp.json`.
    /// Doing so would let "clone a repo and open it" launch arbitrary stdio
    /// `command`s with no user approval (a stdio server's `command` is
    /// executed as a child process). Only the global, user-controlled config
    /// is trusted. Project-scoped servers, if ever needed, must go through an
    /// explicit, user-approved path — never a silent merge.
    pub fn load_trusted(global: &Path) -> Self {
        let mut merged = BTreeMap::new();
        if let Ok(cfg) = read_file(global) {
            merged.extend(cfg.servers);
        }
        McpConfig { servers: merged }.expand_env()
    }

    /// Add (or replace) a server in the global `mcp.json`, writing atomically.
    pub fn add_server(name: &str, config: serde_json::Value) -> Result<()> {
        Self::add_server_at(&Self::default_path(), name, config)
    }

    /// Like [`add_server`](Self::add_server) but writes to an explicit path
    /// (the app injects the CrownPaths-derived `mcp.json` so there is a single
    /// path authority — see P1-16).
    pub fn add_server_at(path: &Path, name: &str, config: serde_json::Value) -> Result<()> {
        if name.is_empty()
            || name.contains(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        {
            return Err(anyhow!(
                "invalid server name '{name}': use letters, numbers, hyphens, underscores"
            ));
        }
        // Validate the config parses.
        parse_server(&config).context("invalid server config")?;

        let mut doc =
            read_raw_json(path).unwrap_or_else(|| serde_json::json!({ "mcpServers": {} }));
        let obj = doc
            .get_mut("mcpServers")
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| anyhow!("mcp.json missing mcpServers object"))?;
        obj.insert(name.to_string(), config);
        write_atomic(path, &doc)
    }

    /// Remove a server from the global `mcp.json`.
    pub fn remove_server(name: &str) -> Result<()> {
        Self::remove_server_at(&Self::default_path(), name)
    }

    /// Like [`remove_server`](Self::remove_server) but at an explicit path.
    pub fn remove_server_at(path: &Path, name: &str) -> Result<()> {
        let mut doc = read_raw_json(path).ok_or_else(|| anyhow!("mcp.json not found"))?;
        let obj = doc
            .get_mut("mcpServers")
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| anyhow!("mcp.json missing mcpServers object"))?;
        if obj.remove(name).is_none() {
            return Err(anyhow!("no MCP server named '{name}'"));
        }
        write_atomic(path, &doc)
    }

    /// Toggle the `disabled` flag of a server in the global `mcp.json`.
    pub fn set_disabled(name: &str, disabled: bool) -> Result<()> {
        Self::set_disabled_at(&Self::default_path(), name, disabled)
    }

    /// Like [`set_disabled`](Self::set_disabled) but at an explicit path.
    pub fn set_disabled_at(path: &Path, name: &str, disabled: bool) -> Result<()> {
        let mut doc = read_raw_json(path).ok_or_else(|| anyhow!("mcp.json not found"))?;
        let entry = doc
            .get_mut("mcpServers")
            .and_then(|v| v.as_object_mut())
            .and_then(|o| o.get_mut(name))
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| anyhow!("no MCP server named '{name}'"))?;
        entry.insert("disabled".to_string(), serde_json::Value::Bool(disabled));
        write_atomic(path, &doc)
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn parse_server(val: &serde_json::Value) -> Result<ServerConfig> {
    let obj = val
        .as_object()
        .ok_or_else(|| anyhow!("server config must be an object"))?;
    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("stdio");
    let disabled = obj
        .get("disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match ty {
        "stdio" => {
            let command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("stdio server requires 'command'"))?
                .to_string();
            let args = obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let env = obj
                .get("env")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            Ok(ServerConfig::Stdio {
                command,
                args,
                env,
                disabled,
            })
        }
        "http" | "sse" => {
            let url = obj
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("{ty} server requires 'url'"))?
                .to_string();
            let headers = obj
                .get("headers")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            if ty == "http" {
                Ok(ServerConfig::Http {
                    url,
                    headers,
                    disabled,
                })
            } else {
                Ok(ServerConfig::Sse {
                    url,
                    headers,
                    disabled,
                })
            }
        }
        other => Err(anyhow!("unknown MCP server type '{other}'")),
    }
}

/// Expand `${VAR}` using the process environment. Unknown vars stay literal.
fn expand(s: &str) -> String {
    // Compile once: this runs for every command/arg/env/header on every load.
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("valid env-var regex")
    });
    RE.replace_all(s, |caps: &regex::Captures| {
        let var = &caps[1];
        match std::env::var(var) {
            Ok(val) => val,
            Err(_) => {
                tracing::warn!(var, "mcp config references undefined env var");
                caps[0].to_string()
            }
        }
    })
    .into_owned()
}

fn read_file(path: &Path) -> Result<McpConfig> {
    let s = std::fs::read_to_string(path)?;
    McpConfig::parse(&s)
}

fn read_raw_json(path: &Path) -> Option<serde_json::Value> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_atomic(path: &Path, doc: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    let pretty = serde_json::to_string_pretty(doc)?;
    std::fs::write(&tmp, pretty)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stdio_and_http_servers() {
        let json = r#"{"mcpServers":{
            "fs":{"command":"npx","args":["-y","x"],"env":{"K":"v"}},
            "gh":{"type":"http","url":"https://x/mcp","headers":{"A":"B"}}
        }}"#;
        let cfg = McpConfig::parse(json).unwrap();
        assert_eq!(cfg.servers.len(), 2);
        match &cfg.servers["fs"] {
            ServerConfig::Stdio { command, args, .. } => {
                assert_eq!(command, "npx");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected stdio"),
        }
        match &cfg.servers["gh"] {
            ServerConfig::Http { url, .. } => assert_eq!(url, "https://x/mcp"),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn expands_env_vars() {
        std::env::set_var("MCP_TEST_TOK", "secret123");
        let json = r#"{"mcpServers":{"s":{"type":"http","url":"https://x","headers":{"Authorization":"Bearer ${MCP_TEST_TOK}"}}}}"#;
        let cfg = McpConfig::parse(json).unwrap().expand_env();
        match &cfg.servers["s"] {
            ServerConfig::Http { headers, .. } => {
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer secret123")
            }
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_type_defaults_to_stdio() {
        let json = r#"{"mcpServers":{"s":{"command":"foo"}}}"#;
        let cfg = McpConfig::parse(json).unwrap();
        assert!(matches!(cfg.servers["s"], ServerConfig::Stdio { .. }));
    }

    #[test]
    fn missing_var_stays_literal() {
        let json = r#"{"mcpServers":{"s":{"command":"${DEFINITELY_NOT_SET_XYZ}"}}}"#;
        let cfg = McpConfig::parse(json).unwrap().expand_env();
        match &cfg.servers["s"] {
            ServerConfig::Stdio { command, .. } => {
                assert_eq!(command, "${DEFINITELY_NOT_SET_XYZ}")
            }
            _ => panic!(),
        }
    }

    #[test]
    fn rejects_stdio_without_command() {
        let json = r#"{"mcpServers":{"s":{"type":"stdio"}}}"#;
        assert!(McpConfig::parse(json).is_err());
    }

    #[test]
    fn path_from_root_is_under_given_root() {
        use std::path::PathBuf;
        let got = McpConfig::path_in_root(&PathBuf::from("/data/crown"));
        assert_eq!(got, PathBuf::from("/data/crown/mcp.json"));
    }

    /// Security (P0-4): a project-local `<cwd>/.mcp.json` must NEVER be loaded
    /// automatically. Auto-loading it would let "clone a repo + open it" run
    /// arbitrary stdio `command`s with no user approval. Only the global,
    /// user-controlled `mcp.json` is trusted.
    #[test]
    fn project_local_mcp_json_is_not_auto_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        // A hostile project drops a .mcp.json that would launch a command.
        let proj = tmp.path().join("project");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join(".mcp.json"),
            r#"{"mcpServers":{"evil":{"command":"calc.exe","args":[]}}}"#,
        )
        .unwrap();

        // Global config lives elsewhere and is empty/absent.
        let global = tmp.path().join("crown").join("mcp.json");

        let cfg = McpConfig::load_trusted(&global);
        assert!(
            !cfg.servers.contains_key("evil"),
            "project-local .mcp.json must not be auto-loaded (got servers: {:?})",
            cfg.servers.keys().collect::<Vec<_>>()
        );
        assert!(cfg.servers.is_empty(), "no global file → empty config");
    }

    /// The global (user-controlled) mcp.json IS loaded.
    #[test]
    fn global_mcp_json_is_loaded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let global = tmp.path().join("mcp.json");
        std::fs::write(
            &global,
            r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","x"]}}}"#,
        )
        .unwrap();
        let cfg = McpConfig::load_trusted(&global);
        assert!(cfg.servers.contains_key("fs"));
    }
}
