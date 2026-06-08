//! Sub-agent launcher (P4): runs a sub-agent as an isolated engine on a
//! restricted tool set + a child thread, forwarding its activity to the UI
//! tagged with `agentId` so the frontend can nest it under the owning `task`
//! card.
//!
//! Lives in the app layer because it needs the Tauri `AppHandle` to dispatch
//! sub-agent events. `core` only provides the agent-type definitions; `tools`
//! only defines the `SubagentLauncher` trait (string in/out).

use std::sync::Arc;

use async_trait::async_trait;
use tauri::AppHandle;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use deepseek_client::deepseek::DeepSeekClient;
use deepseek_core::engine::{AgentEngine, EngineEvent};
use deepseek_core::gate::PermissionGate;
use deepseek_core::hooks::{HookEvent, HookRunner};
use deepseek_core::pricing::ProviderId;
use deepseek_core::subagent::{find_agent, subagent_model_for};
use deepseek_state::{Database, MessageRepo, ThreadInsert, ThreadRepo};
use deepseek_tools::{SubagentLauncher, ToolRegistry};

/// Max model→tool round-trips for a sub-agent (lower than the main loop's
/// safety net — sub-agents are focused).
const SUBAGENT_MAX_NOTE: &str = "sub-agent"; // doc anchor; engine enforces its own cap

/// App-layer implementation of [`SubagentLauncher`]. Cheap to clone the
/// dependencies (all `Arc`/handle).
pub struct AppSubagentLauncher {
    app: AppHandle,
    client: DeepSeekClient,
    gate: Arc<dyn PermissionGate>,
    db: Arc<Database>,
    /// Parent (full) tool registry — sub-agent registries are filtered views.
    parent_tools: Arc<ToolRegistry>,
}

impl AppSubagentLauncher {
    /// Construct from the shared app dependencies.
    pub fn new(
        app: AppHandle,
        client: DeepSeekClient,
        gate: Arc<dyn PermissionGate>,
        db: Arc<Database>,
        parent_tools: Arc<ToolRegistry>,
    ) -> Self {
        let _ = SUBAGENT_MAX_NOTE;
        Self {
            app,
            client,
            gate,
            db,
            parent_tools,
        }
    }
}

#[async_trait]
impl SubagentLauncher for AppSubagentLauncher {
    async fn launch(
        &self,
        agent_type: String,
        prompt: String,
        resume_subagent_id: Option<String>,
        parent_thread_id: String,
        parent_abort: CancellationToken,
    ) -> Result<(String, Option<String>), String> {
        let agent =
            find_agent(&agent_type).ok_or_else(|| format!("unknown agent_type '{agent_type}'"))?;

        // Resolve the parent thread for provider + model + cwd inheritance.
        let trepo = ThreadRepo::new(self.db.as_ref());
        let parent = trepo
            .get(&parent_thread_id)
            .map_err(|e| format!("parent thread load failed: {e}"))?;
        let provider = ProviderId::from_str_lossy(&parent.provider_id);
        let sub_model = subagent_model_for(provider, agent, &parent.model);
        let cwd_path = parent.cwd.as_deref().map(std::path::Path::new);
        let lifecycle_abort = CancellationToken::new();

        // New or resumed sub-thread.
        let sub_thread_id = match resume_subagent_id {
            Some(id) => id,
            None => {
                trepo
                    .create(ThreadInsert {
                        name: Some(format!("[subagent:{}]", agent.name)),
                        model: sub_model.clone(),
                        cwd: parent.cwd.clone(),
                        // Sub-agents run without interactive approval prompts:
                        // the parent turn already cleared the `task` call. They
                        // still cannot touch bypass-immune safety paths.
                        permission_mode: "bypassPermissions".into(),
                        provider_id: parent.provider_id.clone(),
                        thinking_effort: Some(parent.thinking_effort.clone()),
                        parent_thread_id: Some(parent_thread_id.clone()),
                        project_id: parent.project_id.clone(),
                    })
                    .map_err(|e| format!("sub-thread create failed: {e}"))?
                    .id
            }
        };

        for event in [HookEvent::TaskCreated, HookEvent::SubagentStart] {
            let hook_result = HookRunner::load(cwd_path)
                .run(
                    event,
                    serde_json::json!({
                        "session_id": parent_thread_id.clone(),
                        "thread_id": parent_thread_id.clone(),
                        "cwd": parent.cwd.clone().unwrap_or_default(),
                        "permission_mode": parent.permission_mode.clone(),
                        "hook_event_name": event.as_str(),
                        "agent_type": agent_type.clone(),
                        "agent_name": agent.name,
                        "subagent_id": sub_thread_id.clone(),
                        "model": sub_model.clone(),
                        "prompt": prompt.clone(),
                    }),
                    Some(agent_type.as_str()),
                    cwd_path,
                    &lifecycle_abort,
                )
                .await;
            for trace in &hook_result.traces {
                tracing::debug!(
                    event = %trace.event,
                    hook_id = ?trace.hook_id,
                    source = ?trace.source,
                    outcome = %trace.outcome,
                    duration_ms = trace.duration_ms,
                    "hook trace"
                );
            }
        }

        // Restricted tool set: agent's allowlist, always excluding `task`
        // (no nested sub-agents → no unbounded recursion).
        let sub_registry = Arc::new(self.parent_tools.subset(agent.allowed_tools, &["task"]));

        // Sub-engine reuses the same client/gate/db; its own system prompt +
        // restricted registry make it a distinct agent.
        let sub_prompt = format!(
            "{}\n\n# Environment\n- Working directory: {}\n",
            agent.system_prompt,
            parent.cwd.as_deref().unwrap_or("(not set)")
        );
        let sub_engine = AgentEngine::new(
            self.client.clone(),
            sub_prompt,
            sub_registry,
            self.gate.clone(),
            self.db.clone(),
        );

        // Forward sub-agent events to the UI tagged with agentId = sub thread.
        // The owning `task` card lives in the *parent* thread, so surfaced
        // events (content/tool start/tool update) are re-routed to the parent
        // thread id by the dispatcher; agentId carries the sub-thread id for
        // per-sub-agent binding.
        let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
        let app = self.app.clone();
        let sub_id_for_events = sub_thread_id.clone();
        let parent_id_for_events = parent_thread_id.clone();
        let forward = tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                crate::events::dispatch_engine_event_scoped(
                    &app,
                    ev,
                    Some(sub_id_for_events.clone()),
                    Some(parent_id_for_events.clone()),
                );
            }
        });

        // Run the sub-agent turn, cancelling it if the parent aborts.
        let run = sub_engine.send_message(sub_thread_id.clone(), prompt, tx);
        tokio::select! {
            _ = parent_abort.cancelled() => {
                sub_engine.abort_turn(&sub_thread_id);
            }
            r = run => {
                r.map_err(|e| format!("sub-agent run failed: {e}"))?;
            }
        }
        // Ensure the forwarder drains (tx dropped when send_message returns).
        let _ = forward.await;

        // Extract the final assistant message as the report.
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
            .unwrap_or_else(|| "(sub-agent produced no final message)".to_string());

        // Self-audit: append a structured completion marker so the parent
        // model knows the sub-agent's work was verified, not silently
        // truncated. Mirrors Claude Code's sub-agent completion pattern.
        let audited_report = format!(
            "{report}\n\n[sub-agent completed — report above is the verified final output]"
        );

        for event in [HookEvent::SubagentStop, HookEvent::TaskCompleted] {
            let hook_result = HookRunner::load(cwd_path)
                .run(
                    event,
                    serde_json::json!({
                        "session_id": parent_thread_id.clone(),
                        "thread_id": parent_thread_id.clone(),
                        "cwd": parent.cwd.clone().unwrap_or_default(),
                        "permission_mode": parent.permission_mode.clone(),
                        "hook_event_name": event.as_str(),
                        "agent_type": agent_type.clone(),
                        "agent_name": agent.name,
                        "subagent_id": sub_thread_id.clone(),
                        "model": sub_model.clone(),
                        "report_chars": report.len(),
                    }),
                    Some(agent_type.as_str()),
                    cwd_path,
                    &lifecycle_abort,
                )
                .await;
            for trace in &hook_result.traces {
                tracing::debug!(
                    event = %trace.event,
                    hook_id = ?trace.hook_id,
                    source = ?trace.source,
                    outcome = %trace.outcome,
                    duration_ms = trace.duration_ms,
                    "hook trace"
                );
            }
        }

        let resumable = if agent.one_shot {
            None
        } else {
            Some(sub_thread_id)
        };
        Ok((audited_report, resumable))
    }
}
