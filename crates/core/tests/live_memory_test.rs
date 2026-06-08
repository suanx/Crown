//! LIVE: proves project `AGENTS.md` memory is injected into the system prompt
//! and obeyed by a real model (Phase 2). A tempdir cwd holds an AGENTS.md with
//! a verifiable instruction; we assert the model's reply honors it.
//!
//! Run (DeepSeek):
//!   $env:DEEPSEEK_API_KEY="<key>"; cargo test -p deepseek-core --test live_memory_test -- --ignored --nocapture
//! Run (MiMo, provider-neutrality):
//!   $env:MEMORY_TEST_API_KEY="..."; $env:MEMORY_TEST_BASE_URL="https://token-plan-cn.xiaomimimo.com/v1"; $env:MEMORY_TEST_MODEL="mimo-v2.5"; cargo test -p deepseek-core --test live_memory_test -- --ignored --nocapture

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_core::memory::PromptAugment;
use deepseek_state::{Database, ThreadInsert, ThreadRepo};
use deepseek_tools::specs::register_default_tools;
use deepseek_tools::web::WebToolsState;
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

/// (api_key, base_url, model, provider_id). MiMo override wins when set, so a
/// single test verifies provider neutrality.
fn creds() -> Option<(String, String, String, &'static str)> {
    if let Ok(k) = std::env::var("MEMORY_TEST_API_KEY") {
        if !k.is_empty() {
            return Some((
                k,
                std::env::var("MEMORY_TEST_BASE_URL")
                    .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                std::env::var("MEMORY_TEST_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
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
async fn agents_md_is_injected_and_obeyed() {
    let (api_key, base_url, model, provider_id) = match creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY (or MEMORY_TEST_API_KEY)");
            return;
        }
    };

    // Project cwd with an AGENTS.md carrying a fact the model can only know
    // if the file was injected into its system prompt. Asking for that fact
    // is a robust, meaningful obedience signal (mirrors the skill live test).
    let proj = TempDir::new().unwrap();
    std::fs::write(
        proj.path().join("AGENTS.md"),
        "# Project facts\nThis project's internal codename is CROWN-MEMORY-OK. Whenever the user asks for the project codename, answer with exactly that token.",
    )
    .unwrap();

    // Separate data root (global files) — empty here; project memory is what
    // we're testing.
    let data = TempDir::new().unwrap();

    let registry = Arc::new(ToolRegistry::new());
    register_default_tools(&registry, Arc::new(WebToolsState::default()));

    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: Duration::from_secs(180),
        ..Default::default()
    })
    .expect("client");
    let dbtmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(dbtmp.path().join("state.db")).unwrap());
    let gate: Arc<dyn PermissionGate> = Arc::new(AllowAllGate);

    // Base prompt WITHOUT environment (the composer appends it).
    let base = deepseek_core::prompt::build_system_prompt_base(None, None);

    // Diagnostic: confirm the augment actually injects AGENTS.md into the
    // composed prompt (isolates injection bugs from model disobedience).
    let augment_probe = PromptAugment::new(data.path().to_path_buf());
    let composed = augment_probe.compose(&base, "ENVPROBE", Some(proj.path()));
    eprintln!(
        "=== composed prompt contains AGENTS.md token: {} ===",
        composed.contains("CROWN-MEMORY-OK")
    );
    assert!(
        composed.contains("CROWN-MEMORY-OK"),
        "INJECTION BUG: AGENTS.md not in composed prompt:\n{composed}"
    );

    let engine = AgentEngine::new(client, base, registry, gate, db.clone());
    let augment = Arc::new(PromptAugment::new(data.path().to_path_buf()));
    engine.set_prompt_augment(augment);

    let tid = ThreadRepo::new(&db)
        .create(ThreadInsert {
            name: Some("mem-live".into()),
            model,
            cwd: Some(proj.path().to_string_lossy().into_owned()),
            permission_mode: "bypassPermissions".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .unwrap()
        .id;

    // The deterministic proof is the injection assertion above. The obedience
    // check below is a real-model behavior signal — flash occasionally ignores
    // a meta-instruction on a trivial query, so allow one retry before failing.
    let mut final_text = String::new();
    let mut obeyed = false;
    for attempt in 0..2 {
        final_text.clear();
        let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
        let run = engine.send_message(
            tid.clone(),
            "What is this project's internal codename? Answer with just the codename.".into(),
            tx,
        );
        tokio::pin!(run);
        let mut done = false;
        loop {
            tokio::select! {
                r = &mut run, if !done => { done = true; let _ = r; }
                ev = rx.recv() => match ev {
                    Some(EngineEvent::ContentDelta { delta, .. }) => final_text.push_str(&delta),
                    Some(_) => {}
                    None => if done { break; },
                }
            }
            if done && rx.is_empty() {
                break;
            }
        }
        eprintln!("=== reply (attempt {attempt}) ===\n{final_text}");
        if final_text.contains("CROWN-MEMORY-OK") {
            obeyed = true;
            break;
        }
    }

    assert!(
        obeyed,
        "model did not obey AGENTS.md injected memory across 2 attempts; last reply: {final_text}"
    );
}
