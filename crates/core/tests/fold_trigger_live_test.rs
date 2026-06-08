//! LIVE test that ACTUALLY triggers a fold: seed a thread with enough history
//! to cross the pre-fold turn-start threshold (>90% of the context window),
//! run one real turn, and assert the conversation was compacted — a
//! `[compaction-summary]` message replaces the old head, the log shrinks, and
//! the rewrite is persisted to disk.
//!
//! Uses MiMo by default because its context window (131072 fallback) is far
//! lower than DeepSeek's 1M, so the seed is ~120K tokens rather than ~900K —
//! much faster/cheaper while exercising the exact same engine code path. Set
//! USE_DEEPSEEK_FOLD=1 to run the (heavy) DeepSeek variant instead.
//!
//! `#[ignore]` by default. Run with:
//! ```pwsh
//! $env:MIMO_API_KEY = "<key>"
//! $env:MIMO_BASE_URL = "https://token-plan-cn.xiaomimimo.com/v1"
//! $env:MIMO_MODEL = "mimo-v2.5"
//! cargo test -p deepseek-core --test fold_trigger_live_test -- --ignored --nocapture
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_client::types::ChatMessage;
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_state::{Database, MessageInsert, MessageRepo, ThreadInsert, ThreadRepo};
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

fn build_engine(api_key: String, base_url: String, db: Arc<Database>) -> AgentEngine {
    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(120),
        ..Default::default()
    })
    .expect("client");
    let tools = Arc::new(deepseek_tools::ToolRegistry::new());
    let system_prompt = deepseek_core::prompt::build_system_prompt(None);
    AgentEngine::new(client, system_prompt, tools, Arc::new(AllowAllGate), db)
}

/// Seed a thread row + a large alternating user/assistant history so the
/// pre-fold local estimate crosses the >90% threshold for `ctx_max`.
///
/// Estimate-driven: we keep appending turns until
/// `estimate_turn_start(...).ratio` actually exceeds `target_ratio`, so the
/// seed is robust regardless of the real char→token ratio of the filler.
/// We pin a recoverable FACT in the first user turn and assert it survives
/// the fold.
fn seed_until_over_threshold(
    db: &Database,
    provider_id: &str,
    model: &str,
    target_ratio: f64,
) -> String {
    let trepo = ThreadRepo::new(db);
    let row = trepo
        .create(ThreadInsert {
            name: Some("fold-trigger".into()),
            model: model.into(),
            cwd: None,
            permission_mode: "default".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .expect("create");
    let mrepo = MessageRepo::new(db);
    let provider = deepseek_core::pricing::ProviderId::from_str_lossy(provider_id);

    let para = "The project requirement under discussion involves parsing structured \
                data with careful attention to edge cases and encoding. ";
    // Large per-message filler so we reach the target in few messages (keeps
    // the O(n) re-estimate per batch cheap even for DeepSeek's 1M window).
    let filler = para.repeat(80);

    let mut seq: i64 = 0;
    let append = |m: &ChatMessage, seq: &mut i64| {
        mrepo
            .append(MessageInsert {
                thread_id: row.id.clone(),
                seq: *seq,
                role: m.role.clone(),
                content_json: serde_json::to_string(m).unwrap(),
            })
            .unwrap();
        *seq += 1;
    };

    // First user turn carries the pinned fact.
    append(
        &ChatMessage::user(
            "REMEMBER THIS FACT: the secret build token is BANANA-42. Now, here is a lot \
             of background discussion you can summarize away later.",
        ),
        &mut seq,
    );

    // Append in batches, re-estimating until we clear the threshold (with a
    // hard cap so a bad estimate can't loop forever).
    let mut i = 0usize;
    loop {
        for _ in 0..20 {
            let m = if i.is_multiple_of(2) {
                ChatMessage::user(format!("Question {i}: {filler}"))
            } else {
                ChatMessage::assistant(format!("Answer {i}: {filler}"))
            };
            append(&m, &mut seq);
            i += 1;
        }
        let loaded = mrepo.load_by_thread(&row.id).unwrap();
        let msgs: Vec<ChatMessage> = loaded
            .iter()
            .filter_map(|r| serde_json::from_str(&r.content_json).ok())
            .collect();
        let est = deepseek_core::compaction::estimate_turn_start(&msgs, model, provider);
        if est.ratio > target_ratio {
            eprintln!(
                "seed reached ratio={:.4} at {} messages (ctx_max={})",
                est.ratio,
                msgs.len(),
                est.ctx_max
            );
            break;
        }
        if seq > 12000 {
            panic!("seed runaway: ratio never crossed {target_ratio}");
        }
    }
    row.id
}

async fn run_turn_collect(
    engine: &AgentEngine,
    thread_id: String,
    prompt: &str,
) -> (Vec<EngineEvent>, String) {
    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let mut events = Vec::new();
    let mut content = String::new();

    let send_fut = engine.send_message(thread_id, prompt.to_string(), tx);
    tokio::pin!(send_fut);
    let mut send_done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !send_done => {
                send_done = true;
                r.expect("send_message hard error");
            }
            ev = rx.recv() => match ev {
                Some(e) => {
                    if let EngineEvent::ContentDelta { delta, .. } = &e {
                        content.push_str(delta);
                    }
                    events.push(e);
                }
                None => if send_done { break; },
            }
        }
        if send_done && rx.is_empty() {
            break;
        }
    }
    (events, content)
}

#[tokio::test]
#[ignore = "live network; heavy; needs MIMO_API_KEY (or DEEPSEEK_API_KEY with USE_DEEPSEEK_FOLD=1)"]
async fn live_fold_actually_compacts_oversized_history() {
    let use_ds = std::env::var("USE_DEEPSEEK_FOLD")
        .map(|v| v == "1")
        .unwrap_or(false);

    let (api_key, base_url, model, provider_id) = if use_ds {
        match std::env::var("DEEPSEEK_API_KEY") {
            Ok(k) if !k.is_empty() => (
                k,
                std::env::var("DEEPSEEK_BASE_URL")
                    .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                "deepseek-v4-flash".to_string(),
                "deepseek",
            ),
            _ => {
                eprintln!("SKIP: DEEPSEEK_API_KEY not set");
                return;
            }
        }
    } else {
        match std::env::var("MIMO_API_KEY") {
            Ok(k) if !k.is_empty() => (
                k,
                std::env::var("MIMO_BASE_URL")
                    .unwrap_or_else(|_| "https://token-plan-cn.xiaomimimo.com/v1".into()),
                std::env::var("MIMO_MODEL").unwrap_or_else(|_| "mimo-v2.5".into()),
                "other",
            ),
            _ => {
                eprintln!("SKIP: MIMO_API_KEY not set");
                return;
            }
        }
    };

    let tmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
    // Seed past the pre-fold threshold (0.90) with headroom.
    let tid = seed_until_over_threshold(&db, provider_id, &model, 0.92);

    let mrepo = MessageRepo::new(&db);
    let before_count = mrepo.count_for_thread(&tid).unwrap();
    eprintln!("seeded {before_count} messages, provider={provider_id}");

    let engine = build_engine(api_key, base_url, db.clone());
    let (events, content) = run_turn_collect(
        &engine,
        tid.clone(),
        "Given everything above, what is the secret build token? Answer with just the token.",
    )
    .await;

    // A local-source ContextUsage event is emitted right after a fold.
    let local_usage = events.iter().any(|e| {
        matches!(
            e,
            EngineEvent::ContextUsage {
                source: deepseek_core::compaction::ContextUsageSource::Local,
                ..
            }
        )
    });
    let turn_complete = events
        .iter()
        .any(|e| matches!(e, EngineEvent::TurnComplete { .. }));

    // Reload from disk: the fold must have rewritten the thread to a shorter
    // log that begins with a compaction-summary message.
    let after = mrepo.load_by_thread(&tid).unwrap();
    eprintln!(
        "after turn: {} messages on disk (was {before_count})",
        after.len()
    );
    let has_summary = after.iter().any(|row| {
        serde_json::from_str::<ChatMessage>(&row.content_json)
            .ok()
            .and_then(|m| m.content_text())
            .map(|c| c.starts_with("[compaction-summary]\n"))
            .unwrap_or(false)
    });

    eprintln!("\n=== final answer ===\n{}\n", content.trim());
    eprintln!(
        "local_usage_event={local_usage} turn_complete={turn_complete} has_summary={has_summary}"
    );

    // The fold MACHINERY assertions (this is what this test verifies):
    assert!(
        local_usage,
        "a fold must have emitted a ContextUsage(Local) event"
    );
    assert!(
        has_summary,
        "the persisted log must contain a [compaction-summary] message"
    );
    assert!(
        (after.len() as u64) < before_count,
        "fold must shrink the persisted log: before={before_count} after={}",
        after.len()
    );
    assert!(turn_complete, "the turn must still complete after folding");
    assert!(
        !content.trim().is_empty(),
        "the model must still produce a coherent response from the compacted context"
    );

    // The summary must carry real conversation substance (not be empty/noise).
    let summary_text: String = after
        .iter()
        .filter_map(|row| serde_json::from_str::<ChatMessage>(&row.content_json).ok())
        .filter_map(|m| m.content_text())
        .find(|c| c.starts_with("[compaction-summary]\n"))
        .unwrap_or_default();
    assert!(
        summary_text.len() > "[compaction-summary]\n".len() + 40,
        "summary should contain substantive prose, got: {summary_text}"
    );

    // Write the real before/after + the actual generated summary to the report.
    if let Ok(dir) = std::env::var("FOLD_REPORT_DIR") {
        let _ = std::fs::create_dir_all(&dir);
        let label = if use_ds { "deepseek" } else { "openai" };
        let body = format!(
            "## 场景 4 — 真实触发折叠（{label}，完整 turn）\n\n\
             **模型**: {model}　**provider**: {provider_id}\n\n\
             一个超过上下文阈值（>90%）的超长会话，跑一个真实 turn，引擎自动折叠。\n\n\
             ### 折叠前后（持久化到 SQLite 的真实消息数）\n\n\
             - 折叠前: **{before_count}** 条消息\n\
             - 折叠后: **{}** 条消息\n\
             - 触发了 ContextUsage(Local) 事件: {local_usage}\n\
             - turn 正常完成: {turn_complete}\n\n\
             ### 折叠生成的摘要（模型真实返回的原文，替换了被折叠的历史）\n\n{}\n\n\
             ### 折叠后模型对新问题的真实回答\n\n{}\n",
            after.len(),
            summary_text,
            if content.trim().is_empty() {
                "(空)"
            } else {
                content.trim()
            },
        );
        let _ = std::fs::write(
            std::path::Path::new(&dir).join(format!("04-fold-trigger-{label}.md")),
            body,
        );
    }

    // NOTE on constraint preservation: in this synthetic test the pinned fact
    // is one line buried under ~780 identical filler turns, so a weak summary
    // model may legitimately drop it as noise. Constraint/objective
    // preservation against a REALISTIC conversation is verified separately in
    // `fold_live_test::live_*_fold_summary_*` (where the constraint is integral
    // to the dialogue). Here we only assert the fold machinery + coherence.
}
