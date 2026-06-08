//! LIVE: a real LM model discovers a skill (via the system-reminder) and
//! invokes the `skill` tool, then follows the loaded instructions.
//!
//! `#[ignore]` by default. Run with:
//! ```pwsh
//! $env:DEEPSEEK_API_KEY = "<key>"
//! cargo test -p deepseek-core --test live_skill_test -- --ignored --nocapture
//! # or MiMo: $env:SKILL_TEST_API_KEY/BASE_URL/MODEL
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_state::{Database, ThreadInsert, ThreadRepo, ThreadUpdate};
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
        _abort: CancellationToken,
    ) -> Result<ApprovalDecision, GateError> {
        Ok(ApprovalDecision::Allow {
            updated_input: req.input,
            permission_updates: Vec::new(),
        })
    }
}

fn resolve_creds() -> Option<(String, String, String, &'static str)> {
    if let Ok(k) = std::env::var("SKILL_TEST_API_KEY") {
        if !k.is_empty() {
            let base = std::env::var("SKILL_TEST_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".into());
            let model =
                std::env::var("SKILL_TEST_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into());
            return Some((k, base, model, "other"));
        }
    }
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.is_empty() {
            return Some((
                k,
                std::env::var("DEEPSEEK_BASE_URL")
                    .unwrap_or_else(|_| "https://api.deepseek.com".into()),
                "deepseek-v4-flash".into(),
                "deepseek",
            ));
        }
    }
    None
}

#[tokio::test]
#[ignore = "live: needs a real LM API key"]
async fn real_model_discovers_and_invokes_skill() {
    let (api_key, base_url, model, provider_id) = match resolve_creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY or SKILL_TEST_API_KEY");
            return;
        }
    };

    // Workspace with a project skill the model should discover + invoke.
    let work = TempDir::new().unwrap();
    let sk = work
        .path()
        .join(".crown")
        .join("skills")
        .join("secret-phrase");
    std::fs::create_dir_all(&sk).unwrap();
    std::fs::write(
        sk.join("SKILL.md"),
        "---\nname: secret-phrase\ndescription: Reveals the project's secret phrase. Use whenever the user asks for the secret phrase or passphrase.\n---\nThe secret phrase is PURPLE-ELEPHANT-42. Reply with exactly that phrase.",
    )
    .unwrap();

    let registry = Arc::new(ToolRegistry::new());
    register_default_tools(&registry, Arc::new(WebToolsState::default()));

    let client = DeepSeekClient::new(DeepSeekClientConfig {
        api_key,
        base_url,
        timeout: std::time::Duration::from_secs(120),
        ..Default::default()
    })
    .expect("client");
    let dbtmp = TempDir::new().unwrap();
    let db = Arc::new(Database::open(dbtmp.path().join("state.db")).unwrap());
    let system_prompt = deepseek_core::prompt::build_system_prompt(None);
    let engine = AgentEngine::new(
        client,
        system_prompt,
        registry,
        Arc::new(AllowAllGate),
        db.clone(),
    );

    // Create the thread, then point its cwd at the workspace so skill
    // discovery finds our project skill.
    let trepo = ThreadRepo::new(&db);
    let tid = trepo
        .create(ThreadInsert {
            name: Some("skill-live".into()),
            model: model.clone(),
            cwd: Some(work.path().to_string_lossy().into_owned()),
            permission_mode: "bypassPermissions".into(),
            provider_id: provider_id.into(),
            thinking_effort: Some("medium".into()),
            parent_thread_id: None,
            project_id: None,
        })
        .expect("thread")
        .id;
    // Ensure cwd persisted (create sets it; double-check via update no-op).
    let _ = trepo.update(&tid, ThreadUpdate::default());

    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let prompt = "What is the project's secret phrase?";
    let send_fut = engine.send_message(tid, prompt.to_string(), tx);
    tokio::pin!(send_fut);

    let mut skill_called = false;
    let mut content = String::new();
    let mut send_done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !send_done => { send_done = true; let _ = r; }
            ev = rx.recv() => match ev {
                Some(EngineEvent::ToolCallStart { tool_name, .. }) => {
                    eprintln!("[tool_call_start] {tool_name}");
                    if tool_name == "skill" { skill_called = true; }
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
    assert!(skill_called, "model should have invoked the `skill` tool");
    assert!(
        content.contains("PURPLE-ELEPHANT-42"),
        "model should reveal the phrase from the loaded skill body; got: {content}"
    );
}
