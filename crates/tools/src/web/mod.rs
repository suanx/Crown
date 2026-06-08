//! Web search and fetch tools.
//!
//! Two tools for internet access:
//! - `web_search` — search the web via configurable providers (Jina default)
//! - `web_fetch` — fetch and extract content from a URL

pub mod config;
pub mod fetch;
pub mod search;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::permission::{DecisionReason, PermissionMode, PermissionResult};
use crate::types::ToolError;
use crate::Tool;
use crate::ToolContext;
use config::WebConfig;
use fetch::FetchCache;

/// Shared state for web tools (HTTP client + cache + config).
pub struct WebToolsState {
    pub client: Client,
    pub cache: FetchCache,
    config: RwLock<WebConfig>,
}

impl WebToolsState {
    pub fn new(config: WebConfig) -> Self {
        // `redirect(none)` is load-bearing: fetch.rs handles redirects
        // manually with same-host SSRF checks. A default Client (auto-follow
        // redirects) would silently bypass that, so we must NOT fall back to
        // one — a builder failure here is a fatal startup error.
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("failed to build web HTTP client (manual-redirect policy)");
        Self {
            client,
            cache: FetchCache::new(),
            config: RwLock::new(config),
        }
    }

    pub fn config(&self) -> WebConfig {
        self.config.read().clone()
    }

    pub fn set_config(&self, config: WebConfig) {
        *self.config.write() = config;
    }
}

impl Default for WebToolsState {
    fn default() -> Self {
        Self::new(WebConfig::default())
    }
}

// ─── WebSearchTool ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WebSearchArgs {
    query: String,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

fn default_max_results() -> usize {
    5
}

/// Search the web for current information.
pub struct WebSearchTool {
    state: Arc<WebToolsState>,
}

impl WebSearchTool {
    pub fn new(state: Arc<WebToolsState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Cancel
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        if query.trim().is_empty() {
            return Err("'query' is required and must not be empty".into());
        }
        if query.len() > 500 {
            return Err("'query' must be 500 characters or fewer".into());
        }
        Ok(())
    }

    async fn check_permissions(&self, _input: &Value, mode: PermissionMode) -> PermissionResult {
        if mode == PermissionMode::Plan {
            return PermissionResult::Ask {
                message: "Plan mode: web_search requires approval.".into(),
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::Plan,
                }),
                suggestions: vec![],
            };
        }
        PermissionResult::Passthrough {
            message: "web_search requires permission".into(),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: WebSearchArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
                tool: self.name().into(),
                message: e.to_string(),
            })?;

        // Race the provider call against turn abort so a slow search (up to
        // the 30s provider timeout) terminates promptly when the user stops
        // the turn — mirrors web_fetch.
        let config = self.state.config();
        let results = tokio::select! {
            _ = ctx.abort.cancelled() => return Err(ToolError::Aborted),
            r = search::web_search(
                &args.query,
                args.max_results,
                &config,
                &self.state.client,
            ) => r.map_err(ToolError::ExecutionFailed)?,
        };

        if results.is_empty() {
            return Ok(format!("No results found for: {}", args.query));
        }

        let mut output = format!("Search results for: {}\n\n", args.query);
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. {}\n   URL: {}\n   {}\n\n",
                i + 1,
                r.title,
                r.url,
                r.snippet
            ));
        }
        Ok(output)
    }
}

// ─── WebFetchTool ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct WebFetchArgs {
    url: String,
    #[serde(default)]
    prompt: Option<String>,
}

/// Fetch and extract content from a URL.
pub struct WebFetchTool {
    state: Arc<WebToolsState>,
}

impl WebFetchTool {
    pub fn new(state: Arc<WebToolsState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn interrupt_behavior(&self) -> crate::InterruptBehavior {
        crate::InterruptBehavior::Cancel
    }

    async fn validate_input(&self, input: &Value) -> Result<(), String> {
        let url = input.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.trim().is_empty() {
            return Err("'url' is required and must not be empty".into());
        }
        // Pre-validate URL structure
        fetch::validate_url(url)?;
        Ok(())
    }

    async fn check_permissions(&self, _input: &Value, mode: PermissionMode) -> PermissionResult {
        if mode == PermissionMode::Plan {
            return PermissionResult::Ask {
                message: "Plan mode: web_fetch requires approval.".into(),
                decision_reason: Some(DecisionReason::Mode {
                    mode: PermissionMode::Plan,
                }),
                suggestions: vec![],
            };
        }
        PermissionResult::Passthrough {
            message: "web_fetch requires permission".into(),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
        let args: WebFetchArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs {
                tool: self.name().into(),
                message: e.to_string(),
            })?;

        let result = tokio::select! {
            _ = ctx.abort.cancelled() => return Err(ToolError::Aborted),
            r = fetch::fetch_url(&args.url, &self.state.cache, &self.state.client) => {
                r.map_err(ToolError::ExecutionFailed)?
            }
        };

        let mut output = format!(
            "=== Fetched: {} (HTTP {}) ===\n\n",
            result.url, result.status
        );

        if let Some(prompt) = &args.prompt {
            output.push_str(&format!("[Prompt: {}]\n\n", prompt));
        }

        output.push_str(&result.content);
        Ok(output)
    }
}
