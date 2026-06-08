//! LIVE: a real LM model uses the `ask_user_question` tool to clarify a vague
//! request; an auto-answering QuestionGate feeds a canned selection back, and
//! the model continues with the answer in mind. Validates the EPIC 1 chain
//! end-to-end with a real model.
//!
//! Provider neutrality: the same path is exercised for DeepSeek and (via
//! `SUBAGENT_TEST_API_KEY`) a second provider like MiMo — the tool emits only
//! standard tool_use / tool_result, no provider-specific fields.
//!
//! Run: $env:DEEPSEEK_API_KEY="<key>"; cargo test -p deepseek-core --test live_ask_user_question_test -- --ignored --nocapture

use std::sync::Arc;

use async_trait::async_trait;
use deepseek_client::deepseek::{DeepSeekClient, DeepSeekClientConfig};
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
use deepseek_state::{Database, ThreadInsert, ThreadRepo};
use deepseek_tools::specs::register_default_tools;
use deepseek_tools::web::WebToolsState;
use deepseek_tools::{
    AnswerItem, QuestionGate, QuestionGateError, QuestionOutcome, QuestionRequest, ToolRegistry,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Permission gate that allows everything (no UI in tests).
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

/// Auto-answering question gate: for every question, pick the FIRST option's
/// label. Records the questions it saw so the test can assert the tool fired.
struct AutoAnswerGate {
    saw: Arc<parking_lot::Mutex<Vec<QuestionRequest>>>,
}

#[async_trait]
impl QuestionGate for AutoAnswerGate {
    async fn ask(
        &self,
        req: QuestionRequest,
        _abort: CancellationToken,
    ) -> Result<QuestionOutcome, QuestionGateError> {
        self.saw.lock().push(req.clone());
        let answers = req
            .questions
            .iter()
            .map(|q| AnswerItem {
                question: q.question.clone(),
                selected: q
                    .options
                    .first()
                    .map(|o| vec![o.label.clone()])
                    .unwrap_or_default(),
                other: None,
            })
            .collect();
        Ok(QuestionOutcome::Answered(answers))
    }
}

fn creds() -> Option<(String, String, String, &'static str)> {
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
async fn real_model_uses_ask_user_question() {
    let (api_key, base_url, model, provider_id) = match creds() {
        Some(v) => v,
        None => {
            eprintln!("SKIP: set DEEPSEEK_API_KEY");
            return;
        }
    };

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
    let saw = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let qgate: Arc<dyn QuestionGate> = Arc::new(AutoAnswerGate { saw: saw.clone() });

    let system_prompt = deepseek_core::prompt::build_system_prompt(None);
    let engine = AgentEngine::new(client, system_prompt, registry, gate, db.clone());
    engine.set_question_gate(qgate);

    let tid = ThreadRepo::new(&db)
        .create(ThreadInsert {
            name: Some("ask-user-question-live".into()),
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
    // Deliberately vague request that strongly invites a clarifying question.
    let prompt = "I want you to use the ask_user_question tool to ask me which programming \
                  language to use for a new small command-line tool. Offer at least two \
                  concrete language options. After I answer, acknowledge my choice in your \
                  final reply.";
    let send_fut = engine.send_message(tid, prompt.to_string(), tx);
    tokio::pin!(send_fut);

    let mut asked = false;
    let mut final_text = String::new();
    let mut done = false;
    loop {
        tokio::select! {
            r = &mut send_fut, if !done => { done = true; let _ = r; }
            ev = rx.recv() => match ev {
                Some(EngineEvent::ToolCallStart { tool_name, .. }) => {
                    eprintln!("[tool_call_start] {tool_name}");
                    if tool_name == "ask_user_question" { asked = true; }
                }
                Some(EngineEvent::ContentDelta { delta, .. }) => final_text.push_str(&delta),
                Some(_) => {}
                None => if done { break; },
            }
        }
        if done && rx.is_empty() {
            break;
        }
    }

    let seen = saw.lock().clone();
    eprintln!("=== questions asked ===");
    for r in &seen {
        for q in &r.questions {
            eprintln!(
                "  [{}] {} -> {:?}",
                q.header,
                q.question,
                q.options.iter().map(|o| &o.label).collect::<Vec<_>>()
            );
        }
    }
    eprintln!("=== final answer ===\n{final_text}");
    eprintln!("asked={asked}");

    assert!(asked, "model should call the ask_user_question tool");
    assert!(
        !seen.is_empty(),
        "the question gate should have received at least one request"
    );
    let first = &seen[0];
    assert!(
        !first.questions.is_empty() && first.questions[0].options.len() >= 2,
        "the question should carry at least two options"
    );
    // The first option's label was auto-selected; the model should acknowledge
    // it in some form. We assert non-empty continuation rather than exact text
    // (model phrasing varies).
    assert!(
        !final_text.trim().is_empty(),
        "model should produce a final reply after receiving the answer"
    );
}
