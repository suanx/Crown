//! LIVE: a real LM model installs an MCP server on request via mcp_install,
//! then the server's tools become available.
//!
//! Uses a temp HOME/APPDATA so the test writes its own mcp.json, not the
//! user's. `#[ignore]` by default.
//!
//! Run: $env:DEEPSEEK_API_KEY="<key>"; cargo test -p deepseek-mcp --test live_yolo_install_test -- --ignored --nocapture

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_mcp::install_tools::register_install_tools;
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
        _a: CancellationToken,
    ) -> Result<ApprovalDecision, GateError> {
        Ok(ApprovalDecision::Allow {
            updated_input: req.input,
            permission_updates: Vec::new(),
        })
    }
}

fn creds() -> Option<(String, String, String, &'static str)> {
    if let Ok(k) = std::env::var("MCP_TEST_API_KEY") {
        if !k.is_empty() {
            return Some((
                k,
                std::env::var("MCP_TEST_BASE_URL")
                    .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                std::env::var("MCP_TEST_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
                "other",
            ));
        }
    }
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.is_empty() {
            return Some((
                k,
                "https://api.deepseek.com".into(),
                "deepseek-v4-flash".into(),
                "deepseek",
            ));
        }
    }
    None
}

#[tokio::test]
#[ignore = "live: needs npx + a real LM API key"]
async fn real_model_installs_mcp_server() {
    let (api_key, base_url, model, provider_id) = match creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY or MCP_TEST_API_KEY");
            return;
        }
    };

    // Redirect data_dir so mcp.json writes into a temp dir, not the real one.
    let home = TempDir::new().unwrap();
    #[cfg(windows)]
    std::env::set_var("APPDATA", home.path());
    #[cfg(not(windows))]
    std::env::set_var("XDG_DATA_HOME", home.path());

    let registry = Arc::new(ToolRegistry::new());
    let mcp = Arc::new(McpManager::new());
    register_install_tools(&registry, &mcp);

    // Keep the registry in sync as the server connects (mirrors app wiring).
    {
        let mcp_ev = mcp.clone();
        let reg_ev = registry.clone();
        let mut rx = mcp.subscribe();
        tokio::spawn(async move {
            use deepseek_mcp::manager::McpEvent;
            while let Ok(ev) = rx.recv().await {
                if matches!(ev, McpEvent::ToolsChanged) {
                    deepseek_mcp::sync_registry_tools(&reg_ev, &mcp_ev);
                }
            }
        });
    }

    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(120),
        ..Default::default()
    })
    .expect("client");
    let dbtmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(dbtmp.path().join("state.db")).unwrap());
    let mcp_path = deepseek_mcp::config::McpConfig::default_path()
        .to_string_lossy()
        .into_owned();
    let skills_dir = "/tmp/skills".to_string();
    let system_prompt = deepseek_core::prompt::build_system_prompt_with_paths(
        None,
        Some(&mcp_path),
        Some(&skills_dir),
    );
    let engine = AgentEngine::new(
        client,
        system_prompt,
        registry.clone(),
        Arc::new(AllowAllGate),
        db.clone(),
    );

    let tid = ThreadRepo::new(&db)
        .create(ThreadInsert {
            name: Some("yolo".into()),
            model: model.clone(),
            cwd: None,
            permission_mode: "bypassPermissions".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .unwrap()
        .id;

    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let prompt = "Install an MCP server named 'everything' that runs the command `npx` \
                  with args `-y` and `@modelcontextprotocol/server-everything`. Use the mcp_install tool.";
    let send_fut = engine.send_message(tid, prompt.to_string(), tx);
    tokio::pin!(send_fut);
    let mut install_called = false;
    let mut install_result = String::new();
    let mut done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !done => { done = true; let _ = r; }
            ev = rx.recv() => match ev {
                Some(EngineEvent::ToolCallStart { tool_name, .. }) => {
                    eprintln!("[tool_call_start] {tool_name}");
                    if tool_name == "mcp_install" { install_called = true; }
                }
                Some(EngineEvent::ToolCallUpdate { result: Some(r), .. }) => { install_result = r; }
                Some(_) => {}
                None => if done { break; },
            }
        }
        if done && rx.is_empty() {
            break;
        }
    }

    eprintln!("=== mcp_install result ===\n{install_result}");
    let connected = mcp.status("everything") == Some(deepseek_mcp::types::ServerStatus::Connected);
    let echo_present = registry
        .list_names()
        .iter()
        .any(|n| n == "mcp__everything__echo");
    eprintln!("connected={connected} echo_tool_present={echo_present}");
    mcp.shutdown_all().await;

    assert!(install_called, "model should call mcp_install");
    assert!(
        connected,
        "everything server should be connected after install"
    );
    assert!(
        echo_present,
        "echo tool should be live in the registry after install"
    );
}
