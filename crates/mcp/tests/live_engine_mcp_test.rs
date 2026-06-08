//! LIVE end-to-end: a REAL LM model calls a REAL MCP tool through the full
//! engine turn loop.
//!
//! Wires: McpManager (connected to @modelcontextprotocol/server-everything via
//! stdio) → sync MCP tools into a ToolRegistry → AgentEngine with a real
//! DeepSeek (or MiMo) client → send a user message that should make the model
//! call the `echo` MCP tool → assert the tool actually ran.
//!
//! `#[ignore]` by default. Run with:
//! ```pwsh
//! $env:DEEPSEEK_API_KEY = "<key>"
//! cargo test -p deepseek-mcp --test live_engine_mcp_test -- --ignored --nocapture
//! # or MiMo:
//! $env:MCP_TEST_API_KEY="<key>"; $env:MCP_TEST_BASE_URL="https://token-plan-cn.xiaomimimo.com/v1"; $env:MCP_TEST_MODEL="mimo-v2.5"
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_mcp::config::ServerConfig;
use deepseek_mcp::manager::McpManager;
use deepseek_state::{Database, ThreadInsert, ThreadRepo};
use deepseek_tools::ToolRegistry;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct AllowAllGate;

#[async_trait]
impl PermissionGate for AllowAllGate {
    async fn ask(
        &self,
        req: ApprovalRequest,
        _abort: CancellationToken,
    ) -> Result<ApprovalDecision, GateError> {
        Ok(ApprovalDecision::Allow {
            updated_input: req.input,
            permission_updates: Vec::new(),
        })
    }
}

fn resolve_creds() -> Option<(String, String, String, &'static str)> {
    if let Ok(k) = std::env::var("MCP_TEST_API_KEY") {
        if !k.is_empty() {
            let base = std::env::var("MCP_TEST_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".into());
            let model =
                std::env::var("MCP_TEST_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into());
            return Some((k, base, model, "other"));
        }
    }
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.is_empty() {
            let base = std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".into());
            return Some((k, base, "deepseek-v4-flash".into(), "deepseek"));
        }
    }
    None
}

#[tokio::test]
#[ignore = "live: needs npx + a real LM API key"]
async fn real_model_calls_real_mcp_tool() {
    let (api_key, base_url, model, provider_id) = match resolve_creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY or MCP_TEST_API_KEY");
            return;
        }
    };

    // 1. Connect the everything MCP server and sync its tools into a registry.
    let registry = Arc::new(ToolRegistry::new());
    let mcp = Arc::new(McpManager::new());
    mcp.add_connection(
        "everything",
        ServerConfig::Stdio {
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-everything".into(),
            ],
            env: Default::default(),
            disabled: false,
        },
    )
    .await;

    // Wait for it to connect (up to ~30s for npx cold start).
    let mut connected = false;
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if mcp.status("everything") == Some(deepseek_mcp::types::ServerStatus::Connected) {
            connected = true;
            break;
        }
    }
    assert!(connected, "everything server must connect");
    deepseek_mcp::sync_registry_tools(&registry, &mcp);

    let mcp_tools: Vec<String> = registry
        .list_names()
        .into_iter()
        .filter(|n| n.starts_with("mcp__"))
        .collect();
    eprintln!("=== MCP tools in registry ===\n{mcp_tools:#?}");
    assert!(
        mcp_tools.iter().any(|n| n == "mcp__everything__echo"),
        "echo tool must be bridged into the registry"
    );

    // 2. Build the engine with a real LM client + the MCP-populated registry.
    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(120),
        ..Default::default()
    })
    .expect("client");
    let tmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
    let system_prompt = deepseek_core::prompt::build_system_prompt(None);
    let engine = AgentEngine::new(
        client,
        system_prompt,
        registry.clone(),
        Arc::new(AllowAllGate),
        db.clone(),
    );

    let trepo = ThreadRepo::new(&db);
    let tid = trepo
        .create(ThreadInsert {
            name: Some("mcp-live".into()),
            model: model.clone(),
            cwd: None,
            permission_mode: "bypassPermissions".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .expect("thread")
        .id;

    // 3. Ask the model to use the echo tool.
    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let prompt = "Use the mcp__everything__echo tool to echo the exact text \
                  MCP_LIVE_OK. Call the tool, do not just say it.";
    let send_fut = engine.send_message(tid, prompt.to_string(), tx);
    tokio::pin!(send_fut);

    let mut tool_called = false;
    let mut tool_result = String::new();
    let mut content = String::new();
    let mut send_done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !send_done => { send_done = true; let _ = r; }
            ev = rx.recv() => match ev {
                Some(EngineEvent::ToolCallStart { tool_name, .. }) => {
                    eprintln!("[tool_call_start] {tool_name}");
                    if tool_name == "mcp__everything__echo" { tool_called = true; }
                }
                Some(EngineEvent::ToolCallUpdate { tool_use_id, status, result, .. }) => {
                    eprintln!("[tool_call_update] {tool_use_id} {status:?}");
                    if let Some(r) = result { tool_result = r; }
                }
                Some(EngineEvent::ContentDelta { delta, .. }) => content.push_str(&delta),
                Some(_) => {}
                None => if send_done { break; },
            }
        }
        if send_done && rx.is_empty() {
            break;
        }
    }

    eprintln!("=== final content ===\n{}", content.trim());
    eprintln!("=== echo tool result ===\n{tool_result}");
    mcp.shutdown_all().await;

    assert!(tool_called, "model must have called the MCP echo tool");
    assert!(
        tool_result.contains("MCP_LIVE_OK"),
        "echo tool result must contain our text; got: {tool_result}"
    );
}
