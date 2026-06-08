//! LIVE full-turn integration: drive `AgentEngine::send_message` against real
//! endpoints and assert the turn completes and emits a `ContextUsage` event.
//!
//! This exercises the complete turn loop WITH the new compaction hooks
//! (pre-fold turn-start estimate + post-response decision) live, on both a
//! DeepSeek thread and a non-DeepSeek (Xiaomi MiMo) thread. It proves the
//! hooks don't break a normal turn on either provider — the provider-
//! neutrality guarantee, end to end through the engine.
//!
//! `#[ignore]` by default. Run with credentials in env:
//! ```pwsh
//! $env:DEEPSEEK_API_KEY = "<key>"
//! $env:MIMO_API_KEY = "<key>"
//! cargo test -p deepseek-core --test engine_live_test -- --ignored --nocapture
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_state::{Database, ThreadInsert, ThreadRepo};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Allow-all gate: tests never need an interactive approval.
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

fn build_engine(api_key: String, base_url: String, db: Arc<Database>) -> AgentEngine {
    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(60),
        ..Default::default()
    })
    .expect("client");
    let tools = Arc::new(deepseek_tools::ToolRegistry::new()); // no tools: a plain Q&A turn
    let system_prompt = deepseek_core::prompt::build_system_prompt(None);
    AgentEngine::new(client, system_prompt, tools, Arc::new(AllowAllGate), db)
}

/// Create a thread row with the given provider + model, then load it so the
/// engine's in-memory state picks up those values.
fn make_thread(db: &Database, provider_id: &str, model: &str) -> String {
    let trepo = ThreadRepo::new(db);
    let row = trepo
        .create(ThreadInsert {
            name: Some("live".into()),
            model: model.into(),
            cwd: None,
            permission_mode: "default".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .expect("create thread");
    row.id
}

/// Drain events until the turn finishes; return whether a ContextUsage event
/// fired and whether the turn completed cleanly.
async fn run_turn(engine: &AgentEngine, thread_id: String, prompt: &str) -> (bool, bool, String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let engine_arc = engine;
    let tid = thread_id.clone();
    let prompt = prompt.to_string();

    // send_message borrows &self; run it concurrently with event draining.
    let mut saw_context_usage = false;
    let mut turn_complete = false;
    let mut content = String::new();

    let send_fut = engine_arc.send_message(tid, prompt, tx);
    tokio::pin!(send_fut);

    let mut send_done = false;
    let mut send_result: anyhow::Result<()> = Ok(());
    loop {
        tokio::select! {
            r = &mut send_fut, if !send_done => {
                send_done = true;
                send_result = r;
            }
            ev = rx.recv() => {
                match ev {
                    Some(EngineEvent::ContextUsage { used_tokens, max_tokens, ratio, source, .. }) => {
                        saw_context_usage = true;
                        eprintln!(
                            "ContextUsage: used={used_tokens} max={max_tokens} ratio={ratio:.4} source={source:?}"
                        );
                    }
                    Some(EngineEvent::ContentDelta { delta, .. }) => content.push_str(&delta),
                    Some(EngineEvent::TurnComplete { usage, cost_usd, .. }) => {
                        turn_complete = true;
                        eprintln!("TurnComplete: prompt_tokens={} cost_usd={cost_usd}", usage.prompt_tokens);
                    }
                    Some(EngineEvent::Error { error, .. }) => eprintln!("Error event: {error}"),
                    Some(_) => {}
                    None => {
                        if send_done { break; }
                    }
                }
            }
        }
        if send_done && rx.is_empty() {
            break;
        }
    }
    send_result.expect("send_message should not return a hard error");
    (saw_context_usage, turn_complete, content)
}

#[tokio::test]
#[ignore = "live network; needs DEEPSEEK_API_KEY"]
async fn live_deepseek_full_turn_emits_context_usage() {
    let api_key = match std::env::var("DEEPSEEK_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: DEEPSEEK_API_KEY not set");
            return;
        }
    };
    let base_url = std::env::var("DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
    let tmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
    let engine = build_engine(api_key, base_url, db.clone());
    let tid = make_thread(&db, "deepseek", "deepseek-v4-flash");

    let (usage, complete, content) =
        run_turn(&engine, tid, "Reply with exactly the word: PONG").await;

    eprintln!("\n=== DeepSeek turn content ===\n{}\n", content.trim());
    assert!(complete, "turn must complete");
    assert!(usage, "ContextUsage event must fire (api source)");
    assert!(!content.trim().is_empty(), "model must produce content");
}

#[tokio::test]
#[ignore = "live network; needs MIMO_API_KEY"]
async fn live_mimo_full_turn_unaffected_by_compaction_hooks() {
    let api_key = match std::env::var("MIMO_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: MIMO_API_KEY not set");
            return;
        }
    };
    let base_url = std::env::var("MIMO_BASE_URL")
        .unwrap_or_else(|_| "https://token-plan-cn.xiaomimimo.com/v1".to_string());
    let model = std::env::var("MIMO_MODEL").unwrap_or_else(|_| "mimo-v2.5".to_string());
    let tmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
    let engine = build_engine(api_key, base_url, db.clone());
    // provider_id "other" → ProviderId::Other → no DeepSeek-specific paths.
    let tid = make_thread(&db, "other", &model);

    let (usage, complete, content) =
        run_turn(&engine, tid, "Reply with exactly the word: PONG").await;

    eprintln!("\n=== MiMo turn content ===\n{}\n", content.trim());
    assert!(
        complete,
        "non-DeepSeek turn must complete — compaction hooks must not break it"
    );
    assert!(usage, "ContextUsage event must fire for non-DeepSeek too");
    assert!(!content.trim().is_empty(), "MiMo must produce content");
}
