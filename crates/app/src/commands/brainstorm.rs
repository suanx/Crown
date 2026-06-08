use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ulid::Ulid;

use deepseek_core::engine::{AgentEngine, EngineEvent, ToolStatusEvent};
use deepseek_core::gate::PermissionGate;
use deepseek_core::pricing::ProviderId;
use deepseek_state::{MessageRepo, ThreadInsert, ThreadRepo, ThreadUpdate};
use deepseek_tools::ToolRegistry;

use crate::dto::{
    BrainstormParticipantDto, ContinueBrainstormInput, MessageUsageDto, StartBrainstormInput,
    StartBrainstormResultDto, ToolCallDto,
};
use crate::events::{ContentDeltaPayload, TurnCompletePayload};
use crate::AppState;

const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "view_file",
    "list_directory",
    "list_dir",
    "grep",
    "grep_search",
    "glob",
    "web_search",
    "web_fetch",
    "skill",
];

const GREENFIELD_TOOLS: &[&str] = &["web_search", "web_fetch", "skill"];

#[derive(Clone)]
struct BrainstormRole {
    id: &'static str,
    name: &'static str,
    role: &'static str,
    color: &'static str,
    prompt: &'static str,
}

fn roles() -> Vec<BrainstormRole> {
    vec![
        BrainstormRole {
            id: "director",
            name: "讨论规划者",
            role: "调度讨论深度",
            color: "#F2B84B",
            prompt: "你是多 Agent 群聊的讨论规划者。你的职责不是解决问题，而是先判断任务类型，再决定哪些人参与、讨论几轮、谁应该回应谁、什么时候收束。",
        },
        BrainstormRole {
            id: "context",
            name: "上下文侦察",
            role: "定位边界和现状",
            color: "#4DB6AC",
            prompt: "你在群聊里的职责是先把问题落到真实上下文：如果是当前项目、已有功能、bug、代码实现或用户说“我们/现在/这里/这个项目”，你必须先调用只读工具检索文件、目录或文本，再点名现有文件、模块、接口和缺口；如果是从零项目，就定义用户、核心场景、约束、非目标和第一版范围。没有工具证据时，不要编造当前项目结构。",
        },
        BrainstormRole {
            id: "product",
            name: "产品定义",
            role: "定义场景范围",
            color: "#FF9F43",
            prompt: "你在群聊里的职责是把模糊想法变成可实现的软件范围：目标用户、关键工作流、MVP、边界、体验风险。不要堆功能，优先抓第一版必须成立的使用闭环。",
        },
        BrainstormRole {
            id: "solution",
            name: "方案专家",
            role: "提出可行方案",
            color: "#5B8CFF",
            prompt: "你在群聊里的职责是提出可实现的方案。不要写报告，不要套模板；像一个工程同事一样直接说你的判断、取舍和建议。",
        },
        BrainstormRole {
            id: "critic",
            name: "反方审查",
            role: "审查风险漏洞",
            color: "#FF6B6B",
            prompt: "你在群聊里的职责是挑出会失败、浪费 token、体验变差或架构失控的点。必须点名回应前面具体观点，并给修正建议。",
        },
        BrainstormRole {
            id: "planner",
            name: "落地规划",
            role: "拆成实施步骤",
            color: "#45C48B",
            prompt: "你在群聊里的职责是把讨论收成可执行步骤，并按 PRD/执行规格口径约束产出：目标、范围、用户场景、验收标准、实施步骤、风险。不要重新争论方向，直接接住前面的共识和分歧。",
        },
        BrainstormRole {
            id: "moderator",
            name: "主持人",
            role: "汇总共识分歧",
            color: "#B58CFF",
            prompt: "你在群聊里的职责是控场和收束。开场时只拆问题；如果你是最后一轮，必须输出完整 PRD/执行规格，不能省 token，不能只给摘要。",
        },
    ]
}

#[cfg(test)]
#[derive(Clone)]
struct BrainstormTurn {
    role_id: String,
    instruction: String,
}

#[derive(Default)]
struct ParticipantOutput {
    content: String,
    tool_calls: usize,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectorPlan {
    summary: String,
    turns: Vec<DirectorTurn>,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectorTurn {
    role_id: String,
    instruction: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectorDecision {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    task_type: String,
    #[serde(default)]
    next_role_id: String,
    #[serde(default)]
    instruction: String,
    #[serde(default)]
    requires_tool_evidence: bool,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    stop_reason: String,
    #[serde(default)]
    quality_checklist: Vec<String>,
}

struct BrainstormSessionState {
    task_type: String,
    transcript: String,
    round: usize,
    valid_context_evidence: bool,
    invalid_context_attempts: usize,
    last_role_id: String,
    quality_checklist: Vec<String>,
}

impl BrainstormSessionState {
    fn new(_topic: &str) -> Self {
        let task_type = "unclassified".to_string();
        Self {
            task_type,
            transcript: String::new(),
            round: 0,
            valid_context_evidence: false,
            invalid_context_attempts: 0,
            last_role_id: String::new(),
            quality_checklist: Vec::new(),
        }
    }

    fn append_director(&mut self, content: &str) {
        if !content.trim().is_empty() {
            self.transcript
                .push_str(&format!("讨论规划者：\n{}", content.trim()));
        }
    }

    fn append_participant(&mut self, role: &BrainstormRole, content: &str) {
        if !content.trim().is_empty() {
            if !self.transcript.trim().is_empty() {
                self.transcript.push_str("\n\n");
            }
            self.transcript
                .push_str(&format!("{}：\n{}", role.name, content.trim()));
            self.last_role_id = role.id.into();
        }
    }

    fn quality_ready(&self) -> bool {
        let text = self.transcript.as_str();
        let prd_terms = ["目标", "范围", "非目标", "方案", "验收", "风险"];
        let has_prd_terms = prd_terms
            .iter()
            .filter(|term| text.contains(**term))
            .count()
            >= 4;
        let context_ok = !is_project_task_type(&self.task_type) || self.valid_context_evidence;
        context_ok && has_prd_terms
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormRunStartedPayload {
    thread_id: String,
    run_id: String,
    topic: String,
    participants: Vec<BrainstormParticipantDto>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormAgentStatusPayload {
    thread_id: String,
    run_id: String,
    participant_id: String,
    status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormMessageStartPayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant: BrainstormParticipantDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormMessageDeltaPayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant_id: String,
    delta: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormReasoningDeltaPayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant_id: String,
    delta: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormToolCallStartPayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant_id: String,
    tool_call: ToolCallDto,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormToolCallUpdatePayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant_id: String,
    tool_use_id: String,
    status: String,
    input: Option<serde_json::Value>,
    result: Option<String>,
    duration_ms: Option<u64>,
    error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormMessageDonePayload {
    thread_id: String,
    run_id: String,
    message_id: String,
    participant_id: String,
    content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormRunDonePayload {
    thread_id: String,
    run_id: String,
    artifact: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainstormErrorPayload {
    thread_id: String,
    run_id: String,
    error: String,
}

#[tauri::command]
pub async fn start_brainstorm(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    input: StartBrainstormInput,
) -> Result<StartBrainstormResultDto, String> {
    let topic = input.topic.trim().to_string();
    if topic.is_empty() {
        return Err("brainstorm topic is required".into());
    }
    let _legacy_requested_rounds = input.rounds.unwrap_or(1).clamp(1, 2);
    let run_id = Ulid::new().to_string();
    let cancel = CancellationToken::new();
    state.brainstorm_runs.insert(run_id.clone(), cancel.clone());

    let ctx = BrainstormContext {
        app: app.clone(),
        db: state.db.clone(),
        gate: state.gate.clone(),
        tools: state.tools.clone(),
        thread_id: input.thread_id,
        run_id: run_id.clone(),
        topic,
        cancel,
        runs: state.brainstorm_runs.clone(),
    };

    tauri::async_runtime::spawn(async move {
        run_brainstorm(ctx).await;
    });

    Ok(StartBrainstormResultDto { run_id })
}

#[tauri::command]
pub async fn continue_brainstorm(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    input: ContinueBrainstormInput,
) -> Result<StartBrainstormResultDto, String> {
    start_brainstorm(
        app,
        state,
        StartBrainstormInput {
            thread_id: input.thread_id,
            topic: format!("继续上一轮头脑风暴 {}：{}", input.run_id, input.prompt),
            rounds: Some(1),
        },
    )
    .await
}

#[tauri::command]
pub async fn stop_brainstorm(
    state: tauri::State<'_, AppState>,
    run_id: String,
) -> Result<(), String> {
    if let Some((_, token)) = state.brainstorm_runs.remove(&run_id) {
        token.cancel();
    }
    Ok(())
}

struct BrainstormContext {
    app: AppHandle,
    db: Arc<deepseek_state::Database>,
    gate: Arc<crate::gate_impl::TauriPermissionGate>,
    tools: Arc<ToolRegistry>,
    thread_id: String,
    run_id: String,
    topic: String,
    cancel: CancellationToken,
    runs: Arc<dashmap::DashMap<String, CancellationToken>>,
}

async fn run_brainstorm(ctx: BrainstormContext) {
    let result = run_brainstorm_inner(&ctx).await;
    ctx.runs.remove(&ctx.run_id);
    if let Err(error) = result {
        let _ = ctx.app.emit(
            "brainstorm:error",
            BrainstormErrorPayload {
                thread_id: ctx.thread_id,
                run_id: ctx.run_id,
                error,
            },
        );
    }
}

async fn run_brainstorm_inner(ctx: &BrainstormContext) -> Result<(), String> {
    let trepo = ThreadRepo::new(ctx.db.as_ref());
    let parent = trepo.get(&ctx.thread_id).map_err(|e| e.to_string())?;
    let provider = ProviderId::from_str_lossy(&parent.provider_id);
    let model = brainstorm_model(provider, &parent.model);
    let client = crate::commands::config::client_for_provider_id(&parent.provider_id)
        .ok_or_else(|| format!("provider '{}' is not configured", parent.provider_id))?;

    let active_roles = roles();
    let participants = active_roles
        .iter()
        .into_iter()
        .map(|r| BrainstormParticipantDto {
            id: r.id.to_string(),
            name: r.name.to_string(),
            role: r.role.to_string(),
            color: r.color.to_string(),
        })
        .collect::<Vec<_>>();
    let _ = ctx.app.emit(
        "brainstorm:run_started",
        BrainstormRunStartedPayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            topic: ctx.topic.clone(),
            participants,
        },
    );

    persist_user_prompt(ctx, &ctx.topic);

    let director = active_roles
        .iter()
        .find(|r| r.id == "director")
        .ok_or_else(|| "missing director role".to_string())?;

    const MIN_ROUNDS: usize = 4;
    const MAX_ROUNDS: usize = 12;
    let mut session = BrainstormSessionState::new(&ctx.topic);
    let opening =
        "先由讨论规划者判断任务类型和工具边界；如涉及当前项目，会用只读工具核验上下文；如是从零产品，不会假装读取仓库。后续按 steering 推进：上下文/需求 → 方案权衡 → 反证 → 计划/PRD；未满足质量门控不会提前收束。";
    emit_static_participant_message(ctx, "director", director, &format!("讨论计划：{}", opening));
    session.append_director(&opening);

    let mut final_summary = String::new();
    while session.round < MAX_ROUNDS {
        if ctx.cancel.is_cancelled() {
            return Ok(());
        }
        session.round += 1;
        let director_output =
            run_director(ctx, &parent, &client, &model, director, &session).await?;
        let mut decision = parse_director_decision(&director_output)
            .map(|d| sanitize_director_decision(d, &session))
            .unwrap_or_else(|| fallback_director_decision(&session));
        session.append_director(&decision.summary);
        if !decision.task_type.trim().is_empty() {
            session.task_type = decision.task_type.clone();
        }
        if !decision.quality_checklist.is_empty() {
            session.quality_checklist = decision.quality_checklist.clone();
        }

        let can_finish = session.round >= MIN_ROUNDS && session.quality_ready();
        if decision.done && can_finish {
            decision.next_role_id = "moderator".into();
            decision.instruction = final_prd_instruction(&session, &decision);
        } else if decision.done {
            decision.done = false;
            let fallback = fallback_director_decision(&session);
            decision.next_role_id = fallback.next_role_id;
            decision.instruction = fallback.instruction;
            decision.requires_tool_evidence = fallback.requires_tool_evidence;
        }
        finish_director_message(
            ctx,
            director,
            session.round,
            &director_visible_summary(&decision, session.round),
        );

        let role = active_roles
            .iter()
            .find(|r| r.id == decision.next_role_id.as_str())
            .or_else(|| active_roles.iter().find(|r| r.id == "planner"))
            .ok_or_else(|| format!("unknown brainstorm role '{}'", decision.next_role_id))?;
        let visible = !decision.done;
        let output = run_participant(
            ctx,
            &parent,
            &client,
            &model,
            role,
            session.round,
            &decision.instruction,
            &session.transcript,
            &session.task_type,
            visible,
        )
        .await?;

        let requires_evidence = decision.requires_tool_evidence
            || requires_context_tools(role, &session.task_type, &decision.instruction);
        if role.id == "context" && requires_evidence {
            if output.tool_calls > 0 {
                session.valid_context_evidence = true;
            } else {
                session.invalid_context_attempts += 1;
            }
        }
        session.append_participant(role, &output.content);

        if decision.done {
            final_summary = output.content.trim().to_string();
            break;
        }
    }

    if final_summary.is_empty() {
        let role = active_roles
            .iter()
            .find(|r| r.id == "moderator")
            .ok_or_else(|| "missing moderator role".to_string())?;
        let decision = DirectorDecision {
            summary: "达到最大轮次，强制收束为完整 PRD。".into(),
            task_type: session.task_type.clone(),
            next_role_id: "moderator".into(),
            instruction: final_prd_instruction(&session, &DirectorDecision::default()),
            requires_tool_evidence: false,
            done: true,
            stop_reason: "max_rounds".into(),
            quality_checklist: session.quality_checklist.clone(),
        };
        session.append_director(&decision.summary);
        let output = run_participant(
            ctx,
            &parent,
            &client,
            &model,
            role,
            session.round + 1,
            &decision.instruction,
            &session.transcript,
            &session.task_type,
            false,
        )
        .await?;
        final_summary = output.content.trim().to_string();
    }
    if final_summary.is_empty() {
        final_summary = session.transcript.trim().to_string();
    }
    persist_normal_assistant_message(ctx, &final_summary);

    let _ = ctx.app.emit(
        "brainstorm:run_done",
        BrainstormRunDonePayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            artifact: final_summary,
        },
    );
    let _ = trepo.update(
        &ctx.thread_id,
        ThreadUpdate {
            preview: Some(Some(format!("/brainstorm {}", ctx.topic))),
            touch: true,
            ..Default::default()
        },
    );
    Ok(())
}

async fn run_director(
    ctx: &BrainstormContext,
    parent: &deepseek_state::Thread,
    client: &deepseek_client::deepseek::DeepSeekClient,
    model: &str,
    role: &BrainstormRole,
    session: &BrainstormSessionState,
) -> Result<String, String> {
    let trepo = ThreadRepo::new(ctx.db.as_ref());
    let sub_thread = trepo
        .create(ThreadInsert {
            name: Some(format!("[brainstorm:{}:director]", ctx.run_id)),
            model: model.to_string(),
            cwd: parent.cwd.clone(),
            permission_mode: "bypassPermissions".into(),
            provider_id: parent.provider_id.clone(),
            thinking_effort: Some(parent.thinking_effort.clone()),
            parent_thread_id: Some(ctx.thread_id.clone()),
            project_id: parent.project_id.clone(),
        })
        .map_err(|e| format!("brainstorm director thread create failed: {e}"))?;

    let readonly = Arc::new(read_only_tool_registry(&ctx.tools));
    let gate: Arc<dyn PermissionGate> = ctx.gate.clone();
    let system_prompt = format!(
        "{}\n\n{}\n\n输出必须是 JSON 对象，不要 Markdown，不要解释。",
        shared_brainstorm_rules(),
        role.prompt
    );
    let engine = AgentEngine::new(
        client.clone(),
        system_prompt,
        readonly,
        gate,
        ctx.db.clone(),
    );

    let prompt = director_decision_prompt(&ctx.topic, parent.cwd.as_deref(), session);
    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let participant = BrainstormParticipantDto {
        id: role.id.to_string(),
        name: role.name.to_string(),
        role: role.role.to_string(),
        color: role.color.to_string(),
    };
    let message_id = director_message_id(ctx, session.round);
    let _ = ctx.app.emit(
        "brainstorm:message_start",
        BrainstormMessageStartPayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            message_id: message_id.clone(),
            participant: participant.clone(),
        },
    );
    let _ = ctx.app.emit(
        "brainstorm:message_delta",
        BrainstormMessageDeltaPayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            message_id: message_id.clone(),
            participant_id: role.id.to_string(),
            delta: format!("第 {} 轮调度中...", session.round),
        },
    );
    let app = ctx.app.clone();
    let thread_id = ctx.thread_id.clone();
    let run_id = ctx.run_id.clone();
    let participant_id = role.id.to_string();
    let msg_id = message_id.clone();
    let forward = tokio::spawn(async move {
        let mut content = String::new();
        while let Some(ev) = rx.recv().await {
            match ev {
                EngineEvent::ContentDelta { delta, .. } => {
                    content.push_str(&delta);
                }
                EngineEvent::ReasoningDelta { delta, .. } => {
                    let _ = app.emit(
                        "brainstorm:reasoning_delta",
                        BrainstormReasoningDeltaPayload {
                            thread_id: thread_id.clone(),
                            run_id: run_id.clone(),
                            message_id: msg_id.clone(),
                            participant_id: participant_id.clone(),
                            delta,
                        },
                    );
                }
                EngineEvent::ToolCallStart {
                    tool_use_id,
                    tool_name,
                    input,
                    ..
                } => {
                    let _ = app.emit(
                        "brainstorm:tool_call_start",
                        BrainstormToolCallStartPayload {
                            thread_id: thread_id.clone(),
                            run_id: run_id.clone(),
                            message_id: msg_id.clone(),
                            participant_id: participant_id.clone(),
                            tool_call: ToolCallDto {
                                id: tool_use_id,
                                name: tool_name,
                                input,
                                status: "pending_approval".into(),
                                result: None,
                                duration_ms: None,
                                diff: None,
                                error_message: None,
                            },
                        },
                    );
                }
                EngineEvent::ToolCallUpdate {
                    tool_use_id,
                    status,
                    input,
                    result,
                    duration_ms,
                    error_message,
                    ..
                } => {
                    let _ = app.emit(
                        "brainstorm:tool_call_update",
                        BrainstormToolCallUpdatePayload {
                            thread_id: thread_id.clone(),
                            run_id: run_id.clone(),
                            message_id: msg_id.clone(),
                            participant_id: participant_id.clone(),
                            tool_use_id,
                            status: tool_status_to_str(status).into(),
                            input,
                            result,
                            duration_ms,
                            error_message,
                        },
                    );
                }
                _ => {}
            }
        }
        content
    });

    tokio::select! {
        _ = ctx.cancel.cancelled() => {
            engine.abort_turn(&sub_thread.id);
        }
        r = engine.send_message(sub_thread.id.clone(), prompt, tx) => {
            r.map_err(|e| format!("brainstorm director failed: {e}"))?;
        }
    }
    Ok(forward.await.unwrap_or_default())
}

async fn run_participant(
    ctx: &BrainstormContext,
    parent: &deepseek_state::Thread,
    client: &deepseek_client::deepseek::DeepSeekClient,
    model: &str,
    role: &BrainstormRole,
    turn_index: usize,
    turn_instruction: &str,
    transcript: &str,
    task_type: &str,
    visible_in_discussion: bool,
) -> Result<ParticipantOutput, String> {
    let participant = BrainstormParticipantDto {
        id: role.id.to_string(),
        name: role.name.to_string(),
        role: role.role.to_string(),
        color: role.color.to_string(),
    };
    let message_id = format!("brainstorm-{}-{}-{}", ctx.run_id, role.id, turn_index);
    if visible_in_discussion {
        let _ = ctx.app.emit(
            "brainstorm:agent_status",
            BrainstormAgentStatusPayload {
                thread_id: ctx.thread_id.clone(),
                run_id: ctx.run_id.clone(),
                participant_id: role.id.to_string(),
                status: "running".into(),
            },
        );
        let _ = ctx.app.emit(
            "brainstorm:message_start",
            BrainstormMessageStartPayload {
                thread_id: ctx.thread_id.clone(),
                run_id: ctx.run_id.clone(),
                message_id: message_id.clone(),
                participant: participant.clone(),
            },
        );
    }

    let trepo = ThreadRepo::new(ctx.db.as_ref());
    let sub_thread = trepo
        .create(ThreadInsert {
            name: Some(format!("[brainstorm:{}:{}]", ctx.run_id, role.id)),
            model: model.to_string(),
            cwd: parent.cwd.clone(),
            permission_mode: "bypassPermissions".into(),
            provider_id: parent.provider_id.clone(),
            thinking_effort: Some(parent.thinking_effort.clone()),
            parent_thread_id: Some(ctx.thread_id.clone()),
            project_id: parent.project_id.clone(),
        })
        .map_err(|e| format!("brainstorm sub-thread create failed: {e}"))?;

    let readonly = Arc::new(brainstorm_tool_registry(task_type, &ctx.tools));
    let gate: Arc<dyn PermissionGate> = ctx.gate.clone();
    let system_prompt = format!(
        "{}\n\n{}\n\n# 当前角色\n{}\n\n# 工作目录\n{}",
        shared_brainstorm_rules(),
        role.prompt,
        role.name,
        parent.cwd.as_deref().unwrap_or("(not set)")
    );
    let engine = AgentEngine::new(
        client.clone(),
        system_prompt,
        readonly,
        gate,
        ctx.db.clone(),
    );

    let prompt = participant_prompt(
        &ctx.topic,
        transcript,
        role,
        turn_instruction,
        task_type,
        visible_in_discussion,
    );
    let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
    let app = ctx.app.clone();
    let thread_id = ctx.thread_id.clone();
    let run_id = ctx.run_id.clone();
    let participant_id = role.id.to_string();
    let msg_id = message_id.clone();
    let forward = tokio::spawn(async move {
        let mut content = String::new();
        let mut tool_calls = 0usize;
        while let Some(ev) = rx.recv().await {
            match ev {
                EngineEvent::ContentDelta { delta, .. } => {
                    content.push_str(&delta);
                    if visible_in_discussion {
                        let _ = app.emit(
                            "brainstorm:message_delta",
                            BrainstormMessageDeltaPayload {
                                thread_id: thread_id.clone(),
                                run_id: run_id.clone(),
                                message_id: msg_id.clone(),
                                participant_id: participant_id.clone(),
                                delta,
                            },
                        );
                    } else {
                        let _ = app.emit(
                            "stream:content_delta",
                            ContentDeltaPayload {
                                thread_id: thread_id.clone(),
                                message_id: msg_id.clone(),
                                delta,
                                agent_id: None,
                            },
                        );
                    }
                }
                EngineEvent::ReasoningDelta { delta, .. } => {
                    if visible_in_discussion {
                        let _ = app.emit(
                            "brainstorm:reasoning_delta",
                            BrainstormReasoningDeltaPayload {
                                thread_id: thread_id.clone(),
                                run_id: run_id.clone(),
                                message_id: msg_id.clone(),
                                participant_id: participant_id.clone(),
                                delta,
                            },
                        );
                    }
                }
                EngineEvent::ToolCallStart {
                    tool_use_id,
                    tool_name,
                    input,
                    ..
                } => {
                    tool_calls += 1;
                    if visible_in_discussion {
                        let _ = app.emit(
                            "brainstorm:tool_call_start",
                            BrainstormToolCallStartPayload {
                                thread_id: thread_id.clone(),
                                run_id: run_id.clone(),
                                message_id: msg_id.clone(),
                                participant_id: participant_id.clone(),
                                tool_call: ToolCallDto {
                                    id: tool_use_id,
                                    name: tool_name,
                                    input,
                                    status: "pending_approval".into(),
                                    result: None,
                                    duration_ms: None,
                                    diff: None,
                                    error_message: None,
                                },
                            },
                        );
                    }
                }
                EngineEvent::ToolCallUpdate {
                    tool_use_id,
                    status,
                    input,
                    result,
                    duration_ms,
                    error_message,
                    ..
                } => {
                    if visible_in_discussion {
                        let _ = app.emit(
                            "brainstorm:tool_call_update",
                            BrainstormToolCallUpdatePayload {
                                thread_id: thread_id.clone(),
                                run_id: run_id.clone(),
                                message_id: msg_id.clone(),
                                participant_id: participant_id.clone(),
                                tool_use_id,
                                status: tool_status_to_str(status).into(),
                                input,
                                result,
                                duration_ms,
                                error_message,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        ParticipantOutput {
            content,
            tool_calls,
        }
    });

    tokio::select! {
        _ = ctx.cancel.cancelled() => {
            engine.abort_turn(&sub_thread.id);
        }
        r = engine.send_message(sub_thread.id.clone(), prompt, tx) => {
            r.map_err(|e| format!("brainstorm participant failed: {e}"))?;
        }
    }
    let run_output = forward.await.unwrap_or_default();
    if !visible_in_discussion {
        let _ = ctx.app.emit(
            "stream:turn_complete",
            TurnCompletePayload {
                thread_id: ctx.thread_id.clone(),
                message_id: message_id.clone(),
                usage: MessageUsageDto {
                    cache_read_tokens: 0,
                    cache_miss_tokens: 0,
                    cache_creation_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                },
                agent_id: None,
            },
        );
    }
    let mut output = clean_agent_text(&run_output.content);
    if requires_context_tools(role, task_type, turn_instruction) && run_output.tool_calls == 0 {
        let warning = "侦察无效：本轮需要读取当前项目上下文，但没有实际调用只读工具。后续结论不能把它当作代码级依据。";
        if visible_in_discussion {
            let delta = if output.is_empty() {
                warning.to_string()
            } else {
                format!("\n\n{}", warning)
            };
            let _ = ctx.app.emit(
                "brainstorm:message_delta",
                BrainstormMessageDeltaPayload {
                    thread_id: ctx.thread_id.clone(),
                    run_id: ctx.run_id.clone(),
                    message_id: message_id.clone(),
                    participant_id: role.id.to_string(),
                    delta,
                },
            );
        }
        if output.is_empty() {
            output = warning.into();
        } else {
            output.push_str("\n\n");
            output.push_str(warning);
        }
    }

    if visible_in_discussion {
        let _ = ctx.app.emit(
            "brainstorm:message_done",
            BrainstormMessageDonePayload {
                thread_id: ctx.thread_id.clone(),
                run_id: ctx.run_id.clone(),
                message_id: message_id.clone(),
                participant_id: role.id.to_string(),
                content: output.clone(),
            },
        );
        let _ = ctx.app.emit(
            "brainstorm:agent_status",
            BrainstormAgentStatusPayload {
                thread_id: ctx.thread_id.clone(),
                run_id: ctx.run_id.clone(),
                participant_id: role.id.to_string(),
                status: "done".into(),
            },
        );
        persist_agent_message(ctx, &message_id, &participant, &output);
    }
    Ok(ParticipantOutput {
        content: output,
        tool_calls: run_output.tool_calls,
    })
}

fn requires_context_tools(role: &BrainstormRole, task_type: &str, instruction: &str) -> bool {
    if role.id != "context" {
        return false;
    }
    if is_project_task_type(task_type) {
        return true;
    }
    let text = instruction.to_lowercase();
    let project_markers = [
        "当前项目",
        "这个项目",
        "本项目",
        "我们",
        "现在",
        "这里",
        "已有",
        "代码",
        "bug",
        "报错",
        "修复",
        "实现",
        "功能",
        "前端",
        "后端",
        "文件",
        "模块",
        "工具",
    ];
    project_markers.iter().any(|marker| text.contains(marker))
}

fn read_only_tool_registry(tools: &ToolRegistry) -> ToolRegistry {
    tools.subset(READ_ONLY_TOOLS, &["task"])
}

fn brainstorm_tool_registry(task_type: &str, tools: &ToolRegistry) -> ToolRegistry {
    if task_type == "greenfield_product" {
        tools.subset(GREENFIELD_TOOLS, &["task"])
    } else {
        read_only_tool_registry(tools)
    }
}

fn emit_static_participant_message(
    ctx: &BrainstormContext,
    role_id: &str,
    role: &BrainstormRole,
    content: &str,
) {
    let participant = BrainstormParticipantDto {
        id: role.id.to_string(),
        name: role.name.to_string(),
        role: role.role.to_string(),
        color: role.color.to_string(),
    };
    let message_id = format!("brainstorm-{}-{}", ctx.run_id, role_id);
    let _ = ctx.app.emit(
        "brainstorm:message_start",
        BrainstormMessageStartPayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            message_id: message_id.clone(),
            participant: participant.clone(),
        },
    );
    let _ = ctx.app.emit(
        "brainstorm:message_done",
        BrainstormMessageDonePayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            message_id: message_id.clone(),
            participant_id: role.id.to_string(),
            content: content.to_string(),
        },
    );
    persist_agent_message(ctx, &message_id, &participant, content);
}

fn director_message_id(ctx: &BrainstormContext, round: usize) -> String {
    format!("brainstorm-{}-director-round-{}", ctx.run_id, round)
}

fn finish_director_message(
    ctx: &BrainstormContext,
    role: &BrainstormRole,
    round: usize,
    content: &str,
) {
    let participant = BrainstormParticipantDto {
        id: role.id.to_string(),
        name: role.name.to_string(),
        role: role.role.to_string(),
        color: role.color.to_string(),
    };
    let message_id = director_message_id(ctx, round);
    let _ = ctx.app.emit(
        "brainstorm:message_done",
        BrainstormMessageDonePayload {
            thread_id: ctx.thread_id.clone(),
            run_id: ctx.run_id.clone(),
            message_id: message_id.clone(),
            participant_id: role.id.to_string(),
            content: content.to_string(),
        },
    );
    persist_agent_message(ctx, &message_id, &participant, content);
}

fn brainstorm_model(provider: ProviderId, parent_model: &str) -> String {
    match provider {
        ProviderId::Deepseek => "deepseek-v4-flash".into(),
        _ => parent_model.to_string(),
    }
}

fn tool_status_to_str(status: ToolStatusEvent) -> &'static str {
    match status {
        ToolStatusEvent::PendingApproval => "pending_approval",
        ToolStatusEvent::Running => "running",
        ToolStatusEvent::Success => "success",
        ToolStatusEvent::Error => "error",
        ToolStatusEvent::Aborted => "aborted",
    }
}

fn shared_brainstorm_rules() -> &'static str {
    "你是 Crown 多 Agent 头脑风暴中的一个内部 Agent。\n\
禁止寒暄、感谢、赞美、道歉、自我介绍、总结性空话。\n\
不要说“我同意前面专家”“这是个很好的问题”等社交语。\n\
如果引用其他 Agent，只回应其观点中的事实、风险或方案。\n\
每句话必须提供新信息；不能提供新信息就不要写。\n\
默认只读，不修改文件，不执行写入。\n\
如果主题涉及当前项目、已有代码、bug、功能改造或用户说“我们/现在/这里/这个项目”，必须基于只读工具得到的上下文发言，不能脑补技术栈、文件名或架构。\n\
如果主题是从零做一个软件，不要调用本地仓库工具，不要假装已有代码；可以使用联网或 skill 工具补充市场、技术或 PRD 方法论证据；先基于合理假设收敛用户、场景、MVP、非目标、技术约束和第一版验证。\n\
信息不足时不要把问题抛回给用户，不要写“请逐条回答”“我才能继续”；必须用 HYP-1/HYP-2 形式声明假设，并继续推进讨论。\n\
除非工具或上下文证明项目使用某技术，否则不要引入 Python/asyncio/Pydantic/AutoGen/Dify 之类外部框架名来替代当前项目方案。\n\
最终收束必须像 PRD/执行规格，不是聊天摘要：背景、目标、用户场景、范围、非目标、方案、验收标准、风险、实施步骤。\n\
这不是报告生成任务，而是多人群聊。除最后一轮主持人外，你只能发一条短消息，建议 80-260 个中文字符；最后一轮主持人不受这个字数限制，必须完整。"
}

#[cfg(test)]
fn director_prompt(topic: &str, cwd: Option<&str>, max_turns: usize) -> String {
    format!(
        "# 用户主题\n{}\n\n# 当前工作目录\n{}\n\n# 先分类再调度\n你必须先判断主题类型，并把判断写进 summary：\n- existing_project：当前项目实现、已有功能、bug、UI/后端改造、用户说“我们/现在/这里/这个项目”。必须安排 context 先用只读工具侦察现有代码。\n- greenfield_product：从零做一个软件或产品。必须安排 context 或 product 先定义用户、场景、MVP、非目标，不能假装已有代码。\n- research_decision：纯调研或方案比较。必须安排 critic 做反证，planner 收敛决策条件。\n- debug_review：现象、报错、体验不对。必须安排 context 先定位证据，critic 再审查根因。\n\n# 可调度角色\n- context：上下文侦察，负责工具取证或从零边界定义\n- product：产品定义，负责用户场景、MVP、体验边界\n- solution：方案专家，负责提出和修正方案\n- critic：反方审查，负责风险和压力测试\n- planner：落地规划，负责执行顺序\n- moderator：主持人，负责开场或最终收束\n\n# 你的任务\n根据主题复杂度决定几条发言、谁回应谁、是否需要加深。普通问题 4-5 条，复杂工程问题 6-{} 条。必须让后面的发言回应前面的具体观点，不能固定套流程。\n\n# 当前项目类任务的硬约束\n如果分类是 existing_project 或 debug_review：\n1. 第一条可见发言必须是 context，并要求它调用只读工具定位相关文件、模块、状态和缺口。\n2. 最后一条必须是 moderator，要求输出文件/模块级执行计划、验证方式和风险。\n3. 不允许安排任何角色输出泛化框架方案替代当前代码库方案。\n\n# 从零项目类任务的硬约束\n如果分类是 greenfield_product：\n1. 前两条必须覆盖 context/product，先定用户、场景、MVP、非目标。\n2. planner 最终要给第一版交付路线，不要列百科式功能清单。\n\n# 只输出 JSON\n{{\"summary\":\"任务分类 + 一句话说明讨论深度\",\"turns\":[{{\"roleId\":\"context\",\"instruction\":\"这一条发言要做什么，必须指明回应对象、目的，以及是否要调用工具\"}}]}}\n\n约束：turns 长度 3-{}；roleId 只能是 context/product/solution/critic/planner/moderator；最后一条通常由 moderator 收束。",
        topic,
        cwd.unwrap_or("(not set)"),
        max_turns,
        max_turns
    )
}

fn director_decision_prompt(
    topic: &str,
    cwd: Option<&str>,
    session: &BrainstormSessionState,
) -> String {
    format!(
        "# 用户主题\n{}\n\n# 当前工作目录\n{}\n\n# 当前会话状态\n- taskType: {}\n- round: {} / 12\n- validContextEvidence: {}\n- invalidContextAttempts: {}\n- lastRoleId: {}\n- qualityChecklist: {}\n\n# 群聊历史\n{}\n\n# steering 方法论硬约束\n1. Brainstorming：先探索上下文，再细化需求，再提出 2-3 种方案权衡，最后沉淀 PRD。\n2. Writing Plans：最终计划必须具体、可执行、有验收标准；禁止占位、禁止泛泛而谈。\n3. Parallel Agents：只有独立问题域才建议并行；每个 Agent 的任务必须聚焦、自包含、有明确输出。\n4. Provider neutrality：不要写死某个模型供应商专属行为。\n\n# 任务类型判定\n你不是读取本地规则结果，而是自己判断 taskType。\n- 如果 taskType 是 unclassified，第一轮必须先判定类型和工具边界。\n- 你可以调用只读工具核验当前仓库是否相关；只有用户主题涉及当前项目、这个项目、已有代码、bug、实现、UI、后端、前端等现有工程时，才要求后续 context 读取本地仓库。\n- 如果是从零做软件或产品，判为 greenfield_product：不要要求本地仓库侦察；可以让 context/product 使用 web_search、web_fetch 或 skill 做市场、技术、PRD 方法论补证。\n- 如果不确定是否关联当前项目，先安排 context 做一次只读侦察，而不是凭空假设。\n\n# 你的任务\n你是动态调度器。每一轮只决定下一位 Agent，不能一次性排完整流程。根据历史判断：是否需要继续侦察、澄清、方案、反证、落地，或是否已经可以输出完整 PRD。\n\n# 收束门控\n- 最少 4 轮后才能 done=true。\n- 当前项目/bug/实现类任务必须已有有效工具侦察证据，否则不能 done=true。\n- 最终 PRD 必须覆盖：背景、目标、用户场景、范围、非目标、方案权衡、模块/文件或 MVP 模块、验收标准、风险、实施步骤。\n\n# 只输出 JSON\n{{\"summary\":\"为什么下一轮这样调度，必须包含 taskType 判断依据和是否需要工具证据\",\"taskType\":\"existing_project|greenfield_product|research_decision|debug_review\",\"nextRoleId\":\"context|product|solution|critic|planner|moderator\",\"instruction\":\"给下一位 Agent 的具体任务。若要求工具取证，必须写明要查什么。\",\"requiresToolEvidence\":false,\"done\":false,\"stopReason\":\"\",\"qualityChecklist\":[\"已探索上下文\",\"已细化需求\",\"已有方案权衡\",\"已有反证审查\",\"可输出 PRD\"]}}",
        topic,
        cwd.unwrap_or("(not set)"),
        session.task_type,
        session.round,
        session.valid_context_evidence,
        session.invalid_context_attempts,
        if session.last_role_id.is_empty() {
            "(none)"
        } else {
            session.last_role_id.as_str()
        },
        if session.quality_checklist.is_empty() {
            "[]".into()
        } else {
            session.quality_checklist.join(" / ")
        },
        if session.transcript.trim().is_empty() {
            "暂无。".into()
        } else {
            session.transcript.clone()
        }
    )
}

fn parse_director_decision(raw: &str) -> Option<DirectorDecision> {
    let trimmed = raw.trim();
    serde_json::from_str::<DirectorDecision>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            serde_json::from_str::<DirectorDecision>(&trimmed[start..=end]).ok()
        })
}

fn sanitize_director_decision(
    mut decision: DirectorDecision,
    session: &BrainstormSessionState,
) -> DirectorDecision {
    let allowed = [
        "context",
        "product",
        "moderator",
        "solution",
        "critic",
        "planner",
    ];
    if !matches!(
        decision.task_type.as_str(),
        "existing_project" | "greenfield_product" | "research_decision" | "debug_review"
    ) {
        decision.task_type = if matches!(
            session.task_type.as_str(),
            "existing_project" | "greenfield_product" | "research_decision" | "debug_review"
        ) {
            session.task_type.clone()
        } else {
            "research_decision".into()
        };
    }
    if !allowed.contains(&decision.next_role_id.as_str()) {
        let fallback = fallback_director_decision(session);
        decision.next_role_id = fallback.next_role_id;
        if decision.instruction.trim().is_empty() {
            decision.instruction = fallback.instruction;
        }
        decision.requires_tool_evidence = fallback.requires_tool_evidence;
    }
    if decision.instruction.trim().is_empty() {
        decision.instruction = fallback_director_decision(session).instruction;
    }
    if decision.task_type == "greenfield_product" {
        decision.requires_tool_evidence = false;
        let asks_code_probe = ["当前", "仓库", "代码", "文件", "模块"]
            .iter()
            .any(|marker| decision.instruction.contains(marker));
        if decision.next_role_id == "context" && asks_code_probe {
            decision.instruction =
                "这是从零产品类任务。先定义目标用户、核心场景、MVP、非目标、技术约束和第一轮验收标准，不要假装读取当前仓库。"
                    .into();
        }
    }
    if session.round < 4 {
        decision.done = false;
    }
    if is_project_task_type(&decision.task_type) && !session.valid_context_evidence {
        decision.done = false;
        decision.next_role_id = "context".into();
        decision.instruction = "当前项目类任务还没有有效工具侦察。调用只读工具定位相关文件、模块、接口和缺口，输出证据；如果工具不可用，明确标记缺少证据，不能给代码级结论。".into();
        decision.requires_tool_evidence = true;
    }
    decision
}

fn fallback_director_decision(session: &BrainstormSessionState) -> DirectorDecision {
    let project_task = is_project_task_type(&session.task_type);
    let (role, instruction, requires_tool) = if project_task && !session.valid_context_evidence {
        (
            "context",
            "调用只读工具侦察当前项目相关文件、模块、接口和缺口。必须点名读到的文件或搜索结果。",
            true,
        )
    } else {
        match session.round {
            0 | 1 => (
                "context",
                "判断任务类型并建立上下文。从零项目定义用户、场景、MVP、非目标；当前项目则用只读工具取证。",
                project_task,
            ),
            2 => (
                "product",
                "基于上下文细化用户目标、核心工作流、范围、非目标和第一版成功标准。",
                false,
            ),
            3 => (
                "solution",
                "提出 2-3 种方案并说明权衡，推荐一个最小可行方案。",
                false,
            ),
            4 => (
                "critic",
                "反证方案，指出会导致无用、返工、体验失败或供应商耦合的风险。",
                false,
            ),
            5 => (
                "planner",
                "把方案收敛成可执行步骤。当前项目给模块/文件级方向；从零项目给 MVP 模块和验收。",
                false,
            ),
            6 | 7 => (
                "solution",
                "吸收反证和落地规划，修正方案，删除泛化内容。",
                false,
            ),
            _ => (
                "moderator",
                "输出完整 PRD/执行规格，不能省略关键章节。",
                false,
            ),
        }
    };
    DirectorDecision {
        summary: "本地保底调度，继续推进到 PRD 质量门控。".into(),
        task_type: session.task_type.clone(),
        next_role_id: role.into(),
        instruction: instruction.into(),
        requires_tool_evidence: requires_tool,
        done: role == "moderator" && session.round >= 4 && session.quality_ready(),
        stop_reason: String::new(),
        quality_checklist: vec![
            "已探索上下文".into(),
            "已细化需求".into(),
            "已有方案权衡".into(),
            "已有反证审查".into(),
            "可输出 PRD".into(),
        ],
    }
}

fn final_prd_instruction(session: &BrainstormSessionState, decision: &DirectorDecision) -> String {
    format!(
        "你是最终收束 Agent。不要省 token，不要只写摘要。基于全部群聊历史输出完整 PRD/执行规格。\n\
必须包含：\n\
1. 背景和问题定义\n\
2. 目标和成功标准\n\
3. 用户场景或核心工作流\n\
4. 范围和非目标\n\
5. 2-3 种方案设计与权衡，以及推荐方案\n\
6. {} \n\
7. 验收标准和测试方式\n\
8. 风险、取舍和后续迭代\n\
9. 可执行实施步骤\n\
质量门控：{}。\n\
stopReason: {}",
        if is_project_task_type(&session.task_type) {
            "当前项目文件/模块级改动清单；如果侦察证据不足，必须明确缺失证据和下一步侦察清单"
        } else {
            "从零项目的 MVP 模块、数据流、关键界面和第一轮交付"
        },
        if decision.quality_checklist.is_empty() {
            session.quality_checklist.join(" / ")
        } else {
            decision.quality_checklist.join(" / ")
        },
        if decision.stop_reason.trim().is_empty() {
            "quality_gate_ready"
        } else {
            decision.stop_reason.as_str()
        }
    )
}

fn director_visible_summary(decision: &DirectorDecision, round: usize) -> String {
    let summary = if decision.summary.trim().is_empty() {
        "继续按质量门控推进讨论。"
    } else {
        decision.summary.trim()
    };
    let evidence = if decision.requires_tool_evidence {
        "需要工具证据"
    } else {
        "不强制工具证据"
    };
    if decision.done {
        format!(
            "第 {} 轮调度：{}。质量门控已满足，接下来切回主对话输出完整 PRD。",
            round, summary
        )
    } else {
        format!(
            "第 {} 轮调度：{}。下一位：{}；{}。\n任务：{}",
            round,
            summary,
            decision.next_role_id,
            evidence,
            decision.instruction.trim()
        )
    }
}

fn is_project_task_type(task_type: &str) -> bool {
    matches!(task_type, "existing_project" | "debug_review")
}

fn participant_prompt(
    topic: &str,
    transcript: &str,
    role: &BrainstormRole,
    turn_instruction: &str,
    task_type: &str,
    visible_in_discussion: bool,
) -> String {
    let prior = if transcript.trim().is_empty() {
        "暂无，当前是第一位发言。"
    } else {
        transcript
    };
    let greenfield_rule = if task_type == "greenfield_product" {
        "\n# 从零项目补充约束\n这是从零产品类任务。不要调用本地仓库工具或假装读取现有代码；可以使用联网或 skill 工具补充市场、技术或 PRD 方法论证据。不要要求用户先回答问题，不要写“请逐条回答”或“我才能继续”。信息不足时用 HYP-1/HYP-2/HYP-3 明确假设，然后继续产出可验证的 MVP、非目标和验收标准。"
    } else {
        ""
    };
    let output_rule = if visible_in_discussion {
        "发言要求：像群聊一样直接回应，不要写成报告；如果前面已有内容，要点名回应具体人或具体观点；只发一条消息；不要向用户索要回答来中断讨论，缺信息就标注假设并继续。"
    } else {
        "最终输出要求：你是最后收束者，不要省 token，不要只写摘要。必须输出完整 PRD/执行规格，建议包含：\n1. 背景和问题定义\n2. 目标和成功标准\n3. 用户场景或核心工作流\n4. 范围和非目标\n5. 方案设计\n6. 当前项目文件/模块级改动，或从零项目的 MVP 模块\n7. 验收标准和测试方式\n8. 风险、取舍和后续迭代\n当前项目类任务必须引用前面侦察到的具体文件/模块；如果侦察无效，要明确缺少证据并给下一步侦察清单。"
    };
    format!(
        "# 用户主题\n{}\n\n# director 判定的 taskType\n{}\n\n# 群聊历史\n{}\n\n# 这一轮轮到你\n你是{}。{}\n\n# 工具和真实性要求\n- 如果你是上下文侦察，并且 taskType 是 existing_project 或 debug_review，先调用只读工具获取证据；最终发言必须点名读到的文件、目录、接口或状态。\n- 如果没有工具证据，不要编造当前项目结构；可以明确说“还缺少侦察证据”。\n- 如果 taskType 是 greenfield_product，不要假装已有代码，直接定义用户、场景、MVP、非目标和第一轮验证；可以使用 web 或 skill 工具补证。\n{}\n\n{}",
        topic, task_type, prior, role.name, turn_instruction, greenfield_rule, output_rule
    )
}

#[cfg(test)]
fn parse_director_plan(raw: &str) -> Option<DirectorPlan> {
    let trimmed = raw.trim();
    serde_json::from_str::<DirectorPlan>(trimmed)
        .ok()
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            serde_json::from_str::<DirectorPlan>(&trimmed[start..=end]).ok()
        })
}

#[cfg(test)]
fn sanitize_director_plan(plan: DirectorPlan, rounds: u8) -> Vec<BrainstormTurn> {
    let max_turns = if rounds > 1 { 10 } else { 8 };
    let allowed = [
        "context",
        "product",
        "moderator",
        "solution",
        "critic",
        "planner",
    ];
    let mut turns = plan
        .turns
        .into_iter()
        .filter(|turn| allowed.contains(&turn.role_id.as_str()))
        .filter(|turn| !turn.instruction.trim().is_empty())
        .take(max_turns)
        .map(|turn| BrainstormTurn {
            role_id: turn.role_id,
            instruction: turn.instruction,
        })
        .collect::<Vec<_>>();

    if turns.len() < 3 {
        return Vec::new();
    }
    if !turns.iter().any(|turn| turn.role_id == "context") {
        turns.insert(
            0,
            BrainstormTurn {
                role_id: "context".into(),
                instruction: "规划者未安排上下文侦察，你先补位：判断任务类型。当前项目类任务必须调用只读工具定位相关文件和缺口；从零项目则定义用户、场景、MVP、非目标。".into(),
            },
        );
        if turns.len() > max_turns {
            turns.truncate(max_turns);
        }
    }
    if turns
        .last()
        .map(|turn| turn.role_id.as_str() != "moderator")
        .unwrap_or(true)
    {
        turns.push(BrainstormTurn {
            role_id: "moderator".into(),
            instruction: "请基于前面群聊内容收束：共识、分歧、推荐结论、下一步。".into(),
        });
    }
    turns
}

fn persist_user_prompt(ctx: &BrainstormContext, topic: &str) {
    let content = json!({
        "role": "user",
        "content": format!("/brainstorm {}", topic),
    });
    let _ =
        MessageRepo::new(ctx.db.as_ref()).append_next(&ctx.thread_id, "user", &content.to_string());
}

fn persist_agent_message(
    ctx: &BrainstormContext,
    message_id: &str,
    participant: &BrainstormParticipantDto,
    content: &str,
) {
    let content_json = json!({
        "role": "assistant",
        "content": content,
        "brainstorm": {
            "runId": ctx.run_id,
            "messageId": message_id,
            "participant": participant,
        }
    });
    let _ = MessageRepo::new(ctx.db.as_ref()).append_next(
        &ctx.thread_id,
        "assistant",
        &content_json.to_string(),
    );
}

fn persist_normal_assistant_message(ctx: &BrainstormContext, content: &str) {
    let content_json = json!({
        "role": "assistant",
        "content": content,
    });
    let _ = MessageRepo::new(ctx.db.as_ref()).append_next(
        &ctx.thread_id,
        "assistant",
        &content_json.to_string(),
    );
}

fn clean_agent_text(raw: &str) -> String {
    let banned = [
        "谢谢",
        "感谢",
        "我同意",
        "很好的观点",
        "这是个很好的问题",
        "希望这有帮助",
        "如有需要我可以继续",
    ];
    raw.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !banned.iter().any(|word| trimmed.contains(word))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn participant_prompt_carries_prior_transcript() {
        let role = roles()
            .into_iter()
            .find(|r| r.id == "critic")
            .expect("critic role exists");
        let prompt = participant_prompt(
            "做多 Agent",
            "方案专家：\n先做主持人制。",
            &role,
            "直接回应方案专家。",
            "existing_project",
            true,
        );
        assert!(prompt.contains("方案专家"));
        assert!(prompt.contains("先做主持人制"));
        assert!(prompt.contains("点名回应具体人或具体观点"));
        assert!(prompt.contains("taskType 是 greenfield_product"));
        assert!(prompt.contains("不要编造当前项目结构"));
    }

    #[test]
    fn final_participant_prompt_requires_full_prd() {
        let role = roles()
            .into_iter()
            .find(|r| r.id == "moderator")
            .expect("moderator role exists");
        let prompt = participant_prompt(
            "从零做一个学习软件",
            "产品定义：\n先做 MVP。",
            &role,
            "输出最终计划。",
            "greenfield_product",
            false,
        );
        assert!(prompt.contains("不要省 token"));
        assert!(prompt.contains("完整 PRD/执行规格"));
        assert!(prompt.contains("验收标准和测试方式"));
    }

    #[test]
    fn director_prompt_classifies_project_or_greenfield_tasks() {
        let prompt = director_prompt(
            "怎么做一个多 Agent 功能",
            Some("C:\\workspace\\crown"),
            8,
        );
        assert!(prompt.contains("existing_project"));
        assert!(prompt.contains("greenfield_product"));
        assert!(prompt.contains("第一条可见发言必须是 context"));
        assert!(prompt.contains("从零项目类任务"));
        assert!(prompt.contains("只读工具"));
    }

    #[test]
    fn director_decision_prompt_requires_dynamic_gate() {
        let mut session = BrainstormSessionState::new("怎么做一个多agent功能");
        session.round = 2;
        let prompt = director_decision_prompt(
            "怎么做一个多agent功能",
            Some("C:\\workspace\\crown"),
            &session,
        );
        assert!(prompt.contains("每一轮只决定下一位 Agent"));
        assert!(prompt.contains("requiresToolEvidence"));
        assert!(prompt.contains("qualityChecklist"));
        assert!(prompt.contains("最少 4 轮后才能 done=true"));
        assert!(prompt.contains("Writing Plans"));
    }

    #[test]
    fn project_context_without_tool_evidence_cannot_finish() {
        let session = BrainstormSessionState::new("怎么做一个多agent功能");
        let decision = sanitize_director_decision(
            DirectorDecision {
                task_type: "existing_project".into(),
                next_role_id: "moderator".into(),
                instruction: "输出最终 PRD".into(),
                done: true,
                ..Default::default()
            },
            &session,
        );
        assert!(!decision.done);
        assert_eq!(decision.next_role_id, "context");
        assert!(decision.requires_tool_evidence);
    }

    #[test]
    fn project_without_evidence_forces_context_before_solution() {
        let session = BrainstormSessionState::new("怎么做一个多agent功能");
        let decision = sanitize_director_decision(
            DirectorDecision {
                task_type: "existing_project".into(),
                next_role_id: "solution".into(),
                instruction: "直接设计方案".into(),
                done: false,
                ..Default::default()
            },
            &session,
        );
        assert_eq!(decision.next_role_id, "context");
        assert!(decision.instruction.contains("有效工具侦察"));
        assert!(decision.requires_tool_evidence);
    }

    #[test]
    fn greenfield_product_does_not_require_code_probe() {
        let session = BrainstormSessionState::new("我想做一个背单词软件");
        let role = roles()
            .into_iter()
            .find(|r| r.id == "context")
            .expect("context role exists");
        assert_eq!(session.task_type, "unclassified");
        assert!(!requires_context_tools(
            &role,
            "greenfield_product",
            "定义用户、场景、MVP 和非目标。"
        ));
        assert!(GREENFIELD_TOOLS.contains(&"web_search"));
        assert!(GREENFIELD_TOOLS.contains(&"skill"));
        assert!(!GREENFIELD_TOOLS.contains(&"list_directory"));
        assert!(!GREENFIELD_TOOLS.contains(&"read_file"));
    }

    #[test]
    fn greenfield_prompt_uses_assumptions_instead_of_user_questionnaire() {
        let role = roles()
            .into_iter()
            .find(|r| r.id == "context")
            .expect("context role exists");
        let prompt = participant_prompt(
            "我想做一个背单词软件",
            "",
            &role,
            "先定义需求边界。",
            "greenfield_product",
            true,
        );
        assert!(prompt.contains("联网或 skill 工具"));
        assert!(prompt.contains("不要要求用户先回答问题"));
        assert!(prompt.contains("HYP-1"));
        assert!(prompt.contains("不要向用户索要回答"));
    }

    #[test]
    fn director_visible_summary_is_not_blank() {
        let message = director_visible_summary(
            &DirectorDecision {
                summary: "需要先补产品定义。".into(),
                next_role_id: "product".into(),
                instruction: "基于假设收敛 MVP。".into(),
                requires_tool_evidence: false,
                done: false,
                ..Default::default()
            },
            2,
        );
        assert!(message.contains("第 2 轮调度"));
        assert!(message.contains("下一位：product"));
        assert!(message.contains("基于假设收敛 MVP"));
    }

    #[test]
    fn director_can_reclassify_greenfield_as_project_if_it_has_evidence() {
        let session = BrainstormSessionState::new("我想做一个背单词软件");
        let decision = sanitize_director_decision(
            DirectorDecision {
                task_type: "existing_project".into(),
                next_role_id: "context".into(),
                instruction: "读取当前仓库代码".into(),
                requires_tool_evidence: true,
                ..Default::default()
            },
            &session,
        );
        assert_eq!(decision.task_type, "existing_project");
        assert!(decision.requires_tool_evidence);
    }

    #[test]
    fn director_can_reclassify_project_topic_as_greenfield() {
        let session = BrainstormSessionState::new("这个项目怎么做多 agent 功能");
        let decision = sanitize_director_decision(
            DirectorDecision {
                task_type: "greenfield_product".into(),
                next_role_id: "product".into(),
                instruction: "只定义用户场景".into(),
                ..Default::default()
            },
            &session,
        );
        assert_eq!(decision.task_type, "greenfield_product");
        assert_eq!(decision.next_role_id, "product");
        assert!(!decision.requires_tool_evidence);
    }

    #[test]
    fn non_deepseek_brainstorm_keeps_parent_model() {
        assert_eq!(brainstorm_model(ProviderId::Other, "gpt-5.1"), "gpt-5.1");
    }

    #[test]
    fn clean_agent_text_removes_social_fillers() {
        let cleaned = clean_agent_text(
            "谢谢你的观点\n- 必须先限制工具权限\n这是个很好的问题\n- 再做主持人汇总",
        );
        assert!(!cleaned.contains("谢谢"));
        assert!(!cleaned.contains("很好的问题"));
        assert!(cleaned.contains("必须先限制工具权限"));
        assert!(cleaned.contains("再做主持人汇总"));
    }

    #[test]
    fn deepseek_brainstorm_uses_flash_model() {
        assert_eq!(
            brainstorm_model(ProviderId::Deepseek, "deepseek-v4-pro"),
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn parse_director_plan_extracts_json_block() {
        let raw = r#"```json
{"summary":"需要加深一轮","turns":[
  {"roleId":"solution","instruction":"先给方案"},
  {"roleId":"critic","instruction":"回应方案风险"},
  {"roleId":"planner","instruction":"拆执行步骤"}
]}
```"#;
        let plan = parse_director_plan(raw).expect("plan parsed");
        assert_eq!(plan.summary, "需要加深一轮");
        assert_eq!(plan.turns.len(), 3);
        assert_eq!(plan.turns[1].role_id, "critic");
    }

    #[test]
    fn sanitize_director_plan_filters_invalid_roles_and_adds_moderator_end() {
        let plan = DirectorPlan {
            summary: "x".into(),
            turns: vec![
                DirectorTurn {
                    role_id: "solution".into(),
                    instruction: "先说方案".into(),
                },
                DirectorTurn {
                    role_id: "random".into(),
                    instruction: "非法角色".into(),
                },
                DirectorTurn {
                    role_id: "critic".into(),
                    instruction: "压测风险".into(),
                },
                DirectorTurn {
                    role_id: "planner".into(),
                    instruction: "拆步骤".into(),
                },
            ],
        };
        let turns = sanitize_director_plan(plan, 1);
        assert_eq!(turns.len(), 5);
        assert_eq!(turns.first().unwrap().role_id, "context");
        assert!(turns.first().unwrap().instruction.contains("上下文侦察"));
        assert!(turns.iter().all(|turn| turn.role_id != "random"));
        assert_eq!(turns.last().unwrap().role_id, "moderator");
    }
}
