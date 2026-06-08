//! LIVE: a real LM model uses the `task` tool to spawn a sub-agent that runs
//! on a restricted (read-only) tool set in its own sub-thread, then returns a
//! report. Validates the P4 sub-agent chain end-to-end with a real model.
//!
//! Run: $env:DEEPSEEK_API_KEY="<key>"; cargo test -p deepseek-core --test live_subagent_test -- --ignored --nocapture

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_core::pricing::ProviderId;
use deepseek_core::subagent::{find_agent, subagent_model_for};
use deepseek_state::{Database, MessageRepo, ThreadInsert, ThreadRepo};
use deepseek_tools::specs::register_default_tools;
use deepseek_tools::web::WebToolsState;
use deepseek_tools::{SubagentLauncher, ToolRegistry};
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

/// Test launcher mirroring the app-layer one, minus the Tauri event sink.
/// Records the sub-thread's restricted tool names so the test can assert
/// `task` was excluded (no recursion).
struct TestLauncher {
    client: DeepSeekClient,
    gate: Arc<dyn PermissionGate>,
    db: Arc<Database>,
    parent_tools: Arc<ToolRegistry>,
    saw_sub_tools: Arc<parking_lot::Mutex<Vec<String>>>,
}

#[async_trait]
impl SubagentLauncher for TestLauncher {
    async fn launch(
        &self,
        agent_type: String,
        prompt: String,
        resume_subagent_id: Option<String>,
        parent_thread_id: String,
        parent_abort: CancellationToken,
    ) -> Result<(String, Option<String>), String> {
        let agent = find_agent(&agent_type).ok_or_else(|| format!("unknown agent {agent_type}"))?;
        let trepo = ThreadRepo::new(self.db.as_ref());
        let parent = trepo.get(&parent_thread_id).map_err(|e| e.to_string())?;
        let provider = ProviderId::from_str_lossy(&parent.provider_id);
        let sub_model = subagent_model_for(provider, agent, &parent.model);

        let sub_thread_id = match resume_subagent_id {
            Some(id) => id,
            None => {
                trepo
                    .create(ThreadInsert {
                        name: Some(format!("[subagent:{}]", agent.name)),
                        model: sub_model,
                        cwd: parent.cwd.clone(),
                        permission_mode: "bypassPermissions".into(),
                        provider_id: parent.provider_id.clone(),
                        thinking_effort: Some(parent.thinking_effort.clone()),
                        parent_thread_id: Some(parent_thread_id.clone()),
                        project_id: parent.project_id.clone(),
                    })
                    .map_err(|e| e.to_string())?
                    .id
            }
        };

        let sub_registry = Arc::new(self.parent_tools.subset(agent.allowed_tools, &["task"]));
        *self.saw_sub_tools.lock() = sub_registry.list_names();

        let sub_engine = AgentEngine::new(
            self.client.clone(),
            agent.system_prompt.to_string(),
            sub_registry,
            self.gate.clone(),
            self.db.clone(),
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });

        let run = sub_engine.send_message(sub_thread_id.clone(), prompt, tx);
        tokio::select! {
            _ = parent_abort.cancelled() => sub_engine.abort_turn(&sub_thread_id),
            r = run => { r.map_err(|e| e.to_string())?; }
        }
        let _ = drain.await;

        let report = MessageRepo::new(self.db.as_ref())
            .load_by_thread(&sub_thread_id)
            .ok()
            .and_then(|rows| {
                rows.into_iter().rev().find_map(|m| {
                    if m.role != "assistant" {
                        return None;
                    }
                    let v: serde_json::Value = serde_json::from_str(&m.content_json).ok()?;
                    v.get("content")
                        .and_then(|c| c.as_str())
                        .filter(|s| !s.trim().is_empty())
                        .map(String::from)
                })
            })
            .unwrap_or_default();

        let resumable = if agent.one_shot {
            None
        } else {
            Some(sub_thread_id)
        };
        Ok((report, resumable))
    }
}

fn creds() -> Option<(String, String, String, &'static str)> {
    // Second-provider override (MiMo) to verify provider neutrality.
    if let Ok(k) = std::env::var("SUBAGENT_TEST_API_KEY") {
        if !k.is_empty() {
            return Some((
                k,
                std::env::var("SUBAGENT_TEST_BASE_URL")
                    .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                std::env::var("SUBAGENT_TEST_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
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
#[ignore = "live: needs a real LM API key"]
async fn real_model_spawns_explore_subagent() {
    let (api_key, base_url, model, provider_id) = match creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY");
            return;
        }
    };

    // Workspace with a file the explore sub-agent can read.
    let work = TempDir::new().unwrap();
    std::fs::write(
        work.path().join("secret.txt"),
        "The magic number for this project is ZEBRA-77.",
    )
    .unwrap();

    let registry = Arc::new(ToolRegistry::new());
    register_default_tools(&registry, Arc::new(WebToolsState::default()));

    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(180),
        ..Default::default()
    })
    .expect("client");
    let dbtmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(dbtmp.path().join("state.db")).unwrap());

    let gate: Arc<dyn PermissionGate> = Arc::new(AllowAllGate);
    let saw_sub_tools = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let launcher = Arc::new(TestLauncher {
        client: client.clone(),
        gate: gate.clone(),
        db: db.clone(),
        parent_tools: registry.clone(),
        saw_sub_tools: saw_sub_tools.clone(),
    });

    let system_prompt = deepseek_core::prompt::build_system_prompt(Some(work.path()));
    let engine = AgentEngine::new(client, system_prompt, registry, gate, db.clone());
    engine.set_subagent_launcher(launcher);

    let tid = ThreadRepo::new(&db)
        .create(ThreadInsert {
            name: Some("subagent-live".into()),
            model: model.clone(),
            cwd: Some(work.path().to_string_lossy().into_owned()),
            permission_mode: "bypassPermissions".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .unwrap()
        .id;

    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let prompt = "Use the task tool with agent_type 'explore' to investigate the file secret.txt \
                  in the working directory and report the magic number. Then tell me the magic number.";
    let send_fut = engine.send_message(tid, prompt.to_string(), tx);
    tokio::pin!(send_fut);

    let mut task_called = false;
    let mut sub_agent_events = 0;
    let mut final_text = String::new();
    let mut done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !done => { done = true; let _ = r; }
            ev = rx.recv() => match ev {
                Some(EngineEvent::ToolCallStart { tool_name, .. }) => {
                    eprintln!("[tool_call_start] {tool_name}");
                    if tool_name == "task" { task_called = true; }
                }
                Some(EngineEvent::ContentDelta { delta, .. }) => final_text.push_str(&delta),
                Some(_) => { sub_agent_events += 1; }
                None => if done { break; },
            }
        }
        if done && rx.is_empty() {
            break;
        }
    }

    let sub_tools = saw_sub_tools.lock().clone();
    eprintln!("=== sub-agent restricted tools ===\n{sub_tools:?}");
    eprintln!("=== final answer ===\n{final_text}");
    eprintln!("task_called={task_called} sub_agent_events={sub_agent_events}");

    assert!(task_called, "model should call the task tool");
    assert!(
        !sub_tools.is_empty() && !sub_tools.contains(&"task".to_string()),
        "sub-agent tools must exclude `task` (no recursion); got {sub_tools:?}"
    );
    assert!(
        sub_tools.contains(&"read_file".to_string()),
        "explore sub-agent should have read_file"
    );
    assert!(
        !sub_tools.contains(&"write_file".to_string()),
        "explore sub-agent must be read-only (no write_file)"
    );
    assert!(
        final_text.contains("ZEBRA-77"),
        "main agent should report the magic number found by the sub-agent; got: {final_text}"
    );
}
