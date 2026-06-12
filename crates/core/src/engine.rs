//! Agent execution engine — multi-thread aware, gate-driven, abort-aware.
//!
//! The engine is owned by the Tauri app crate and shared via [`Arc`]. It
//! holds shared resources (LLM client, tool registry, permission gate,
//! database) and a [`ThreadCache`] of in-memory thread state. Each
//! [`AgentEngine::send_message`] call resolves the thread by id (cache hit
//! or DB load), runs a multi-turn loop, persists every message to SQLite as
//! it lands, and surfaces [`EngineEvent`]s on an mpsc channel.
//!
//! ## Permission flow per tool call
//!
//! 1. [`check_tool_permission`] runs the 9-step decision flow (rules + tool
//!    callback + mode).
//! 2. If the result is `Allow` → execute directly.
//! 3. If `Deny` → append a tool error message and skip execution.
//! 4. If `Ask` (or `Passthrough` that the flow already converted to ask) →
//!    call [`PermissionGate::ask`]. The user's [`ApprovalDecision::Allow`]
//!    may carry [`PermissionUpdate`]s ("Allow always") which are applied
//!    in-place; `Deny` produces a tool error result fed back to the model.
//!
//! ## Abort
//!
//! Each turn replaces `state.abort_token` with a fresh
//! [`CancellationToken`]. [`abort_turn`] cancels it; the streaming `select!`
//! loops drop out, `gate.ask` returns `Aborted`, and the iteration emits
//! [`EngineEvent::Aborted`] before bailing. In-flight tool calls receive the
//! token via `ToolContext.abort`: long-running tools (shell, web_fetch)
//! select on it and terminate promptly (shell kills the process tree). On
//! abort, any unfinished tool_calls get a synthetic aborted result so the
//! assistant→tool message sequence stays valid (P1, 2026-05-30).

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use ulid::Ulid;

use deepseek_client::deepseek::{ChatOpts, DeepSeekClient};
use deepseek_client::types::{
    ChatMessage, ExtraBody, FunctionCall, MessageContent, ThinkingConfig, ToolCall as ApiToolCall, Usage,
};
use deepseek_state::{
    Database, MessageRepo, ThreadInsert, ThreadRepo, ThreadUpdate, UsageInsert, UsageRepo,
};
use deepseek_tools::permission::PermissionResult;
use deepseek_tools::specs::{build_tool_specs, build_tool_specs_from_registry};
use deepseek_tools::types::ToolCall as InnerToolCall;
use deepseek_tools::ToolRegistry;

use crate::compaction::{self, PostUsageDecision};
use crate::gate::{ApprovalDecision, ApprovalRequest, PermissionGate};
use crate::hooks::{self, HookEvent, HookPermissionBehavior, HookRunner};
use crate::permission::{check_tool_permission, PermissionMode, ToolPermissionContext};
use crate::pricing::{self, ProviderId, UsageBreakdown};
use crate::thread::{ThreadCache, ThreadId, ThreadState};

/// Maximum number of model→tool→model round-trips per turn.
///
/// Claude Code sets this to UNLIMITED for the main session (only subagents
/// have caps). We use 200 as a safety net against truly pathological loops
/// while allowing complex tasks (full-stack projects, large refactors) to
/// complete naturally. The model should stop itself via natural completion
/// (no more tool calls) long before hitting 200.
const MAX_TURN_ITERATIONS: usize = 200;

/// How many times to transparently retry establishing/streaming a model
/// response **when nothing has been produced yet** this iteration.
///
/// DeepSeek (and other providers) occasionally reset the SSE connection
/// before the first byte arrives — most visibly on the first message of a
/// session (cold TLS connection). reqwest surfaces this as
/// `error decoding response body`. Because no content/reasoning/tool delta
/// has been emitted yet, re-issuing the identical request is safe (no
/// duplicate or out-of-order output). Once any delta has streamed, we do
/// NOT retry (that would duplicate partial output) — the error is surfaced
/// instead. Provider-agnostic: any transport blip benefits.
const MAX_STREAM_RETRIES: usize = 2;

/// Default model when a thread is created without an explicit one.
const DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// Tool execution status surfaced via [`EngineEvent::ToolCallUpdate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatusEvent {
    /// Tool call parsed but permission decision not yet made — card shows
    /// "awaiting". Used by the post-stream input-backfill update so the card
    /// keeps its pending state instead of jumping to "running" before the
    /// permission gate has decided.
    PendingApproval,
    /// Tool started executing.
    Running,
    /// Tool finished without error.
    Success,
    /// Tool finished with `is_error = true` or was rejected by permission.
    Error,
    /// Tool was aborted before it could complete.
    Aborted,
}

/// Events emitted to consumers (Tauri frontend, CLI, tests) during a turn.
///
/// All variants carry `thread_id` so multi-thread consumers can route by
/// thread without inspecting another channel. `message_id` is the ULID of
/// the assistant message currently being produced (or, for tool events,
/// the assistant message whose `tool_calls` triggered them).
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A streamed chunk of assistant content (model output).
    ContentDelta {
        thread_id: ThreadId,
        message_id: String,
        delta: String,
    },
    /// A streamed chunk of reasoning content (DeepSeek-specific).
    ReasoningDelta {
        thread_id: ThreadId,
        message_id: String,
        delta: String,
    },
    /// A tool call has been parsed from the stream and is about to execute.
    ToolCallStart {
        thread_id: ThreadId,
        message_id: String,
        tool_use_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    /// A tool call's status changed (running → success / error / aborted).
    ToolCallUpdate {
        thread_id: ThreadId,
        message_id: String,
        tool_use_id: String,
        status: ToolStatusEvent,
        /// 工具入参。流式参数增长时尽力发送 partial input；流结束后发送完整
        /// 解析结果；纯状态更新为 `None`。
        input: Option<serde_json::Value>,
        result: Option<String>,
        duration_ms: Option<u64>,
        error_message: Option<String>,
    },
    /// The session todo list changed (via the todo_write tool).
    TodosUpdated {
        thread_id: ThreadId,
        todos: Vec<deepseek_tools::todo::TodoItem>,
    },
    /// Context usage update — for the frontend progress ring.
    ContextUsage {
        thread_id: ThreadId,
        used_tokens: u64,
        max_tokens: u64,
        ratio: f64,
        source: crate::compaction::ContextUsageSource,
    },
    /// The model produced no further tool calls; the turn is complete.
    TurnComplete {
        thread_id: ThreadId,
        message_id: String,
        usage: Usage,
        /// USD cost of the turn computed via [`crate::pricing::compute_cost`]
        /// using the thread's active provider + model. The same value is
        /// persisted to the `usage` SQLite row immediately before this
        /// event fires (best-effort; insert failure logs but doesn't
        /// abort the turn).
        cost_usd: f64,
    },
    /// The user aborted the current turn before it could complete.
    Aborted {
        thread_id: ThreadId,
        message_id: String,
    },
    /// A non-recoverable error occurred (network, parse, etc.).
    Error {
        thread_id: ThreadId,
        message_id: Option<String>,
        error: String,
        retryable: bool,
    },
}

/// Multi-thread agent engine. Cheaply cloneable — internal state is shared
/// behind [`Arc`].
pub struct AgentEngine {
    client: DeepSeekClient,
    client_resolver: parking_lot::RwLock<Option<Arc<dyn ProviderClientResolver>>>,
    tools: Arc<ToolRegistry>,
    gate: Arc<dyn PermissionGate>,
    db: Arc<Database>,
    cache: Arc<ThreadCache>,
    system_prompt_template: String,
    /// Records pre-change file content for rewind (P2). Built from `db`.
    file_history_sink: Arc<dyn deepseek_tools::FileHistorySink>,
    /// Sub-agent launcher (P4). Injected post-construction by the app layer
    /// (it needs the Tauri event sink). `None` until set.
    subagent: parking_lot::RwLock<Option<Arc<dyn deepseek_tools::SubagentLauncher>>>,
    /// 结构化问答 gate（`ask_user_question` 工具委托）。post-construction
    /// 注入（app 层需要 AppHandle）。`None` 时该工具不可用。
    question_gate: parking_lot::RwLock<Option<Arc<dyn deepseek_tools::QuestionGate>>>,
    /// Per-thread prompt augmentation (Phase 2): AGENTS.md memory + rules +
    /// output-style. Injected post-construction by the app layer (it needs
    /// the CrownPaths data root). `None` → `system_prompt_template` is used
    /// verbatim (unchanged behavior for tests / non-Tauri).
    prompt_augment: parking_lot::RwLock<Option<Arc<crate::memory::PromptAugment>>>,
    /// 用户自定义上下文长度覆盖表。key = 模型ID, value = 自定义窗口大小。
    /// 通过 Tauri 命令层在配置加载/保存时更新。
    context_window_overrides: Arc<parking_lot::RwLock<HashMap<String, usize>>>,
}

/// A batch of tool calls to execute together. Read-only batches run
/// concurrently; mutating batches contain exactly one call run serially.
struct ToolBatch {
    concurrent: bool,
    calls: Vec<ApiToolCall>,
}

/// 按供应商 ID 解析当前可用客户端。App 层负责从用户配置读取密钥和端点。
pub trait ProviderClientResolver: Send + Sync {
    fn client_for(&self, provider_id: &str) -> Option<DeepSeekClient>;
}

fn turn_chat_opts(
    tools: Vec<deepseek_client::types::ToolSpec>,
    provider: ProviderId,
    _provider_id: &str,
    thinking_effort: &str,
) -> ChatOpts {
    let extra_body = match provider {
        ProviderId::Deepseek => Some(ExtraBody {
            thinking: Some(ThinkingConfig {
                thinking_type: "enabled".to_string(),
            }),
        }),
        // Anthropic, OpenAI and others: no DeepSeek-specific extra_body.
        ProviderId::Anthropic | ProviderId::Openai | ProviderId::Other => None,
    };
    let thinking = None;

    let reasoning_effort = match (provider, thinking_effort) {
        (ProviderId::Deepseek, "ultra") => Some("max".to_string()),
        (ProviderId::Deepseek, _) => Some("high".to_string()),
        // Non-DeepSeek: pass through regardless of effort level so
        // OpenAI-compatible models (o1/o3, etc.) receive reasoning_effort
        // even at the default "medium" setting.
        (ProviderId::Anthropic | ProviderId::Openai | ProviderId::Other, effort) if !effort.is_empty() => Some(effort.to_string()),
        _ => None,
    };
    ChatOpts {
        tools,
        extra_body,
        thinking,
        reasoning_effort,
    }
}

impl AgentEngine {
    /// Construct the engine with all shared dependencies. The cache is
    /// created internally with [`ThreadCache::with_default_capacity`] (3).
    pub fn new(
        client: DeepSeekClient,
        system_prompt_template: String,
        tools: Arc<ToolRegistry>,
        gate: Arc<dyn PermissionGate>,
        db: Arc<Database>,
    ) -> Self {
        let file_history_sink: Arc<dyn deepseek_tools::FileHistorySink> =
            Arc::new(crate::rewind::DbFileHistorySink::new(db.clone()));
        Self {
            client,
            client_resolver: parking_lot::RwLock::new(None),
            tools,
            gate,
            db,
            cache: Arc::new(ThreadCache::with_default_capacity()),
            system_prompt_template,
            file_history_sink,
            subagent: parking_lot::RwLock::new(None),
            question_gate: parking_lot::RwLock::new(None),
            prompt_augment: parking_lot::RwLock::new(None),
            context_window_overrides: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Inject the sub-agent launcher (P4). Called by the app layer after the
    /// engine is constructed (the launcher needs the Tauri event sink).
    pub fn set_subagent_launcher(&self, launcher: Arc<dyn deepseek_tools::SubagentLauncher>) {
        *self.subagent.write() = Some(launcher);
    }

    /// 注入结构化问答 gate（EPIC 1）。app 层构造后调用（需 AppHandle）。
    pub fn set_question_gate(&self, gate: Arc<dyn deepseek_tools::QuestionGate>) {
        *self.question_gate.write() = Some(gate);
    }

    /// Inject the per-thread prompt augmentation (Phase 2). Called by the app
    /// layer after construction (it needs the CrownPaths data root).
    pub fn set_prompt_augment(&self, augment: Arc<crate::memory::PromptAugment>) {
        *self.prompt_augment.write() = Some(augment);
    }

    /// 注入运行时供应商客户端解析器。
    pub fn set_provider_client_resolver

    /// 更新用户自定义上下文长度覆盖表。key = 模型 ID，value = 上下文窗口(token)。
    pub fn set_context_window_overrides(&self, overrides: HashMap<String, usize>) {
        *self.context_window_overrides.write() = overrides;
    }

    /// 获取模型的有效上下文长度：优先用用户自定义值，否则从定价表读取。
    pub fn effective_context_window(&self, provider: ProviderId, model: &str) -> usize {
        let overrides = self.context_window_overrides.read();
        let custom = overrides.get(model).copied();
        drop(overrides);
        pricing::context_window(provider, model, custom)
    }(&self, resolver: Arc<dyn ProviderClientResolver>) {
        *self.client_resolver.write() = Some(resolver);
    }

    /// Compose the system prompt for a thread with the given `cwd`. With a
    /// prompt augment set, layers in global/project memory + rules +
    /// output-style and a fresh per-thread environment block. Without one,
    /// returns the static template verbatim (unchanged behavior).
    fn compose_thread_prompt(&self, cwd: Option<&std::path::Path>) -> String {
        match self.prompt_augment.read().as_ref() {
            Some(a) => {
                let env = crate::prompt::environment_block_pub(cwd);
                a.compose(&self.system_prompt_template, &env, cwd)
            }
            None => self.system_prompt_template.clone(),
        }
    }

    /// Cache reference for direct access from commands (e.g. switch model
    /// updates the in-memory state in addition to the DB).
    pub fn cache(&self) -> &ThreadCache {
        &self.cache
    }

    /// HTTP client reference for commands that talk directly to the
    /// provider (e.g. `get_user_balance` hits `/user/balance` and bypasses
    /// the engine loop entirely).
    pub fn client(&self) -> &DeepSeekClient {
        &self.client
    }

    /// Database reference (commands that touch threads/messages directly).
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get a thread from cache, or rebuild it from the database on miss.
    ///
    /// Rebuilding loads the messages table, replays each row into the new
    /// [`AppendOnlyLog`], and sets the cached `permission_mode` from the
    /// `threads` row.
    pub fn get_or_load(&self, thread_id: &ThreadId) -> Result<Arc<ThreadState>> {
        if let Some(s) = self.cache.get(thread_id) {
            return Ok(s);
        }

        let trepo = ThreadRepo::new(self.db.as_ref());
        let mrepo = MessageRepo::new(self.db.as_ref());
        let thread = trepo.get(thread_id)?;
        let messages = mrepo.load_by_thread(thread_id)?;

        let state = Arc::new(ThreadState::new(
            thread.id.clone(),
            thread.model.clone(),
            thread.thinking_effort.clone(),
            ProviderId::from_str_lossy(&thread.provider_id),
            thread.provider_id.clone(),
            thread.cwd.as_deref().map(PathBuf::from),
            ToolPermissionContext::new(PermissionMode::from_str_lossy(&thread.permission_mode)),
            self.compose_thread_prompt(thread.cwd.as_deref().map(std::path::Path::new)),
        ));

        // Replay persisted messages into the in-memory log.
        {
            let mut log = state.log.write();
            for row in messages {
                match serde_json::from_str::<ChatMessage>(&row.content_json) {
                    Ok(msg) => log.append(msg),
                    Err(e) => {
                        warn!(
                            thread_id = %thread.id,
                            seq = row.seq,
                            error = %e,
                            "skipping unparseable persisted message",
                        );
                    }
                }
            }
        }

        self.cache.put(Arc::clone(&state));
        Ok(state)
    }

    /// Create a new thread and cache it. Returns the freshly built state.
    pub fn create_thread(
        &self,
        name: Option<String>,
        cwd: Option<PathBuf>,
    ) -> Result<Arc<ThreadState>> {
        let trepo = ThreadRepo::new(self.db.as_ref());
        let row = trepo.create(ThreadInsert {
            name,
            model: DEFAULT_MODEL.to_string(),
            cwd: cwd.as_ref().map(|p| p.to_string_lossy().into_owned()),
            permission_mode: PermissionMode::Default.as_str().to_string(),
            provider_id: "deepseek".to_string(),
            thinking_effort: Some("high".to_string()),
            parent_thread_id: None,
            project_id: None,
        })?;
        let provider = ProviderId::from_str_lossy(&row.provider_id);
        let state = Arc::new(ThreadState::new(
            row.id,
            row.model,
            row.thinking_effort,
            provider,
            row.provider_id.clone(),
            cwd.clone(),
            ToolPermissionContext::default(),
            self.compose_thread_prompt(cwd.as_deref()),
        ));
        self.cache.put(Arc::clone(&state));
        Ok(state)
    }

    /// Cancel the current turn for `thread_id`. Idempotent: cancelling a
    /// thread with no in-flight turn or an unloaded thread is a no-op.
    pub fn abort_turn(&self, thread_id: &ThreadId) {
        if let Some(state) = self.cache.get(thread_id) {
            if let Some(token) = state.abort_token.load_full() {
                token.cancel();
            }
        }
    }

    /// Run a turn for the given thread, streaming events on `event_tx`.
    ///
    /// Errors returned from this function are unrecoverable for the turn
    /// (network failure, max iterations, etc.). Recoverable conditions
    /// (tool errors, permission denials, user abort) are surfaced as
    /// events and the function still returns `Ok(())`.
    pub async fn send_message(
        &self,
        thread_id: ThreadId,
        user_input: String,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<()> {
        self.send_message_inner(thread_id, user_input, vec![], event_tx).await
    }

    /// Like [`send_message`] but with image data URIs for multimodal models.
    /// Each URI must be a `data:image/...;base64,...` string.
    pub async fn send_message_with_images(
        &self,
        thread_id: ThreadId,
        user_input: String,
        images: Vec<String>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<()> {
        self.send_message_inner(thread_id, user_input, images, event_tx).await
    }

    async fn send_message_inner(
        &self,
        thread_id: ThreadId,
        user_input: String,
        images: Vec<String>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<()> {
        let state = self.get_or_load(&thread_id)?;

        // Replace the per-turn abort token with a fresh one. Any prior
        // turn's token is dropped here, which is the cancellation signal
        // for that turn's `select!` blocks (if it was still running, which
        // it shouldn't be — but defense in depth).
        let abort = CancellationToken::new();
        state.abort_token.store(Some(Arc::new(abort.clone())));

        let permission_mode = state.permission_ctx.read().mode.as_str().to_string();
        let turn_hook_runner = HookRunner::load(state.cwd.as_deref());
        let prompt_hook = turn_hook_runner
            .run(
                HookEvent::UserPromptSubmit,
                hooks::user_prompt_input(
                    &state.id,
                    &state.id,
                    state.cwd.as_deref(),
                    &permission_mode,
                    &user_input,
                ),
                None,
                state.cwd.as_deref(),
                &abort,
            )
            .await;
        if let Some(reason) = prompt_hook.blocking_error {
            let _ = event_tx.send(EngineEvent::Error {
                thread_id: state.id.clone(),
                message_id: None,
                error: format!("UserPromptSubmit hook blocked prompt: {reason}"),
                retryable: false,
            });
            return Ok(());
        }

        // 1. Append user message to the in-memory log AND persist it.
        let user_msg = if images.is_empty() {
            ChatMessage::user(&user_input)
        } else {
            let image_parts: Vec<deepseek_client::types::ContentPart> = images
                .into_iter()
                .map(|uri| deepseek_client::types::ContentPart::ImageUrl {
                    image_url: deepseek_client::types::ImageUrl { url: uri },
                })
                .collect();
            ChatMessage::user_with_images(user_input.clone(), image_parts)
        };
        self.persist_message(&state, &user_msg).await?;
        state.log.write().append(user_msg);
        if !prompt_hook.additional_contexts.is_empty() {
            let hook_msg = ChatMessage::system(format!(
                "<system-reminder>\n{}\n</system-reminder>",
                prompt_hook.additional_contexts.join("\n\n")
            ));
            self.persist_message(&state, &hook_msg).await?;
            state.log.write().append(hook_msg);
        }
        state.touch();

        // The user message's seq anchors this turn's file-history records
        // (rewind, P2): a rewind to this seq restores every file the turn
        // (and later turns) changed. Best-effort — falls back to 0.
        let turn_user_seq = {
            let mrepo = MessageRepo::new(self.db.as_ref());
            mrepo.max_seq(&thread_id).unwrap_or(0)
        };

        // Snapshot model + mode for the duration of the turn. They can
        // change between iterations only via explicit IPC commands; in
        // practice we re-read the mode every iteration via permission_ctx.
        let model = state.model.read().clone();
        let thinking_effort = state.thinking_effort.read().clone();
        let provider = *state.provider.read();
        let provider_id = state.provider_id.read().clone();
        let turn_client = self
            .client_resolver
            .read()
            .as_ref()
            .and_then(|r| r.client_for(&provider_id))
            .unwrap_or_else(|| self.client.clone());
        info!(
            thread_id = %state.id,
            provider_id = %provider_id,
            model = %model,
            thinking_effort = %thinking_effort,
            "agent turn runtime selected"
        );

        // Pre-fold: before the first API round-trip, locally estimate the
        // request size. A terminal prior turn, a fresh session restore, or a
        // huge user paste can push us over the line in a way the
        // post-response fold (which needs a usage reply first) can't catch.
        // Folding here keeps the very first call of the turn from 400ing on
        // an over-budget context.
        {
            let messages = self.build_messages(&state);
            let est = compaction::estimate_turn_start(&messages, &model, provider);
            if est.ratio > compaction::TURN_START_FOLD_THRESHOLD {
                let tail_budget =
                    (est.ctx_max as f64 * compaction::FOLD_AGGRESSIVE_TAIL_FRACTION) as usize;
                if self
                    .fold(&state, tail_budget, &abort)
                    .await
                    .unwrap_or(false)
                {
                    self.emit_local_context_usage(&state, &event_tx);
                }
            }
        }

        // Tracks whether we've already folded this turn so the post-response
        // decision doesn't fold twice (which would thrash the prefix cache).
        let mut already_folded_this_turn = false;
        let tool_failure_ledger = Arc::new(Mutex::new(ToolFailureLedger::default()));
        let tool_dispatch_trace = Arc::new(Mutex::new(ToolDispatchTrace::default()));
        let mut stop_hook_blocked_once = false;

        for iter in 0..MAX_TURN_ITERATIONS {
            if abort.is_cancelled() {
                let _ = event_tx.send(EngineEvent::Aborted {
                    thread_id: state.id.clone(),
                    message_id: "(turn-aborted-before-iteration)".to_string(),
                });
                return Ok(());
            }
            debug!(thread_id = %state.id, iter, "agent loop iteration starting");

            let messages = self.build_messages(&state);

            // Stream with transparent retry for pre-output transport blips.
            // `message_id` + accumulators are (re)initialized per attempt so a
            // retry starts clean. We only retry when NOTHING was produced yet.
            let message_id = Ulid::new().to_string();
            let mut content_acc = String::new();
            let mut reasoning_acc = String::new();
            let mut final_usage = Usage::default();
            let mut tool_assembler = ToolCallAssembler::default();
            let mut early_emitted_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut stream_retries_left = MAX_STREAM_RETRIES;

            'stream_attempt: loop {
                let tools = build_tool_specs_from_registry(&self.tools);
                let opts = turn_chat_opts(tools, provider, &provider_id, &thinking_effort);
                let mut stream = match turn_client
                    .stream_with_opts(messages.clone(), &model, opts)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        // Connection establishment failed. Nothing produced —
                        // safe to retry within budget.
                        if stream_retries_left > 0 && !abort.is_cancelled() {
                            stream_retries_left -= 1;
                            warn!(
                                thread_id = %state.id,
                                error = %e,
                                retries_left = stream_retries_left,
                                "stream connect failed before any output — retrying"
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                            continue 'stream_attempt;
                        }
                        let _ = event_tx.send(EngineEvent::Error {
                            thread_id: state.id.clone(),
                            message_id: None,
                            error: e.to_string(),
                            retryable: false,
                        });
                        return Err(e);
                    }
                };

                // Streaming loop. Aborting drops the stream, which closes the
                // underlying reqwest connection.
                let mut transport_error: Option<anyhow::Error> = None;
                loop {
                    let chunk_opt = tokio::select! {
                        biased;
                        _ = abort.cancelled() => None,
                        c = stream.next() => c,
                    };
                    let chunk_result = match chunk_opt {
                        Some(c) => c,
                        None => break,
                    };
                    let chunk = match chunk_result {
                        Ok(c) => c,
                        Err(e) => {
                            transport_error = Some(e);
                            break;
                        }
                    };

                    if let Some(delta) = &chunk.content_delta {
                        content_acc.push_str(delta);
                        let _ = event_tx.send(EngineEvent::ContentDelta {
                            thread_id: state.id.clone(),
                            message_id: message_id.clone(),
                            delta: delta.clone(),
                        });
                    }
                    if let Some(delta) = &chunk.reasoning_delta {
                        reasoning_acc.push_str(delta);
                        let _ = event_tx.send(EngineEvent::ReasoningDelta {
                            thread_id: state.id.clone(),
                            message_id: message_id.clone(),
                            delta: delta.clone(),
                        });
                    }
                    if let Some(tcd) = &chunk.tool_call_delta {
                        let is_new = tool_assembler.is_new_call(tcd);
                        tool_assembler.feed(tcd);
                        let partial = tool_assembler.partial(tcd.index);
                        // Emit ToolCallStart as soon as we know a new tool
                        // call's name — gives the frontend immediate card
                        // rendering during streaming, not a batch dump after.
                        if let Some((id, name, arguments)) = partial {
                            if is_new {
                                if !id.is_empty() && !name.is_empty() {
                                    early_emitted_ids.insert(id.clone());
                                    let _ = event_tx.send(EngineEvent::ToolCallStart {
                                        thread_id: state.id.clone(),
                                        message_id: message_id.clone(),
                                        tool_use_id: id.clone(),
                                        tool_name: name.clone(),
                                        input: partial_tool_input(name, arguments),
                                    });
                                }
                            } else if early_emitted_ids.contains(id)
                                && tcd.arguments_delta.is_some()
                            {
                                let input = partial_tool_input(name, arguments);
                                if input.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
                                    let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                                        thread_id: state.id.clone(),
                                        message_id: message_id.clone(),
                                        tool_use_id: id.clone(),
                                        status: ToolStatusEvent::PendingApproval,
                                        input: Some(input),
                                        result: None,
                                        duration_ms: None,
                                        error_message: None,
                                    });
                                }
                            }
                        }
                    }
                    if let Some(usage) = chunk.usage {
                        final_usage = usage;
                    }
                }

                // Handle a mid-stream transport error. If NOTHING was produced
                // yet this attempt, the connection blipped before any output —
                // retrying is safe (no duplicate/partial). Otherwise we must
                // not retry (would duplicate the partial answer): surface it.
                if let Some(e) = transport_error {
                    let produced_nothing = content_acc.is_empty()
                        && reasoning_acc.is_empty()
                        && early_emitted_ids.is_empty();
                    if produced_nothing && stream_retries_left > 0 && !abort.is_cancelled() {
                        stream_retries_left -= 1;
                        warn!(
                            thread_id = %state.id,
                            error = %e,
                            retries_left = stream_retries_left,
                            "stream transport error before any output — retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        continue 'stream_attempt;
                    }
                    error!(thread_id = %state.id, error = %e, "stream error");
                    let _ = event_tx.send(EngineEvent::Error {
                        thread_id: state.id.clone(),
                        message_id: Some(message_id.clone()),
                        error: e.to_string(),
                        retryable: true,
                    });
                    return Err(e);
                }

                // Stream completed cleanly (or aborted) — leave the retry loop.
                break 'stream_attempt;
            }

            if abort.is_cancelled() {
                let _ = event_tx.send(EngineEvent::Aborted {
                    thread_id: state.id.clone(),
                    message_id,
                });
                return Ok(());
            }

            let assembled = tool_assembler.finish();

            // Capture emptiness BEFORE moving accumulators into the message.
            let content_was_empty = content_acc.is_empty();
            let reasoning_was_empty = reasoning_acc.is_empty();

            // Guard against spurious empty responses (API hiccup): if the
            // model returned NOTHING (no content, no reasoning, no tools)
            // and we're past iteration 0, DO NOT persist this empty message
            // — it would corrupt the message sequence (API requires
            // assistant{tool_calls} to be immediately followed by tool
            // results). Instead, skip and retry.
            if assembled.is_empty() && content_was_empty && reasoning_was_empty && iter > 0 {
                warn!(
                    thread_id = %state.id,
                    iter,
                    "empty response (no content, no reasoning, no tools) — skipping, will retry"
                );
                continue;
            }

            // Build + persist the assistant message.
            let mut assistant_msg = ChatMessage::assistant(&content_acc);
            if !reasoning_acc.is_empty() {
                assistant_msg.reasoning_content = Some(reasoning_acc);
            }
            if !assembled.is_empty() {
                assistant_msg.tool_calls = Some(assembled.clone());
            }
            self.persist_message(&state, &assistant_msg).await?;
            state.log.write().append(assistant_msg);

            // Termination: model produced no tool calls.
            if assembled.is_empty() {
                if !stop_hook_blocked_once {
                    let stop_hook = turn_hook_runner
                        .run(
                            HookEvent::Stop,
                            hooks::stop_input(
                                &state.id,
                                &state.id,
                                state.cwd.as_deref(),
                                &permission_mode,
                            ),
                            None,
                            state.cwd.as_deref(),
                            &abort,
                        )
                        .await;
                    if let Some(reason) = stop_hook.blocking_error {
                        stop_hook_blocked_once = true;
                        let reminder = ChatMessage::system(format!(
                            "<system-reminder>\nStop hook blocked turn completion: {reason}\n继续处理，直到 Stop hook 允许结束。\n</system-reminder>"
                        ));
                        self.persist_message(&state, &reminder).await?;
                        state.log.write().append(reminder);
                        continue;
                    }
                } else {
                    let stop_failure = turn_hook_runner
                        .run(
                            HookEvent::StopFailure,
                            serde_json::json!({
                                "session_id": state.id.clone(),
                                "thread_id": state.id.clone(),
                                "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                                "permission_mode": permission_mode,
                                "hook_event_name": "StopFailure",
                            }),
                            None,
                            state.cwd.as_deref(),
                            &abort,
                        )
                        .await;
                    for trace in &stop_failure.traces {
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
                self.log_tool_dispatch_summary(&state, &tool_dispatch_trace, &message_id);
                log_turn_diff_summary(&state);
                self.finalize_turn_complete(&state, &model, &message_id, &final_usage, &event_tx);
                info!(thread_id = %state.id, iter, "turn complete (no tool calls)");
                return Ok(());
            }

            info!(
                thread_id = %state.id,
                iter,
                count = assembled.len(),
                "executing tool calls",
            );

            // Execute tool calls in batches: consecutive concurrency-safe
            // (read-only) calls run concurrently, mutating calls run serially
            // (Roadmap GAP-PARALLEL-001, mirrors Claude's partitionToolCalls).
            //
            // Emit ToolCallStart for any calls NOT already emitted during
            // streaming (fallback for edge cases where name arrived late).
            // For early-emitted calls, emit a ToolCallUpdate with the now-
            // parsed full input so the frontend can update the card.
            for api_call in &assembled {
                let input: serde_json::Value =
                    crate::repair::parse_tool_args(&api_call.function.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                if early_emitted_ids.contains(&api_call.id) {
                    // Already shown to user during streaming — backfill the
                    // now-fully-parsed input. Keep status PendingApproval so the
                    // card stays in "awaiting" until the permission gate decides
                    // (resolve_permission emits Running once cleared).
                    let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                        thread_id: state.id.clone(),
                        message_id: message_id.clone(),
                        tool_use_id: api_call.id.clone(),
                        status: ToolStatusEvent::PendingApproval,
                        input: Some(input.clone()),
                        result: None,
                        duration_ms: None,
                        error_message: None,
                    });
                } else {
                    let _ = event_tx.send(EngineEvent::ToolCallStart {
                        thread_id: state.id.clone(),
                        message_id: message_id.clone(),
                        tool_use_id: api_call.id.clone(),
                        tool_name: api_call.function.name.clone(),
                        input: input.clone(),
                    });
                }
            }

            let mut done_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
            for batch in self.partition_tool_calls(&assembled) {
                if abort.is_cancelled() {
                    self.finalize_aborted_tool_calls(
                        &state,
                        &assembled,
                        &done_ids,
                        &message_id,
                        Arc::clone(&tool_dispatch_trace),
                        &event_tx,
                    )
                    .await?;
                    let _ = event_tx.send(EngineEvent::Aborted {
                        thread_id: state.id.clone(),
                        message_id: message_id.clone(),
                    });
                    return Ok(());
                }
                if batch.concurrent && batch.calls.len() > 1 {
                    // Run read-only calls concurrently. Tool result ordering
                    // in the message log is non-deterministic across a
                    // concurrent batch, but each result carries its
                    // `tool_call_id` so the API pairs them by id, not
                    // position.
                    //
                    // Sibling abort (aligned to Claude Code
                    // StreamingToolExecutor.siblingAbortController): if any
                    // tool in the batch fails, cancel sibling calls so they
                    // don't waste resources or compound the error.
                    let sibling_abort = CancellationToken::new();
                    let ids: Vec<String> = batch.calls.iter().map(|c| c.id.clone()).collect();
                    let futures: Vec<_> = batch
                        .calls
                        .iter()
                        .map(|c| {
                            self.execute_one_tool_call(
                                c,
                                &state,
                                &abort,
                                &message_id,
                                turn_user_seq,
                                Arc::clone(&tool_failure_ledger),
                                Arc::clone(&tool_dispatch_trace),
                                &event_tx,
                                sibling_abort.clone(),
                            )
                        })
                        .collect();
                    let results = futures::future::join_all(futures).await;
                    for r in results {
                        if let Err(e) = r {
                            // Unrecoverable — bail the turn.
                            return Err(e);
                        }
                    }
                    done_ids.extend(ids);
                } else {
                    // Serial (mutating) calls each get a no-op sibling token.
                    let noop_abort = CancellationToken::new();
                    for c in &batch.calls {
                        self.execute_one_tool_call(
                            c,
                            &state,
                            &abort,
                            &message_id,
                            turn_user_seq,
                            Arc::clone(&tool_failure_ledger),
                            Arc::clone(&tool_dispatch_trace),
                            &event_tx,
                            noop_abort.clone(),
                        )
                        .await?;
                        done_ids.insert(c.id.clone());
                    }
                }
            }

            // Post-response context decision. Now that tool results are in
            // the log and we have a precise `prompt_tokens` from this turn's
            // usage, decide whether the NEXT iteration would be over-budget.
            // Folding here (between iterations) compacts old history before
            // the next API call, keeping the prefix cache warm for the tail.
            match compaction::decide_after_usage(
                &final_usage,
                &model,
                provider,
                already_folded_this_turn,
            ) {
                PostUsageDecision::Fold { tail_budget, .. } => {
                    if self
                        .fold(&state, tail_budget, &abort)
                        .await
                        .unwrap_or(false)
                    {
                        already_folded_this_turn = true;
                        self.emit_local_context_usage(&state, &event_tx);
                    }
                }
                PostUsageDecision::ExitWithSummary { ratio, .. } => {
                    // Context is critically full (>80%) and folding either
                    // already ran or wouldn't help enough. Fold once more as
                    // a last resort, then stop the turn cleanly so we never
                    // send an over-budget request that would 400.
                    let ctx_max = self.effective_context_window(provider, &model);
                    let tail_budget =
                        (ctx_max as f64 * compaction::FOLD_AGGRESSIVE_TAIL_FRACTION) as usize;
                    let folded = self
                        .fold(&state, tail_budget, &abort)
                        .await
                        .unwrap_or(false);
                    if folded {
                        self.emit_local_context_usage(&state, &event_tx);
                    }
                    if !folded {
                        // Couldn't compact further — end the turn with a
                        // visible notice instead of looping into a 400.
                        warn!(
                            thread_id = %state.id,
                            ratio,
                            "context critically full and fold could not help; ending turn"
                        );
                        let notice = "上下文已接近模型上限，且无法进一步压缩。本轮已停止。请开启新会话，或精简后继续。";
                        let _ = event_tx.send(EngineEvent::ContentDelta {
                            thread_id: state.id.clone(),
                            message_id: message_id.clone(),
                            delta: notice.to_string(),
                        });
                        let notice_msg = ChatMessage::assistant(notice);
                        self.persist_message(&state, &notice_msg).await?;
                        state.log.write().append(notice_msg);
                        let _ = event_tx.send(EngineEvent::TurnComplete {
                            thread_id: state.id.clone(),
                            message_id,
                            usage: final_usage.clone(),
                            cost_usd: 0.0,
                        });
                        return Ok(());
                    }
                }
                PostUsageDecision::None { .. } => {}
            }
        }

        let msg = format!(
            "Agent stopped: exceeded max iterations ({})",
            MAX_TURN_ITERATIONS
        );
        error!(thread_id = %state.id, "{}", msg);
        let _ = event_tx.send(EngineEvent::Error {
            thread_id: state.id.clone(),
            message_id: None,
            error: msg.clone(),
            retryable: false,
        });
        Err(anyhow::anyhow!(msg))
    }

    /// Resolve the model to use for fold summaries.
    ///
    /// ## Provider neutrality
    ///
    /// DeepSeek uses its cheap `deepseek-v4-flash` for summaries regardless
    /// of the turn model (a pro turn still summarizes on flash — fine, the
    /// summary is internal). For any other provider we do NOT inject a
    /// DeepSeek model name; we fall back to the thread's own model so the
    /// call stays valid for that provider (see
    /// `.kiro/steering/provider-neutrality.md`).
    fn summary_model_for(provider: ProviderId, turn_model: &str) -> String {
        match provider {
            ProviderId::Deepseek => "deepseek-v4-flash".to_string(),
            _ => turn_model.to_string(),
        }
    }

    /// Fold (compact) the thread's conversation log.
    ///
    /// Splits the log into `head` (old, to summarize) + `tail` (recent, to
    /// keep), summarizes the head with a cheap model call that reuses the
    /// verbatim system prompt + tools for prefix-cache alignment, then
    /// replaces the log with `[summary, ...tail]` both in memory and on disk.
    ///
    /// Returns `Ok(true)` when a fold actually happened, `Ok(false)` when it
    /// was a no-op (boundary not worthwhile, summary failed/timed out, or
    /// abort). A no-op is always safe — the turn just continues uncompacted.
    ///
    /// `tail_budget_tokens`: token budget for the recent tail to keep.
    async fn fold(
        &self,
        state: &Arc<ThreadState>,
        tail_budget_tokens: usize,
        abort: &CancellationToken,
    ) -> Result<bool> {
        // Snapshot the log under a short read lock; the summary HTTP call
        // happens without holding the lock.
        let all: Vec<ChatMessage> = state.log.read().messages().to_vec();
        let boundary = match compaction::find_fold_boundary(&all, tail_budget_tokens) {
            Some(b) => b,
            None => return Ok(false),
        };

        let head = &all[..boundary];
        let tail = &all[boundary..];
        if head.is_empty() {
            return Ok(false);
        }

        let permission_mode = state.permission_ctx.read().mode.as_str().to_string();
        let pre_compact = HookRunner::load(state.cwd.as_deref())
            .run(
                HookEvent::PreCompact,
                serde_json::json!({
                    "session_id": state.id.clone(),
                    "thread_id": state.id.clone(),
                    "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                    "permission_mode": permission_mode,
                    "hook_event_name": "PreCompact",
                    "message_count": all.len(),
                    "head_count": head.len(),
                    "tail_count": tail.len(),
                    "tail_budget_tokens": tail_budget_tokens,
                }),
                None,
                state.cwd.as_deref(),
                abort,
            )
            .await;
        for trace in &pre_compact.traces {
            tracing::debug!(
                event = %trace.event,
                hook_id = ?trace.hook_id,
                source = ?trace.source,
                outcome = %trace.outcome,
                duration_ms = trace.duration_ms,
                "hook trace"
            );
        }
        if let Some(reason) = pre_compact.blocking_error {
            warn!(
                thread_id = %state.id,
                reason = %reason,
                "PreCompact hook blocked context fold"
            );
            return Ok(false);
        }

        // Build the cache-aligned summary request: verbatim system prompt +
        // head + instruction, with tools forwarded for prefix-cache reuse.
        let system_prompt = state.prefix.system_prompt();
        let instruction = compaction::build_fold_summary_instruction();
        let summary_messages =
            compaction::build_fold_summary_messages(system_prompt, head, &instruction);
        let provider = *state.provider.read();
        let provider_id = state.provider_id.read().clone();
        let opts = compaction::fold_summary_opts(build_tool_specs(), provider);

        let turn_model = state.model.read().clone();
        let summary_model = Self::summary_model_for(provider, &turn_model);
        let fold_client = self
            .client_resolver
            .read()
            .as_ref()
            .and_then(|r| r.client_for(&provider_id))
            .unwrap_or_else(|| self.client.clone());

        // Race the summary call against the turn abort + a hard timeout so a
        // hung request can never stall the loop.
        let summary_fut = fold_client.chat_with_opts(summary_messages, &summary_model, opts);
        let timeout = tokio::time::Duration::from_millis(compaction::SUMMARY_TIMEOUT_MS);
        let resp = tokio::select! {
            biased;
            _ = abort.cancelled() => return Ok(false),
            r = tokio::time::timeout(timeout, summary_fut) => match r {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => {
                    warn!(thread_id = %state.id, error = %e, "fold summary call failed; skipping fold");
                    return Ok(false);
                }
                Err(_elapsed) => {
                    warn!(thread_id = %state.id, "fold summary timed out; skipping fold");
                    return Ok(false);
                }
            },
        };

        let summary_text = resp.content.trim();
        if summary_text.is_empty() {
            warn!(thread_id = %state.id, "fold summary empty; skipping fold");
            return Ok(false);
        }

        // Build replacement = [summary_msg, ...tail].
        let summary_msg = compaction::build_summary_message(summary_text, provider);

        // Replace in-memory log. Re-acquire the write lock; if the log grew
        // since the snapshot (a message raced in during the awaited summary
        // call), preserve the newly-arrived suffix so the fold never drops it.
        let replacement = {
            let mut log = state.log.write();
            let replacement =
                compaction::assemble_fold_replacement(summary_msg, tail, all.len(), log.messages());
            log.compact_in_place(replacement.clone());
            replacement
        };

        // Mirror the rewrite to disk so an LRU eviction / restart doesn't
        // resurrect the elided history. Best-effort: an in-memory fold still
        // helps even if persistence fails.
        let rewrite_rows: Vec<(String, String)> = replacement
            .iter()
            .filter_map(|m| {
                serde_json::to_string(m)
                    .ok()
                    .map(|json| (m.role.clone(), json))
            })
            .collect();
        let thread_id = state.id.clone();
        let db = self.db.clone();
        let rewrite_result = tokio::task::spawn_blocking(move || {
            let mrepo = MessageRepo::new(db.as_ref());
            mrepo.rewrite_thread(&thread_id, rewrite_rows)
        })
        .await;
        match rewrite_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(thread_id = %state.id, error = %e, "fold persist failed (in-memory fold still applied)")
            }
            Err(e) => warn!(thread_id = %state.id, error = %e, "fold persist task panicked"),
        }

        // Folding rewrote history → the read-before-edit tracker's snapshots
        // no longer correspond to anything in the (now-summarized) log. Clear
        // it so a stale read can't authorize an edit against vanished context.
        state.file_state.lock().clear();

        info!(
            thread_id = %state.id,
            before = all.len(),
            after = replacement.len(),
            summary_chars = summary_text.len(),
            "context fold complete",
        );
        let post_compact = HookRunner::load(state.cwd.as_deref())
            .run(
                HookEvent::PostCompact,
                serde_json::json!({
                    "session_id": state.id.clone(),
                    "thread_id": state.id.clone(),
                    "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                    "permission_mode": state.permission_ctx.read().mode.as_str(),
                    "hook_event_name": "PostCompact",
                    "before_count": all.len(),
                    "after_count": replacement.len(),
                    "summary_chars": summary_text.len(),
                }),
                None,
                state.cwd.as_deref(),
                abort,
            )
            .await;
        for trace in &post_compact.traces {
            tracing::debug!(
                event = %trace.event,
                hook_id = ?trace.hook_id,
                source = ?trace.source,
                outcome = %trace.outcome,
                duration_ms = trace.duration_ms,
                "hook trace"
            );
        }
        Ok(true)
    }

    /// Emit a local-tokenizer context-usage measurement (used right after a
    /// fold, before the next API round-trip gives a precise count).
    fn emit_local_context_usage(
        &self,
        state: &Arc<ThreadState>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let model = state.model.read().clone();
        let provider = *state.provider.read();
        let ctx_max = self.effective_context_window(provider, &model);
        let messages = self.build_messages(state);
        let est = compaction::estimate_turn_start(&messages, &model, provider);
        let used = est.estimate_tokens.min(ctx_max) as u64;
        let _ = event_tx.send(EngineEvent::ContextUsage {
            thread_id: state.id.clone(),
            used_tokens: used,
            max_tokens: ctx_max as u64,
            ratio: used as f64 / ctx_max as f64,
            source: compaction::ContextUsageSource::Local,
        });
    }

    /// Partition tool calls into batches of consecutive concurrency-safe
    /// (read-only) calls vs single mutating calls. Mirrors Claude's
    /// partitionToolCalls.
    fn partition_tool_calls(&self, assembled: &[ApiToolCall]) -> Vec<ToolBatch> {
        let mut batches: Vec<ToolBatch> = Vec::new();
        for call in assembled {
            // Per-call parallel safety: parse the call's arguments so tools
            // like `task` can decide based on `agent_type` (read-only explore/
            // plan sub-agents are concurrency-safe; writable ones are not).
            let args: serde_json::Value =
                serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::Value::Null);
            let is_safe = self
                .tools
                .get(&call.function.name)
                .map(|t| t.is_call_parallel_safe(&args))
                .unwrap_or(false);
            if is_safe {
                if let Some(last) = batches.last_mut() {
                    if last.concurrent {
                        last.calls.push(call.clone());
                        continue;
                    }
                }
                batches.push(ToolBatch {
                    concurrent: true,
                    calls: vec![call.clone()],
                });
            } else {
                batches.push(ToolBatch {
                    concurrent: false,
                    calls: vec![call.clone()],
                });
            }
        }
        batches
    }

    /// Emit turn-completion bookkeeping after a model response with no tool
    /// calls: compute cost via the active price table, persist a usage row
    /// (best-effort — a DB failure logs and continues so cost tracking never
    /// breaks the chat path), then emit `TurnComplete` + an API-precise
    /// `ContextUsage` event for the frontend ring.
    fn finalize_turn_complete(
        &self,
        state: &Arc<ThreadState>,
        model: &str,
        message_id: &str,
        final_usage: &Usage,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        let provider = *state.provider.read();
        let breakdown = UsageBreakdown::from_usage(provider, final_usage);
        let cost_usd = pricing::compute_cost(provider, model, breakdown);

        let urepo = UsageRepo::new(self.db.as_ref());
        if let Err(e) = urepo.insert(UsageInsert {
            thread_id: state.id.clone(),
            message_id: message_id.to_string(),
            provider_id: provider.as_str().into(),
            model: model.to_string(),
            cache_read_tokens: breakdown.cache_read_tokens,
            cache_miss_tokens: breakdown.cache_miss_tokens,
            cache_creation_tokens: breakdown.cache_creation_tokens,
            output_tokens: breakdown.output_tokens,
            cost_usd,
            created_at: chrono::Utc::now().timestamp_millis(),
        }) {
            warn!(error = %e, thread_id = %state.id, "usage row insert failed");
        }

        let _ = event_tx.send(EngineEvent::TurnComplete {
            thread_id: state.id.clone(),
            message_id: message_id.to_string(),
            usage: final_usage.clone(),
            cost_usd,
        });

        // Emit context usage (API-precise) for the frontend ring.
        let ctx_max = self.effective_context_window(provider, model);
        let used = final_usage.prompt_tokens as u64;
        let _ = event_tx.send(EngineEvent::ContextUsage {
            thread_id: state.id.clone(),
            used_tokens: used,
            max_tokens: ctx_max as u64,
            ratio: used as f64 / ctx_max as f64,
            source: crate::compaction::ContextUsageSource::Api,
        });
    }

    /// On abort, emit Aborted status + append a tool-result message for every
    /// tool_call in `assembled` that has not yet been resolved (`done_ids`).
    /// Keeps the message sequence valid (assistant tool_calls must each have a
    /// matching tool result) so the next turn / a reload never 400s, and so
    /// the cards restore correctly after restart (P0).
    async fn finalize_aborted_tool_calls(
        &self,
        state: &Arc<ThreadState>,
        assembled: &[ApiToolCall],
        done_ids: &std::collections::HashSet<String>,
        message_id: &str,
        tool_dispatch_trace: Arc<Mutex<ToolDispatchTrace>>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<()> {
        for call in assembled {
            if done_ids.contains(&call.id) {
                continue;
            }
            let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                thread_id: state.id.clone(),
                message_id: message_id.to_string(),
                tool_use_id: call.id.clone(),
                status: ToolStatusEvent::Aborted,
                input: None,
                result: None,
                duration_ms: None,
                error_message: None,
            });
            self.append_tool_result_msg(
                state,
                &call.id,
                "[用户已中止此工具调用]".to_string(),
                true,
                None,
            )
            .await?;
            tool_dispatch_trace.lock().record(ToolDispatchEntry {
                tool_use_id: call.id.clone(),
                tool_name: call.function.name.clone(),
                status: ToolTraceStatus::Aborted,
                duration_ms: Some(0),
                category: Some("aborted"),
                subgoal: active_todo_label(state),
            });
        }
        Ok(())
    }

    /// Execute the full per-call flow (parse args → permission → execution →
    /// events → result append) for a single tool call. Returns `Ok(())`
    /// unless an unrecoverable error occurs (in which case the caller bails
    /// out of the turn). All recoverable conditions (unknown tool, denied
    /// permission, malformed args, tool error) are surfaced as events and a
    /// tool-role result message, and still return `Ok(())`.
    async fn execute_one_tool_call(
        &self,
        api_call: &ApiToolCall,
        state: &Arc<ThreadState>,
        abort: &CancellationToken,
        message_id: &str,
        turn_user_seq: i64,
        tool_failure_ledger: Arc<Mutex<ToolFailureLedger>>,
        tool_dispatch_trace: Arc<Mutex<ToolDispatchTrace>>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        sibling_abort: CancellationToken,
    ) -> Result<()> {
        // Sibling-abort check: if a sibling tool has already failed and
        // cancelled this token, skip execution.
        if sibling_abort.is_cancelled() {
            let msg = "[sibling tool failed — call skipped]".to_string();
            let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                thread_id: state.id.clone(),
                message_id: message_id.to_string(),
                tool_use_id: api_call.id.clone(),
                status: ToolStatusEvent::Aborted,
                input: None,
                result: None,
                duration_ms: Some(0),
                error_message: None,
            });
            // Scope the lock guards so they drop before the .await below —
            // parking_lot MutexGuard is !Send and must not live across .await.
            let (recovery, trace_entry) = {
                let recovery = tool_failure_ledger.lock().record(
                    &api_call.function.name,
                    &msg,
                    active_todo_label(state),
                );
                let entry = ToolDispatchEntry {
                    tool_use_id: api_call.id.clone(),
                    tool_name: api_call.function.name.clone(),
                    status: ToolTraceStatus::Aborted,
                    duration_ms: Some(0),
                    category: Some(recovery.category),
                    subgoal: recovery.subgoal.clone(),
                };
                (recovery, entry)
            };
            tool_dispatch_trace.lock().record(trace_entry);
            self.append_tool_result_msg(state, &api_call.id, msg, true, Some(recovery))
                .await?;
            return Ok(());
        }

        let input: serde_json::Value =
            match crate::repair::parse_tool_args(&api_call.function.arguments) {
                Ok(v) => v,
                Err(reason) => {
                    warn!(
                        tool = %api_call.function.name,
                        args = %api_call.function.arguments,
                        %reason,
                        "unparseable tool arguments",
                    );
                    let msg = format!(
                    "Invalid tool arguments: {reason}. Please re-issue the call with valid JSON."
                );
                    let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                        thread_id: state.id.clone(),
                        message_id: message_id.to_string(),
                        tool_use_id: api_call.id.clone(),
                        status: ToolStatusEvent::Error,
                        input: None,
                        result: None,
                        duration_ms: Some(0),
                        error_message: Some(msg.clone()),
                    });
                    let (recovery, trace_entry) = {
                        let recovery = tool_failure_ledger.lock().record(
                            &api_call.function.name,
                            &msg,
                            active_todo_label(state),
                        );
                        let entry = ToolDispatchEntry {
                            tool_use_id: api_call.id.clone(),
                            tool_name: api_call.function.name.clone(),
                            status: ToolTraceStatus::Error,
                            duration_ms: Some(0),
                            category: Some(recovery.category),
                            subgoal: recovery.subgoal.clone(),
                        };
                        (recovery, entry)
                    };
                    tool_dispatch_trace.lock().record(trace_entry);
                    self.append_tool_result_msg(state, &api_call.id, msg, true, Some(recovery))
                        .await?;
                    return Ok(());
                }
            };

        let tool = match self.tools.get(&api_call.function.name) {
            Some(t) => t,
            None => {
                let msg = format!("Unknown tool: {}", api_call.function.name);
                let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                    thread_id: state.id.clone(),
                    message_id: message_id.to_string(),
                    tool_use_id: api_call.id.clone(),
                    status: ToolStatusEvent::Error,
                    input: None,
                    result: None,
                    duration_ms: Some(0),
                    error_message: Some(msg.clone()),
                });
                let (recovery, trace_entry) = {
                    let recovery = tool_failure_ledger.lock().record(
                        &api_call.function.name,
                        &msg,
                        active_todo_label(state),
                    );
                    let entry = ToolDispatchEntry {
                        tool_use_id: api_call.id.clone(),
                        tool_name: api_call.function.name.clone(),
                        status: ToolTraceStatus::Error,
                        duration_ms: Some(0),
                        category: Some(recovery.category),
                        subgoal: recovery.subgoal.clone(),
                    };
                    (recovery, entry)
                };
                tool_dispatch_trace.lock().record(trace_entry);
                self.append_tool_result_msg(state, &api_call.id, msg, true, Some(recovery))
                    .await?;
                return Ok(());
            }
        };

        let hook_runner = HookRunner::load(state.cwd.as_deref());
        let permission_mode = state.permission_ctx.read().mode.as_str().to_string();
        let pre_hook = hook_runner
            .run(
                HookEvent::PreToolUse,
                hooks::pre_tool_input(
                    &state.id,
                    &state.id,
                    state.cwd.as_deref(),
                    &permission_mode,
                    &api_call.function.name,
                    &input,
                ),
                Some(&api_call.function.name),
                state.cwd.as_deref(),
                abort,
            )
            .await;
        for trace in &pre_hook.traces {
            debug!(
                thread_id = %state.id,
                event = %trace.event,
                command = %trace.command,
                outcome = %trace.outcome,
                duration_ms = trace.duration_ms,
                "hook trace"
            );
        }
        if let Some(reason) =
            pre_hook
                .blocking_error
                .clone()
                .or_else(|| match pre_hook.permission_behavior {
                    Some(HookPermissionBehavior::Deny) => pre_hook
                        .permission_decision_reason
                        .clone()
                        .or_else(|| Some("blocked by PreToolUse hook".into())),
                    _ => None,
                })
        {
            let hook_name = pre_hook
                .traces
                .last()
                .and_then(|t| t.hook_id.clone())
                .unwrap_or_else(|| "PreToolUse".into());
            let msg = format!(
                "<tool_use_error>Hook `{hook_name}` blocked tool `{}`: {reason}</tool_use_error>",
                api_call.function.name
            );
            let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                thread_id: state.id.clone(),
                message_id: message_id.to_string(),
                tool_use_id: api_call.id.clone(),
                status: ToolStatusEvent::Error,
                input: pre_hook.updated_input.clone(),
                result: None,
                duration_ms: Some(0),
                error_message: Some(msg.clone()),
            });
            let (recovery, trace_entry) = {
                let recovery = tool_failure_ledger.lock().record(
                    &api_call.function.name,
                    &msg,
                    active_todo_label(state),
                );
                let entry = ToolDispatchEntry {
                    tool_use_id: api_call.id.clone(),
                    tool_name: api_call.function.name.clone(),
                    status: ToolTraceStatus::Error,
                    duration_ms: Some(0),
                    category: Some("hook_blocked"),
                    subgoal: recovery.subgoal.clone(),
                };
                (recovery, entry)
            };
            tool_dispatch_trace.lock().record(trace_entry);
            self.append_tool_result_msg(state, &api_call.id, msg, true, Some(recovery))
                .await?;
            return Ok(());
        }
        let permission_input = pre_hook.updated_input.clone().unwrap_or(input);

        // Permission resolution. Returns:
        // - Some(updated_input) → allowed, execute with this input
        // - None → denied (already wrote the deny message + emitted
        //          ToolCallUpdate(Error))
        let final_input = if matches!(
            pre_hook.permission_behavior,
            Some(HookPermissionBehavior::Allow)
        ) {
            permission_input
        } else {
            match self
                .resolve_permission(
                    tool.as_ref(),
                    &permission_input,
                    state,
                    &api_call.id,
                    abort,
                    message_id,
                    Arc::clone(&tool_failure_ledger),
                    Arc::clone(&tool_dispatch_trace),
                    event_tx,
                )
                .await
            {
                Ok(Some(updated)) => updated,
                Ok(None) => return Ok(()),
                Err(e) => {
                    let _ = event_tx.send(EngineEvent::Error {
                        thread_id: state.id.clone(),
                        message_id: Some(message_id.to_string()),
                        error: e.to_string(),
                        retryable: false,
                    });
                    return Err(e);
                }
            }
        };

        // Pre-execution repeat-failure check: if the same (subgoal, tool,
        // category) has already failed >= 3 times this turn, block the call
        // before dispatch so the model is forced to try a different approach.
        let active_subgoal = active_todo_label(state);
        // Scope lock guard before .await: parking_lot MutexGuard is !Send.
        let blocked_category = {
            tool_failure_ledger
                .lock()
                .should_block(&api_call.function.name, active_subgoal.as_deref())
        };
        if let Some(blocked_category) = blocked_category {
            // Build a summary of all prior failed attempts so the model
            // knows what was already tried, not just that it was blocked.
            let tried_tools = {
                let ledger = tool_failure_ledger.lock();
                ledger
                    .by_tool_category
                    .iter()
                    .filter(|((sg, _t, _c), _count)| sg.as_deref() == active_subgoal.as_deref())
                    .map(|((_sg, t, c), count)| format!("{t} ({c}, {count}次)"))
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let tried_summary = if tried_tools.is_empty() {
                String::new()
            } else {
                format!("\n已尝试路径: {tried_tools}")
            };
            let msg = format!(
                "引擎拦截: 工具 `{}` 在当前子目标 `{}` 下已因 `{}` 类错误连续失败。\
                 禁止重复同一调用。请换一种实质不同的方法，或缩小目标范围。\
                 {tried_summary}\n\
                 建议: 先用搜索/glob确认资源存在，再执行操作；或拆分为更小的子目标逐个推进。",
                &api_call.function.name,
                active_subgoal.as_deref().unwrap_or("(无)"),
                blocked_category,
            );
            warn!(
                thread_id = %state.id,
                tool = %api_call.function.name,
                category = blocked_category,
                subgoal = active_subgoal.as_deref().unwrap_or(""),
                "blocking repeated tool failure before dispatch"
            );
            let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                thread_id: state.id.clone(),
                message_id: message_id.to_string(),
                tool_use_id: api_call.id.clone(),
                status: ToolStatusEvent::Error,
                input: None,
                result: None,
                duration_ms: Some(0),
                error_message: Some(msg.clone()),
            });
            // Scope lock guards before .await so they don't live across it.
            let (recovery, trace_entry) = {
                let recovery = tool_failure_ledger.lock().record(
                    &api_call.function.name,
                    &msg,
                    active_subgoal.clone(),
                );
                let entry = ToolDispatchEntry {
                    tool_use_id: api_call.id.clone(),
                    tool_name: api_call.function.name.clone(),
                    status: ToolTraceStatus::Error,
                    duration_ms: Some(0),
                    category: Some(recovery.category),
                    subgoal: recovery.subgoal.clone(),
                };
                (recovery, entry)
            };
            tool_dispatch_trace.lock().record(trace_entry);
            self.append_tool_result_msg(state, &api_call.id, msg, true, Some(recovery))
                .await?;
            return Ok(());
        }

        let executed_input = final_input.clone();
        let inner = InnerToolCall {
            id: api_call.id.clone(),
            name: api_call.function.name.clone(),
            arguments: final_input,
        };
        let ctx = deepseek_tools::context::ToolContext {
            file_state: Arc::clone(&state.file_state),
            cwd: state.cwd.clone(),
            abort: abort.clone(),
            todos: Arc::clone(&state.todos),
            thread_id: Some(state.id.clone()),
            message_seq: Some(turn_user_seq),
            file_history: Some(self.file_history_sink.clone()),
            subagent: self.subagent.read().clone(),
            question_gate: self.question_gate.read().clone(),
            current_tool_use_id: Some(api_call.id.clone()),
            turn_diff: Some(Arc::new(EngineTurnDiffRecorder {
                diff: Arc::clone(&state.turn_diff),
            })),
        };
        let mut result = self.tools.execute(&inner, &ctx).await;

        let post_event = if result.is_error || shell_output_indicates_failure(&result) {
            HookEvent::PostToolUseFailure
        } else {
            HookEvent::PostToolUse
        };
        let post_hook = hook_runner
            .run(
                post_event,
                hooks::post_tool_input(
                    post_event,
                    &state.id,
                    &state.id,
                    state.cwd.as_deref(),
                    &permission_mode,
                    &api_call.function.name,
                    &executed_input,
                    &result.content,
                ),
                Some(&api_call.function.name),
                state.cwd.as_deref(),
                abort,
            )
            .await;
        for trace in &post_hook.traces {
            debug!(
                thread_id = %state.id,
                event = %trace.event,
                command = %trace.command,
                outcome = %trace.outcome,
                duration_ms = trace.duration_ms,
                "hook trace"
            );
        }
        if !post_hook.additional_contexts.is_empty() {
            result.content.push_str("\n\n<hook_context>\n");
            result
                .content
                .push_str(&post_hook.additional_contexts.join("\n\n"));
            result.content.push_str("\n</hook_context>");
        }
        if let Some(reason) = post_hook.blocking_error {
            result.is_error = true;
            result.content.push_str(&format!(
                "\n\n<tool_use_error>Hook `{}` blocked continuation after `{}`: {}</tool_use_error>",
                post_hook
                    .traces
                    .last()
                    .and_then(|t| t.hook_id.clone())
                    .unwrap_or_else(|| post_event.as_str().into()),
                api_call.function.name,
                reason
            ));
        }

        if !result.is_error {
            if let Some(path) =
                changed_path_from_tool_input(&api_call.function.name, &executed_input)
            {
                let file_changed = hook_runner
                    .run(
                        HookEvent::FileChanged,
                        serde_json::json!({
                            "session_id": state.id.clone(),
                            "thread_id": state.id.clone(),
                            "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                            "permission_mode": permission_mode,
                            "hook_event_name": "FileChanged",
                            "tool_name": api_call.function.name,
                            "tool_input": executed_input,
                            "path": path,
                        }),
                        Some(api_call.function.name.as_str()),
                        state.cwd.as_deref(),
                        abort,
                    )
                    .await;
                for trace in &file_changed.traces {
                    tracing::debug!(
                        thread_id = %state.id,
                        event = %trace.event,
                        command = %trace.command,
                        outcome = %trace.outcome,
                        duration_ms = trace.duration_ms,
                        "hook trace"
                    );
                }
            }
        }

        let status = if result.is_error {
            ToolStatusEvent::Error
        } else {
            ToolStatusEvent::Success
        };
        let _ = event_tx.send(EngineEvent::ToolCallUpdate {
            thread_id: state.id.clone(),
            message_id: message_id.to_string(),
            tool_use_id: result.tool_use_id.clone(),
            status,
            input: None,
            result: Some(result.content.clone()),
            duration_ms: Some(result.duration_ms),
            error_message: None,
        });

        // Surface todo list changes to the UI. The todo_write tool
        // mutated the shared list in `state.todos`; emit a snapshot
        // so the frontend can render the updated task list.
        if api_call.function.name == "todo_write" && !result.is_error {
            let todos = state.todos.lock().clone();
            let _ = event_tx.send(EngineEvent::TodosUpdated {
                thread_id: state.id.clone(),
                todos,
            });
        }

        // Scope lock guards before .await.
        let (recovery, trace_entry) = {
            let recovery = if result.is_error || shell_output_indicates_failure(&result) {
                // Sibling abort: if this tool failed, cancel sibling calls in
                // the concurrent batch so they don't waste resources.
                if result.is_error {
                    sibling_abort.cancel();
                }
                Some(tool_failure_ledger.lock().record(
                    &result.tool_name,
                    &result.content,
                    active_subgoal.clone(),
                ))
            } else {
                None
            };
            let trace_status = if result.is_error {
                ToolTraceStatus::Error
            } else if recovery.is_some() {
                ToolTraceStatus::Recoverable
            } else {
                ToolTraceStatus::Success
            };
            let entry = ToolDispatchEntry {
                tool_use_id: result.tool_use_id.clone(),
                tool_name: result.tool_name.clone(),
                status: trace_status,
                duration_ms: Some(result.duration_ms),
                category: recovery.as_ref().map(|snapshot| snapshot.category),
                subgoal: recovery
                    .as_ref()
                    .and_then(|snapshot| snapshot.subgoal.clone())
                    .or(active_subgoal),
            };
            (recovery, entry)
        };
        tool_dispatch_trace.lock().record(trace_entry);

        self.append_tool_result_msg(
            state,
            &result.tool_use_id,
            result.content,
            result.is_error,
            recovery,
        )
        .await?;
        Ok(())
    }

    fn build_messages(&self, state: &ThreadState) -> Vec<ChatMessage> {
        let mut msgs = state.prefix.messages();
        let model = state.model.read().clone();
        let provider_id = state.provider_id.read().clone();
        msgs.push(ChatMessage::system(crate::prompt::crown_identity_block(
            &provider_id,
            &model,
        )));

        // Progressive skill disclosure: inject a system-reminder listing the
        // available skills (name + description only). The model loads a
        // skill's full body on demand via the `skill` tool. Budget = 1% of
        // the context window in chars (~Claude's SKILL_BUDGET_CONTEXT_PERCENT).
        let metas = deepseek_skill::discovery::discover_all(state.cwd.as_deref());
        if !metas.is_empty() {
            let entries: Vec<crate::skills::SkillListEntry> =
                metas.iter().map(Into::into).collect();
            let provider = *state.provider.read();
            let ctx_max = self.effective_context_window(provider, &model);
            let char_budget = (ctx_max as f64 * 4.0 * 0.01) as usize;
            let listing = crate::skills::format_skill_listing(&entries, char_budget);
            if !listing.is_empty() {
                msgs.push(ChatMessage::system(format!(
                    "<system-reminder>\n{listing}\n</system-reminder>"
                )));
            }
        }

        msgs.extend_from_slice(state.log.read().messages());
        // Heal any message sequence violations before sending to the API.
        // This defends against corrupted history (e.g. prior bugs that
        // persisted empty assistant messages between tool_calls and results).
        heal_message_sequence(&mut msgs);
        msgs
    }

    /// Append a message to the thread's persistence layer and refresh the
    /// `threads.preview` + `updated_at` columns.
    ///
    /// Run on `tokio::task::spawn_blocking` so the SQLite write never
    /// stalls the agent loop's runtime.
    async fn persist_message(&self, state: &ThreadState, msg: &ChatMessage) -> Result<()> {
        let json = serde_json::to_string(msg)?;
        let preview = msg
            .content_text()
            .map(|c| c.chars().take(200).collect::<String>())
            .filter(|s| !s.is_empty());
        let role = msg.role.clone();
        let thread_id = state.id.clone();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let mrepo = MessageRepo::new(db.as_ref());
            let trepo = ThreadRepo::new(db.as_ref());
            // Atomic seq assignment (MAX(seq)+1 + INSERT in one tx) — avoids
            // the (thread_id, seq) UNIQUE race between concurrent persists
            // (e.g. sub-agent + main). BUG-E2E-003.
            mrepo.append_next(&thread_id, &role, &json)?;
            trepo.update(
                &thread_id,
                ThreadUpdate {
                    preview: preview.map(Some),
                    touch: true,
                    ..Default::default()
                },
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("persist task panicked: {e}"))??;

        Ok(())
    }

    async fn append_tool_result_msg(
        &self,
        state: &ThreadState,
        tool_use_id: &str,
        content: String,
        is_error: bool,
        recovery: Option<ToolFailureSnapshot>,
    ) -> Result<()> {
        let content = if is_error {
            format_tool_error_for_model(&content, recovery.as_ref())
        } else if let Some(snapshot) = recovery.as_ref() {
            format_tool_recovery_context_for_model(&content, snapshot)
        } else {
            content
        };
        let msg = ChatMessage {
            role: "tool".to_string(),
            content: Some(MessageContent::Text(content)),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_use_id.to_string()),
        };
        self.persist_message(state, &msg).await?;
        state.log.write().append(msg);
        Ok(())
    }

    /// Permission flow: check → if Ask, prompt the gate → apply updates.
    /// Returns `Some(final_input)` if the call should execute, `None` if
    /// it was denied (caller continues to the next tool call).
    ///
    /// Side effects on `None`:
    /// - Emits `ToolCallUpdate { status: Error }`
    /// - Appends a tool-role message with the deny reason so the model
    ///   sees what happened on the next turn
    #[allow(clippy::too_many_arguments)]
    async fn resolve_permission(
        &self,
        tool: &dyn deepseek_tools::Tool,
        input: &serde_json::Value,
        state: &ThreadState,
        tool_use_id: &str,
        abort: &CancellationToken,
        message_id: &str,
        tool_failure_ledger: Arc<Mutex<ToolFailureLedger>>,
        tool_dispatch_trace: Arc<Mutex<ToolDispatchTrace>>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Result<Option<serde_json::Value>> {
        let ctx_snapshot = state.permission_ctx.read().clone();
        let result = check_tool_permission(tool, input, &ctx_snapshot, abort)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        match result {
            PermissionResult::Allow { updated_input, .. } => {
                // Bridge the gap between `pending_approval` (emitted at
                // ToolCallStart) and the post-execution status. Without
                // this transition the UI card would jump straight from
                // "awaiting decision" to "success", losing the
                // "running" phase that long-lived tools (shell, network)
                // need to animate.
                let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                    thread_id: state.id.clone(),
                    message_id: message_id.to_string(),
                    tool_use_id: tool_use_id.to_string(),
                    status: ToolStatusEvent::Running,
                    input: Some(updated_input.clone()),
                    result: None,
                    duration_ms: None,
                    error_message: None,
                });
                Ok(Some(updated_input))
            }
            PermissionResult::Deny { message, .. } => {
                let hook_runner = HookRunner::load(state.cwd.as_deref());
                let permission_mode = ctx_snapshot.mode.as_str();
                let hook_result = hook_runner
                    .run(
                        HookEvent::PermissionDenied,
                        serde_json::json!({
                            "session_id": state.id.clone(),
                            "thread_id": state.id.clone(),
                            "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                            "permission_mode": permission_mode,
                            "hook_event_name": "PermissionDenied",
                            "tool_name": tool.name(),
                            "tool_input": input,
                            "message": message.clone(),
                        }),
                        Some(tool.name()),
                        state.cwd.as_deref(),
                        abort,
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
                let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                    thread_id: state.id.clone(),
                    message_id: message_id.to_string(),
                    tool_use_id: tool_use_id.to_string(),
                    status: ToolStatusEvent::Error,
                    input: None,
                    result: None,
                    duration_ms: Some(0),
                    error_message: Some(message.clone()),
                });
                let (recovery, trace_entry) = {
                    let recovery = tool_failure_ledger.lock().record(
                        tool.name(),
                        &message,
                        active_todo_label(state),
                    );
                    let entry = ToolDispatchEntry {
                        tool_use_id: tool_use_id.to_string(),
                        tool_name: tool.name().to_string(),
                        status: ToolTraceStatus::Error,
                        duration_ms: Some(0),
                        category: Some(recovery.category),
                        subgoal: recovery.subgoal.clone(),
                    };
                    (recovery, entry)
                };
                tool_dispatch_trace.lock().record(trace_entry);
                self.append_tool_result_msg(state, tool_use_id, message, true, Some(recovery))
                    .await?;
                Ok(None)
            }
            PermissionResult::Ask { .. } | PermissionResult::Passthrough { .. } => {
                let hook_runner = HookRunner::load(state.cwd.as_deref());
                let permission_mode = ctx_snapshot.mode.as_str();
                let permission_result_value = serde_json::to_value(&result).unwrap_or_default();
                let request_hook = hook_runner
                    .run(
                        HookEvent::PermissionRequest,
                        serde_json::json!({
                            "session_id": state.id.clone(),
                            "thread_id": state.id.clone(),
                            "cwd": state.cwd.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                            "permission_mode": permission_mode,
                            "hook_event_name": "PermissionRequest",
                            "tool_name": tool.name(),
                            "tool_input": input,
                            "permission_result": permission_result_value,
                        }),
                        Some(tool.name()),
                        state.cwd.as_deref(),
                        abort,
                    )
                    .await;
                for trace in &request_hook.traces {
                    tracing::debug!(
                        event = %trace.event,
                        hook_id = ?trace.hook_id,
                        source = ?trace.source,
                        outcome = %trace.outcome,
                        duration_ms = trace.duration_ms,
                        "hook trace"
                    );
                }
                if matches!(
                    request_hook.permission_behavior,
                    Some(HookPermissionBehavior::Allow)
                ) && request_hook.blocking_error.is_none()
                {
                    let final_in = request_hook
                        .updated_input
                        .clone()
                        .unwrap_or_else(|| input.clone());
                    let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                        thread_id: state.id.clone(),
                        message_id: message_id.to_string(),
                        tool_use_id: tool_use_id.to_string(),
                        status: ToolStatusEvent::Running,
                        input: Some(final_in.clone()),
                        result: None,
                        duration_ms: None,
                        error_message: None,
                    });
                    return Ok(Some(final_in));
                }
                if request_hook.blocking_error.is_some()
                    || matches!(
                        request_hook.permission_behavior,
                        Some(HookPermissionBehavior::Deny)
                    )
                {
                    let feedback = request_hook
                        .blocking_error
                        .clone()
                        .or(request_hook.permission_decision_reason.clone())
                        .unwrap_or_else(|| "Permission denied by hook.".to_string());
                    let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                        thread_id: state.id.clone(),
                        message_id: message_id.to_string(),
                        tool_use_id: tool_use_id.to_string(),
                        status: ToolStatusEvent::Error,
                        input: None,
                        result: None,
                        duration_ms: Some(0),
                        error_message: Some(feedback.clone()),
                    });
                    let (recovery, trace_entry) = {
                        let recovery = tool_failure_ledger.lock().record(
                            tool.name(),
                            &feedback,
                            active_todo_label(state),
                        );
                        let entry = ToolDispatchEntry {
                            tool_use_id: tool_use_id.to_string(),
                            tool_name: tool.name().to_string(),
                            status: ToolTraceStatus::Error,
                            duration_ms: Some(0),
                            category: Some("hook_blocked"),
                            subgoal: recovery.subgoal.clone(),
                        };
                        (recovery, entry)
                    };
                    tool_dispatch_trace.lock().record(trace_entry);
                    self.append_tool_result_msg(state, tool_use_id, feedback, true, Some(recovery))
                        .await?;
                    return Ok(None);
                }
                let req = ApprovalRequest {
                    thread_id: state.id.clone(),
                    tool_use_id: tool_use_id.to_string(),
                    tool_name: tool.name().to_string(),
                    input: input.clone(),
                    description: tool.name().to_string(),
                    cwd: state.cwd.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    permission_result: result,
                };
                let decision = self
                    .gate
                    .ask(req, abort.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!("gate: {e}"))?;
                match decision {
                    ApprovalDecision::Allow {
                        updated_input,
                        permission_updates,
                    } => {
                        if !permission_updates.is_empty() {
                            let mut perm = state.permission_ctx.write();
                            for u in &permission_updates {
                                perm.apply_update(u);
                            }
                        }
                        // The P4 frontend never genuinely edits input, so
                        // `updated_input` is only ever the echoed original —
                        // OR a stale streaming placeholder `{}` (the inline
                        // approval bar can be clicked while ToolCallStart's
                        // empty `{}` is still showing, before args finish
                        // streaming; BUG-E2E-002). In both "no real edit"
                        // cases fall back to the engine's own fully-parsed
                        // `input` so we never execute with empty args.
                        let updated_is_empty_obj = updated_input
                            .as_object()
                            .map(|o| o.is_empty())
                            .unwrap_or(false);
                        let final_in = if updated_input.is_null() || updated_is_empty_obj {
                            input.clone()
                        } else {
                            updated_input
                        };
                        // ApprovalDialog is dismissed; transition the card
                        // from `pending_approval` to `running` before tool
                        // execution starts. Mirror of the fast-path Allow
                        // branch above.
                        let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                            thread_id: state.id.clone(),
                            message_id: message_id.to_string(),
                            tool_use_id: tool_use_id.to_string(),
                            status: ToolStatusEvent::Running,
                            input: Some(final_in.clone()),
                            result: None,
                            duration_ms: None,
                            error_message: None,
                        });
                        Ok(Some(final_in))
                    }
                    ApprovalDecision::Deny { message } => {
                        let feedback =
                            message.unwrap_or_else(|| "Permission denied by user.".to_string());
                        let _ = event_tx.send(EngineEvent::ToolCallUpdate {
                            thread_id: state.id.clone(),
                            message_id: message_id.to_string(),
                            tool_use_id: tool_use_id.to_string(),
                            status: ToolStatusEvent::Error,
                            input: None,
                            result: None,
                            duration_ms: Some(0),
                            error_message: Some(feedback.clone()),
                        });
                        let (recovery, trace_entry) = {
                            let recovery = tool_failure_ledger.lock().record(
                                tool.name(),
                                &feedback,
                                active_todo_label(state),
                            );
                            let entry = ToolDispatchEntry {
                                tool_use_id: tool_use_id.to_string(),
                                tool_name: tool.name().to_string(),
                                status: ToolTraceStatus::Error,
                                duration_ms: Some(0),
                                category: Some(recovery.category),
                                subgoal: recovery.subgoal.clone(),
                            };
                            (recovery, entry)
                        };
                        tool_dispatch_trace.lock().record(trace_entry);
                        self.append_tool_result_msg(
                            state,
                            tool_use_id,
                            feedback,
                            true,
                            Some(recovery),
                        )
                        .await?;
                        Ok(None)
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolDispatchEntry {
    tool_use_id: String,
    tool_name: String,
    status: ToolTraceStatus,
    duration_ms: Option<u64>,
    category: Option<&'static str>,
    subgoal: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolTraceStatus {
    Success,
    Recoverable,
    Error,
    Aborted,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ToolDispatchSummary {
    total: usize,
    success: usize,
    recoverable: usize,
    error: usize,
    aborted: usize,
    total_duration_ms: u64,
}

#[derive(Debug, Default)]
struct ToolDispatchTrace {
    entries: Vec<ToolDispatchEntry>,
}

impl ToolDispatchTrace {
    fn record(&mut self, entry: ToolDispatchEntry) {
        self.entries.push(entry);
    }

    fn summary(&self) -> ToolDispatchSummary {
        let mut summary = ToolDispatchSummary {
            total: self.entries.len(),
            ..Default::default()
        };
        for entry in &self.entries {
            match entry.status {
                ToolTraceStatus::Success => summary.success += 1,
                ToolTraceStatus::Recoverable => summary.recoverable += 1,
                ToolTraceStatus::Error => summary.error += 1,
                ToolTraceStatus::Aborted => summary.aborted += 1,
            }
            summary.total_duration_ms += entry.duration_ms.unwrap_or(0);
        }
        summary
    }
}

impl AgentEngine {
    /// Persist turn dispatch trace + log structured summary.
    fn log_tool_dispatch_summary(
        &self,
        state: &Arc<ThreadState>,
        tool_dispatch_trace: &Arc<Mutex<ToolDispatchTrace>>,
        message_id: &str,
    ) {
        let trace = tool_dispatch_trace.lock();
        let summary = trace.summary();
        if summary.total == 0 {
            return;
        }
        let failed_categories = trace
            .entries
            .iter()
            .filter_map(|entry| entry.category)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        let active_subgoals = trace
            .entries
            .iter()
            .filter_map(|entry| entry.subgoal.as_deref())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" | ");
        info!(
            thread_id = %state.id,
            total = summary.total,
            success = summary.success,
            recoverable = summary.recoverable,
            error = summary.error,
            aborted = summary.aborted,
            total_duration_ms = summary.total_duration_ms,
            failed_categories = %failed_categories,
            active_subgoals = %active_subgoals,
            "tool dispatch trace summary"
        );

        // Persist to database for diagnostics / post-hoc analysis.
        // Best-effort: a write failure logs and continues.
        let repo = deepseek_state::ToolDispatchTraceRepo::new(self.db.as_ref());
        if let Err(e) = repo.insert(deepseek_state::ToolDispatchTraceInsert {
            thread_id: state.id.clone(),
            message_id: message_id.to_string(),
            total: summary.total,
            success: summary.success,
            recoverable: summary.recoverable,
            error: summary.error,
            aborted: summary.aborted,
            total_duration_ms: summary.total_duration_ms,
            categories: if failed_categories.is_empty() {
                None
            } else {
                Some(failed_categories)
            },
            subgoals: if active_subgoals.is_empty() {
                None
            } else {
                Some(active_subgoals)
            },
        }) {
            warn!(error = %e, thread_id = %state.id, "tool dispatch trace persist failed");
        }
    }
}

/// Turn-end diff summary (Codex-aligned turn_diff_tracker): logs which files
/// were created/modified/deleted this turn so every code-change turn has an
/// auditable record.
fn log_turn_diff_summary(state: &Arc<ThreadState>) {
    let diff = state.turn_diff.lock();
    if !diff.has_changes() {
        return;
    }
    info!(
        thread_id = %state.id,
        created = diff.created.len(),
        modified = diff.modified.len(),
        deleted = diff.deleted.len(),
        created_files = %diff.created.join(", "),
        modified_files = %diff.modified.join(", "),
        deleted_files = %diff.deleted.join(", "),
        "turn diff summary"
    );
    // Clear for the next turn.
    drop(diff);
    state.turn_diff.lock().clear();
}

/// Glue type: adapts the engine's per-thread `TurnDiffTracker` into the
/// trait object that `ToolContext` carries. Write/Edit tools call
/// `record_create` / `record_modify` / `record_delete` through this.
struct EngineTurnDiffRecorder {
    diff: Arc<Mutex<crate::thread::TurnDiffTracker>>,
}

impl deepseek_tools::context::TurnDiffRecorder for EngineTurnDiffRecorder {
    fn record_create(&self, path: &str) {
        let mut d = self.diff.lock();
        d.created.push(path.to_string());
    }

    fn record_modify(&self, path: &str) {
        let mut d = self.diff.lock();
        d.modified.push(path.to_string());
    }

    fn record_delete(&self, path: &str) {
        let mut d = self.diff.lock();
        d.deleted.push(path.to_string());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolFailureSnapshot {
    tool_name: String,
    category: &'static str,
    subgoal: Option<String>,
    total_failures: usize,
    same_tool_category_attempts: usize,
    failed_tools: Vec<String>,
}

#[derive(Debug, Default)]
struct ToolFailureLedger {
    total_failures: usize,
    by_tool_category: HashMap<(Option<String>, String, &'static str), usize>,
    failed_tools: BTreeSet<String>,
}

impl ToolFailureLedger {
    fn record(
        &mut self,
        tool_name: &str,
        error: &str,
        subgoal: Option<String>,
    ) -> ToolFailureSnapshot {
        let category = classify_tool_failure(error);
        self.total_failures += 1;
        self.failed_tools.insert(tool_name.to_string());
        let key = (subgoal.clone(), tool_name.to_string(), category);
        let same_tool_category_attempts = {
            let count = self.by_tool_category.entry(key).or_insert(0);
            *count += 1;
            *count
        };

        ToolFailureSnapshot {
            tool_name: tool_name.to_string(),
            category,
            subgoal,
            total_failures: self.total_failures,
            same_tool_category_attempts,
            failed_tools: self.failed_tools.iter().cloned().collect(),
        }
    }

    /// Check whether a tool call should be blocked BEFORE execution because
    /// the same (subgoal, tool, category) has already failed too many times.
    /// Returns the category string if blocked, `None` if the call can proceed.
    fn should_block(&self, tool_name: &str, subgoal: Option<&str>) -> Option<&'static str> {
        // Check every category tracked for this (subgoal, tool) pair.
        // Block threshold: >= 3 same-category attempts for the same subgoal.
        const BLOCK_THRESHOLD: usize = 3;
        for ((key_subgoal, key_tool, category), count) in &self.by_tool_category {
            if key_tool == tool_name
                && key_subgoal.as_deref() == subgoal
                && *count >= BLOCK_THRESHOLD
            {
                return Some(category);
            }
        }
        None
    }
}

fn active_todo_label(state: &ThreadState) -> Option<String> {
    let todos = state.todos.lock();
    todos
        .iter()
        .find(|todo| todo.status == deepseek_tools::todo::TodoStatus::InProgress)
        .map(|todo| {
            if todo.active_form.trim().is_empty() {
                todo.content.clone()
            } else {
                todo.active_form.clone()
            }
        })
}

fn shell_output_indicates_failure(result: &deepseek_tools::types::ToolResult) -> bool {
    result.tool_name == "run_command"
        && (
            // Prefer structured failure_stage when available (Codex-aligned).
            matches!(
                result.failure_stage,
                Some(
                    deepseek_tools::types::ShellFailureStage::NonZeroExit
                        | deepseek_tools::types::ShellFailureStage::PermissionDenied
                        | deepseek_tools::types::ShellFailureStage::SandboxDenied
                )
            ) || shell_exit_code(&result.content).is_some_and(|code| code != 0)
        )
}

fn changed_path_from_tool_input(tool_name: &str, input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "write_file"
        | "write_to_file"
        | "edit_file"
        | "replace_file_content"
        | "multi_replace_file_content" => input
            .get("path")
            .or_else(|| input.get("file_path"))
            .or_else(|| input.get("filePath"))
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn shell_exit_code(content: &str) -> Option<i32> {
    let first = content.lines().next()?.trim();
    first
        .strip_prefix("Exit code:")
        .and_then(|value| value.trim().parse::<i32>().ok())
}

fn format_tool_error_for_model(error: &str, recovery: Option<&ToolFailureSnapshot>) -> String {
    let category = recovery
        .map(|snapshot| snapshot.category)
        .unwrap_or_else(|| classify_tool_failure(error));
    let suggestion = tool_failure_recovery_hint(category);
    let mut out = format!(
        "<tool_use_error>\n错误: {error}\n分类: {category}\n恢复建议: {suggestion}\n</tool_use_error>"
    );

    if let Some(snapshot) = recovery {
        let subgoal = snapshot
            .subgoal
            .as_deref()
            .map(|value| format!("\n当前子目标: {value}"))
            .unwrap_or_default();
        out = format!(
            "<tool_use_error>\n错误: {error}\n分类: {category}{subgoal}\n本轮失败次数: {}\n当前子目标同类重复: {} 次（工具: {}）\n本轮失败工具: {}\n恢复建议: {suggestion}\n{}\n</tool_use_error>",
            snapshot.total_failures,
            snapshot.same_tool_category_attempts,
            snapshot.tool_name,
            snapshot.failed_tools.join(", "),
            tool_failure_escalation_hint(snapshot),
        );
    }

    out
}

fn format_tool_recovery_context_for_model(content: &str, snapshot: &ToolFailureSnapshot) -> String {
    let subgoal = snapshot
        .subgoal
        .as_deref()
        .map(|value| format!("\n当前子目标: {value}"))
        .unwrap_or_default();
    format!(
        "{content}\n<tool_recovery_context>\n分类: {}{subgoal}\n本轮失败次数: {}\n当前子目标同类重复: {} 次（工具: {}）\n本轮失败工具: {}\n恢复建议: {}\n{}\n</tool_recovery_context>",
        snapshot.category,
        snapshot.total_failures,
        snapshot.same_tool_category_attempts,
        snapshot.tool_name,
        snapshot.failed_tools.join(", "),
        tool_failure_recovery_hint(snapshot.category),
        tool_failure_escalation_hint(snapshot),
    )
}

fn tool_failure_escalation_hint(snapshot: &ToolFailureSnapshot) -> &'static str {
    if snapshot.same_tool_category_attempts >= 3 {
        "升级要求: 这类失败已经重复三次。停止重复同一工具和同一思路，先换工具或缩小目标；如果仍无法推进，再向用户说明三次尝试。"
    } else if snapshot.same_tool_category_attempts >= 2 {
        "升级要求: 这类失败已经重复。下一步必须换一种实质不同的方法，不要只微调参数后重试。"
    } else if snapshot.total_failures >= 3 {
        "升级要求: 本轮已有多次工具失败。先总结失败模式，再选择更保守、更小范围的验证路径。"
    } else {
        "升级要求: 继续推进，但下一步必须体现恢复建议，避免无信息重复。"
    }
}

fn classify_tool_failure(error: &str) -> &'static str {
    let e = error.to_lowercase();
    if e.contains("invalid tool arguments")
        || e.contains("invalid arguments")
        || e.contains("valid json")
        || e.contains("json")
    {
        "invalid_arguments"
    } else if e.contains("unknown tool") || e.contains("tool not found") {
        "unknown_tool"
    } else if e.contains("sandbox denied")
        || e.contains("sandbox_denied")
        || e.contains("sandbox blocked")
        || e.contains("沙箱")
    {
        "sandbox_denied"
    } else if e.contains("access is denied")
        || e.contains("permission denied")
        || e.contains("permissiondenied")
        || e.contains("denied by user")
        || e.contains("用户拒绝")
        || e.contains("权限")
        || e.contains("eacces")
    {
        "permission_denied"
    } else if e.contains("no such file")
        || e.contains("file not found")
        || e.contains("path not found")
        || e.contains("cannot find path")
        || e.contains("找不到")
        || e.contains("不存在")
    {
        "path_not_found"
    } else if e.contains("command not found")
        || e.contains("not recognized")
        || e.contains("无法识别")
        || e.contains("不是内部或外部命令")
        || e.contains("不是内部命令")
    {
        "command_not_found"
    } else if e.contains("timed out") || e.contains("timeout") || e.contains("超时") {
        "timeout"
    } else if e.contains("network")
        || e.contains("connection")
        || e.contains("502")
        || e.contains("503")
        || e.contains("tls")
    {
        "network"
    } else if e.contains("syntax error")
        || e.contains("parsererror")
        || e.contains("parse error")
        || e.contains("语法")
    {
        "syntax_error"
    } else if e.contains("type error")
        || e.contains("typeerror")
        || e.contains("tsc")
        || e.contains("类型")
    {
        "type_error"
    } else if e.contains("test failed")
        || e.contains("tests failed")
        || e.contains("test failure")
        || e.contains("assertion")
        || e.contains("测试")
    {
        "test_failure"
    } else {
        "unknown"
    }
}

fn tool_failure_recovery_hint(category: &str) -> &'static str {
    match category {
        "invalid_arguments" => "不要重复同一个调用。检查工具 schema，修正 JSON 字段和值后再调用。",
        "unknown_tool" => {
            "不要调用不存在的工具。改用当前可用工具；如果需要相邻能力，先搜索或读取可用工具说明。"
        }
        "sandbox_denied" => {
            "沙箱已拦截此操作。不要重试同一条命令；改用项目内允许的工具链，或请求用户调整沙箱策略。"
        }
        "permission_denied" => {
            "不要硬闯同一路径。换用只读或更小范围的安全操作；确实需要权限时说明原因并通过 ask_user_question 请求用户介入，说明具体需要什么权限和为什么需要。"
        }
        "path_not_found" => "先列出父目录或搜索相似文件名，确认真实路径后再继续。",
        "command_not_found" => "先检查项目脚本、依赖和系统可用命令；换用等价命令或本地语言 API。",
        "timeout" => "缩小范围后重试，例如单文件、单测试、增加过滤条件；不要直接重复完整长命令。",
        "network" => "可短暂重试一次；仍失败时使用本地缓存、官方配置或让用户确认网络/API 状态。",
        "syntax_error" => "读取具体报错位置，修正语法后运行最小验证。",
        "type_error" => "读取类型定义和调用点，修正类型契约后运行最小类型检查。",
        "test_failure" => "定位第一个失败断言，先修根因，再运行相关最小测试。",
        _ => "分析 stderr/stdout，提出一个实质不同的替代方法；不要无信息地重复失败调用。",
    }
}

/// Assembles streaming `tool_call_delta` chunks into complete `ToolCall` objects.
///
/// The DeepSeek SSE protocol streams tool calls in pieces:
/// - First chunk for a new call: `{ index, id, name, arguments_delta? }`
/// - Subsequent chunks for same index: `{ index, arguments_delta }`
/// - Multiple parallel tool calls each have a different `index`
///
/// We accumulate by index; when the stream ends, we emit all collected calls.
#[derive(Debug, Default)]
struct ToolCallAssembler {
    calls: Vec<PartialToolCall>,
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

impl ToolCallAssembler {
    /// Returns true if this delta introduces a NEW tool call index (first
    /// time we see it). Used to emit ToolCallStart early during streaming.
    fn is_new_call(&self, delta: &deepseek_client::types::ToolCallDelta) -> bool {
        delta.index >= self.calls.len()
            || (self.calls[delta.index].id.is_empty() && delta.id.is_some())
            || (self.calls[delta.index].name.is_empty() && delta.name.is_some())
    }

    fn feed(&mut self, delta: &deepseek_client::types::ToolCallDelta) {
        while self.calls.len() <= delta.index {
            self.calls.push(PartialToolCall::default());
        }
        let entry = &mut self.calls[delta.index];
        if let Some(id) = &delta.id {
            entry.id = id.clone();
        }
        if let Some(name) = &delta.name {
            entry.name = name.clone();
        }
        if let Some(args) = &delta.arguments_delta {
            entry.arguments.push_str(args);
        }
    }

    fn partial(&self, index: usize) -> Option<(&String, &String, &String)> {
        self.calls
            .get(index)
            .map(|p| (&p.id, &p.name, &p.arguments))
    }

    fn finish(self) -> Vec<ApiToolCall> {
        self.calls
            .into_iter()
            .filter(|p| !p.id.is_empty() && !p.name.is_empty())
            .map(|p| ApiToolCall {
                id: p.id,
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: p.name,
                    arguments: if p.arguments.is_empty() {
                        "{}".to_string()
                    } else {
                        p.arguments
                    },
                },
            })
            .collect()
    }
}

fn partial_tool_input(tool_name: &str, arguments: &str) -> serde_json::Value {
    if arguments.trim().is_empty() {
        return serde_json::json!({});
    }
    if let Ok(v) = crate::repair::parse_tool_args(arguments) {
        return v;
    }

    let fields: &[&str] = match tool_name {
        "write_file" | "write_to_file" => &["path", "content", "CodeContent"],
        "edit_file" | "replace_file_content" => &[
            "path",
            "old_string",
            "new_string",
            "TargetContent",
            "ReplacementContent",
        ],
        "multi_replace_file_content" => &[
            "path",
            "old_string",
            "new_string",
            "TargetContent",
            "ReplacementContent",
        ],
        _ => &["path", "content", "old_string", "new_string"],
    };

    let mut obj = serde_json::Map::new();
    for field in fields {
        if let Some(value) = extract_partial_json_string_field(arguments, field) {
            obj.insert((*field).to_string(), serde_json::Value::String(value));
        }
    }
    serde_json::Value::Object(obj)
}

fn extract_partial_json_string_field(source: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let key_start = source.find(&key)?;
    let after_key = &source[(key_start + key.len())..];
    let colon = after_key.find(':')?;
    let mut value = after_key[(colon + 1)..].trim_start().chars().peekable();
    if value.next()? != '"' {
        return None;
    }

    let mut out = String::new();
    while let Some(ch) = value.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match value.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{0008}'),
                Some('f') => out.push('\u{000C}'),
                Some('u') => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        match value.next() {
                            Some(c) => hex.push(c),
                            None => return Some(out),
                        }
                    }
                    if let Ok(code) = u16::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(code as u32) {
                            out.push(c);
                        }
                    }
                }
                Some(other) => out.push(other),
                None => return Some(out),
            },
            other => out.push(other),
        }
    }
    Some(out)
}

/// Heal message sequence violations that would cause DeepSeek API 400 errors.
///
/// The DeepSeek/OpenAI API requires:
/// 1. Every assistant message with `tool_calls` must be immediately followed
///    by `tool` role messages for each tool_call_id.
/// 2. Every `tool` role message must be preceded by an assistant message
///    with a matching `tool_calls` entry.
///
/// This function removes violations in both directions:
/// - Orphaned assistant messages (tool_calls with no matching tool results)
/// - Orphaned tool messages (no preceding assistant with matching tool_calls)
/// - Empty assistant messages (no content, no reasoning, no tools)
#[allow(clippy::needless_range_loop)] // index-based: passes compare across positions + consult `to_remove`
fn heal_message_sequence(msgs: &mut Vec<ChatMessage>) {
    if msgs.is_empty() {
        return;
    }

    let mut to_remove: Vec<usize> = Vec::new();

    // Pass 1: Find orphaned assistant messages (tool_calls with missing results)
    for i in 0..msgs.len() {
        let msg = &msgs[i];
        if msg.role == "assistant" {
            if let Some(ref tool_calls) = msg.tool_calls {
                if !tool_calls.is_empty() {
                    let expected_ids: Vec<&str> =
                        tool_calls.iter().map(|tc| tc.id.as_str()).collect();
                    let mut found_ids: Vec<&str> = Vec::new();
                    for j in (i + 1)..msgs.len() {
                        if msgs[j].role == "tool" {
                            if let Some(ref tcid) = msgs[j].tool_call_id {
                                if expected_ids.contains(&tcid.as_str()) {
                                    found_ids.push(tcid.as_str());
                                }
                            }
                        } else if msgs[j].role == "assistant" || msgs[j].role == "user" {
                            break;
                        }
                    }
                    if found_ids.len() < expected_ids.len() {
                        tracing::warn!(
                            index = i,
                            expected = expected_ids.len(),
                            found = found_ids.len(),
                            "healing: removing orphaned assistant message with unmatched tool_calls"
                        );
                        to_remove.push(i);
                    }
                }
            }
        }
    }

    // Pass 2: Find orphaned tool messages (no preceding assistant with matching tool_calls)
    for i in 0..msgs.len() {
        if to_remove.contains(&i) {
            continue;
        }
        let msg = &msgs[i];
        if msg.role == "tool" {
            let tool_call_id = match &msg.tool_call_id {
                Some(id) => id.as_str(),
                None => {
                    to_remove.push(i);
                    continue;
                }
            };
            // Search backwards for a matching assistant with this tool_call_id
            let mut found_parent = false;
            for j in (0..i).rev() {
                if to_remove.contains(&j) {
                    continue;
                }
                let prev = &msgs[j];
                if prev.role == "assistant" {
                    if let Some(ref tcs) = prev.tool_calls {
                        if tcs.iter().any(|tc| tc.id == tool_call_id) {
                            found_parent = true;
                            break;
                        }
                    }
                    // Hit an assistant without matching tool_calls — stop
                    break;
                }
                if prev.role == "user" {
                    break;
                }
            }
            if !found_parent {
                tracing::warn!(
                    index = i,
                    tool_call_id = tool_call_id,
                    "healing: removing orphaned tool message with no matching assistant"
                );
                to_remove.push(i);
            }
        }
    }

    // Pass 3: Remove empty assistant messages
    for i in 0..msgs.len() {
        if to_remove.contains(&i) {
            continue;
        }
        let msg = &msgs[i];
        if msg.role == "assistant"
            && msg.content_text().unwrap_or("").is_empty()
            && msg.reasoning_content.as_deref().unwrap_or("").is_empty()
            && msg.tool_calls.as_ref().is_none_or(|tc| tc.is_empty())
        {
            tracing::warn!(index = i, "healing: removing empty assistant message");
            to_remove.push(i);
        }
    }

    if !to_remove.is_empty() {
        to_remove.sort_unstable();
        to_remove.dedup();
        tracing::warn!(
            count = to_remove.len(),
            "healing: removing invalid messages from sequence"
        );
        for &idx in to_remove.iter().rev() {
            msgs.remove(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_client::types::ToolCallDelta;

    use crate::gate::{ApprovalDecision, ApprovalRequest, GateError, PermissionGate};
    use async_trait::async_trait;

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

    #[test]
    fn turn_chat_opts_deepseek_maps_effort() {
        let low = turn_chat_opts(vec![], ProviderId::Deepseek, "deepseek", "low");
        assert_eq!(
            low.extra_body.unwrap().thinking.unwrap().thinking_type,
            "enabled"
        );
        assert_eq!(low.reasoning_effort.as_deref(), Some("high"));

        let ultra = turn_chat_opts(vec![], ProviderId::Deepseek, "deepseek", "ultra");
        assert_eq!(ultra.reasoning_effort.as_deref(), Some("max"));
    }

    #[test]
    fn turn_chat_opts_other_provider_is_plain() {
        let opts = turn_chat_opts(vec![], ProviderId::Other, "openai", "ultra");
        assert!(opts.extra_body.is_none());
        assert!(opts.thinking.is_none());
        assert!(opts.reasoning_effort.is_none());
    }

    #[test]
    fn tool_error_feedback_includes_recovery_hint() {
        let msg = format_tool_error_for_model("No such file or directory: src/missing.rs", None);
        assert!(msg.contains("<tool_use_error>"));
        assert!(msg.contains("path_not_found"));
        assert!(msg.contains("先列出父目录"));
    }

    #[test]
    fn tool_failure_ledger_marks_repeated_failures() {
        let mut ledger = ToolFailureLedger::default();
        ledger.record(
            "read_file",
            "No such file or directory: src/a.rs",
            Some("Reading files".into()),
        );
        let snapshot = ledger.record(
            "read_file",
            "No such file or directory: src/b.rs",
            Some("Reading files".into()),
        );
        let msg =
            format_tool_error_for_model("No such file or directory: src/b.rs", Some(&snapshot));
        assert!(msg.contains("本轮失败次数: 2"));
        assert!(msg.contains("当前子目标: Reading files"));
        assert!(msg.contains("当前子目标同类重复: 2 次"));
        assert!(msg.contains("必须换一种实质不同的方法"));
    }

    #[test]
    fn tool_failure_ledger_separates_subgoals() {
        let mut ledger = ToolFailureLedger::default();
        ledger.record(
            "read_file",
            "No such file or directory: src/a.rs",
            Some("Reading files".into()),
        );
        let snapshot = ledger.record(
            "read_file",
            "No such file or directory: src/b.rs",
            Some("Running tests".into()),
        );
        assert_eq!(snapshot.total_failures, 2);
        assert_eq!(snapshot.same_tool_category_attempts, 1);
        assert_eq!(snapshot.subgoal.as_deref(), Some("Running tests"));
    }

    #[test]
    fn tool_dispatch_trace_summarizes_statuses() {
        let mut trace = ToolDispatchTrace::default();
        for (idx, status) in [
            ToolTraceStatus::Success,
            ToolTraceStatus::Recoverable,
            ToolTraceStatus::Error,
            ToolTraceStatus::Aborted,
        ]
        .into_iter()
        .enumerate()
        {
            trace.record(ToolDispatchEntry {
                tool_use_id: format!("call_{idx}"),
                tool_name: "run_command".into(),
                status,
                duration_ms: Some(10),
                category: None,
                subgoal: Some("Running checks".into()),
            });
        }

        let summary = trace.summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.success, 1);
        assert_eq!(summary.recoverable, 1);
        assert_eq!(summary.error, 1);
        assert_eq!(summary.aborted, 1);
        assert_eq!(summary.total_duration_ms, 40);
    }

    #[test]
    fn shell_nonzero_exit_is_recoverable_signal() {
        let result = deepseek_tools::types::ToolResult {
            tool_use_id: "call_1".into(),
            tool_name: "run_command".into(),
            is_error: false,
            content: "Exit code: 1\n--- stderr ---\nrg: command not found".into(),
            duration_ms: 10,
            exit_code: Some(1),
            failure_stage: Some(deepseek_tools::types::ShellFailureStage::NonZeroExit),
        };
        assert!(shell_output_indicates_failure(&result));
    }

    #[test]
    fn shell_recovery_context_does_not_mark_tool_error() {
        let mut ledger = ToolFailureLedger::default();
        let content = "Exit code: 1\n--- stderr ---\nrg: command not found";
        let snapshot = ledger.record("run_command", content, Some("Searching code".into()));
        let msg = format_tool_recovery_context_for_model(content, &snapshot);
        assert!(msg.contains("<tool_recovery_context>"));
        assert!(msg.contains("当前子目标: Searching code"));
        assert!(msg.contains("command_not_found"));
        assert!(!msg.contains("<tool_use_error>"));
    }

    #[test]
    fn tool_failure_ledger_blocks_repeated_same_call() {
        let mut ledger = ToolFailureLedger::default();
        let subgoal = Some("Reading files".to_string());
        // Simulate 3 path_not_found failures for the same (subgoal, tool).
        ledger.record("read_file", "No such file: src/a.rs", subgoal.clone());
        ledger.record("read_file", "No such file: src/b.rs", subgoal.clone());
        ledger.record("read_file", "No such file: src/c.rs", subgoal.clone());
        // Should block: 3 same-category failures for same subgoal+tool.
        assert!(
            ledger
                .should_block("read_file", subgoal.as_deref())
                .is_some(),
            "should block after 3 same-category failures"
        );
        // Different tool should NOT be blocked.
        assert!(
            ledger.should_block("grep", subgoal.as_deref()).is_none(),
            "different tool should not be blocked"
        );
        // Same tool but different subgoal should NOT be blocked.
        assert!(
            ledger
                .should_block("read_file", Some("Running tests"))
                .is_none(),
            "different subgoal should not be blocked"
        );
    }

    #[test]
    fn tool_failure_ledger_blocks_at_threshold_not_before() {
        let mut ledger = ToolFailureLedger::default();
        let subgoal = Some("Searching code".to_string());
        // 1 failure: not blocked.
        ledger.record("grep", "rg: command not found", subgoal.clone());
        assert!(ledger.should_block("grep", subgoal.as_deref()).is_none());
        // 2 failures: still not blocked.
        ledger.record("grep", "rg: command not found", subgoal.clone());
        assert!(ledger.should_block("grep", subgoal.as_deref()).is_none());
        // 3 failures: now blocked.
        ledger.record("grep", "rg: command not found", subgoal.clone());
        assert!(ledger.should_block("grep", subgoal.as_deref()).is_some());
    }

    #[test]
    fn tool_error_classifier_detects_command_not_found() {
        assert_eq!(
            classify_tool_failure("'rg' is not recognized as an internal command"),
            "command_not_found"
        );
    }

    #[test]
    fn shell_classifier_matrix_windows_errors() {
        // command_not_found
        assert_eq!(
            classify_tool_failure("'xyz' is not recognized as an internal or external command"),
            "command_not_found"
        );
        assert_eq!(
            classify_tool_failure("The term 'xyz' is not recognized"),
            "command_not_found"
        );
        assert_eq!(
            classify_tool_failure("不是内部或外部命令"),
            "command_not_found"
        );
        // path_not_found
        assert_eq!(
            classify_tool_failure("Cannot find path 'D:\\missing' because it does not exist"),
            "path_not_found"
        );
        assert_eq!(
            classify_tool_failure("No such file or directory"),
            "path_not_found"
        );
        assert_eq!(classify_tool_failure("找不到文件"), "path_not_found");
        // permission_denied
        assert_eq!(
            classify_tool_failure("Access is denied"),
            "permission_denied"
        );
        assert_eq!(
            classify_tool_failure("PermissionDenied"),
            "permission_denied"
        );
        assert_eq!(classify_tool_failure("EACCES"), "permission_denied");
        // timeout
        assert_eq!(
            classify_tool_failure("timed out after 120 seconds"),
            "timeout"
        );
        assert_eq!(classify_tool_failure("operation timed out"), "timeout");
        // network
        assert_eq!(classify_tool_failure("502 Bad Gateway"), "network");
        assert_eq!(classify_tool_failure("Connection refused"), "network");
        assert_eq!(classify_tool_failure("TLS handshake failed"), "network");
        // syntax_error
        assert_eq!(
            classify_tool_failure("ParserError: unexpected token"),
            "syntax_error"
        );
        assert_eq!(classify_tool_failure("语法错误"), "syntax_error");
        // type_error
        assert_eq!(
            classify_tool_failure("TypeError: undefined is not a function"),
            "type_error"
        );
        assert_eq!(classify_tool_failure("类型不匹配"), "type_error");
        // test_failure
        assert_eq!(
            classify_tool_failure("assertion failed: expected true, got false"),
            "test_failure"
        );
        assert_eq!(classify_tool_failure("1 tests failed"), "test_failure");
        // invalid_arguments
        assert_eq!(
            classify_tool_failure("Invalid tool arguments"),
            "invalid_arguments"
        );
        assert_eq!(classify_tool_failure("not valid JSON"), "invalid_arguments");
    }

    #[test]
    fn shell_classifier_multiline_error() {
        let multi = "Error: something went wrong\nCannot find path 'D:\\test\\foo.txt' because it does not exist\nat line 42";
        assert_eq!(classify_tool_failure(multi), "path_not_found");
    }

    #[test]
    fn shell_pipeline_nonzero_exit_to_recovery_hint() {
        // Full pipeline: nonzero exit → recovery → hint should be actionable.
        let result = deepseek_tools::types::ToolResult {
            tool_use_id: "call_x".into(),
            tool_name: "run_command".into(),
            is_error: false,
            content: "Exit code: 1\n--- stderr ---\n'rg' is not recognized as an internal command"
                .into(),
            duration_ms: 100,
            exit_code: Some(1),
            failure_stage: Some(deepseek_tools::types::ShellFailureStage::NonZeroExit),
        };
        assert!(shell_output_indicates_failure(&result));
        let category = classify_tool_failure(&result.content);
        assert_eq!(category, "command_not_found");
        let hint = tool_failure_recovery_hint(category);
        assert!(!hint.is_empty());
        assert!(
            hint.contains("等价命令") || hint.contains("本地"),
            "hint should suggest alternatives: {hint}"
        );
    }

    fn api_call(id: &str, name: &str) -> ApiToolCall {
        ApiToolCall {
            id: id.into(),
            call_type: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: "{}".into(),
            },
        }
    }

    /// Assistant message carrying the given tool_calls (by id).
    fn assistant_with_calls(content: &str, call_ids: &[&str]) -> ChatMessage {
        let mut m = ChatMessage::assistant(content);
        m.tool_calls = Some(call_ids.iter().map(|id| api_call(id, "x")).collect());
        m
    }

    /// Tool-result message paired to a tool_call id.
    fn tool_result(id: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".into(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(id.into()),
        }
    }

    fn roles(msgs: &[ChatMessage]) -> Vec<&str> {
        msgs.iter().map(|m| m.role.as_str()).collect()
    }

    #[test]
    fn heal_keeps_valid_sequence() {
        let mut msgs = vec![
            ChatMessage::user("hi"),
            assistant_with_calls("", &["c1"]),
            tool_result("c1", "ok"),
            ChatMessage::assistant("done"),
        ];
        heal_message_sequence(&mut msgs);
        assert_eq!(roles(&msgs), vec!["user", "assistant", "tool", "assistant"]);
    }

    #[test]
    fn heal_removes_orphan_assistant_with_unmatched_tool_calls() {
        // assistant declares c1 but no tool result follows → orphaned.
        let mut msgs = vec![
            ChatMessage::user("hi"),
            assistant_with_calls("", &["c1"]),
            ChatMessage::user("next"),
        ];
        heal_message_sequence(&mut msgs);
        assert_eq!(roles(&msgs), vec!["user", "user"]);
    }

    #[test]
    fn heal_removes_orphan_tool_without_parent() {
        // tool result with no preceding assistant declaring its id.
        let mut msgs = vec![
            ChatMessage::user("hi"),
            tool_result("stray", "orphan"),
            ChatMessage::assistant("sure"),
        ];
        heal_message_sequence(&mut msgs);
        assert_eq!(roles(&msgs), vec!["user", "assistant"]);
    }

    #[test]
    fn heal_removes_empty_assistant() {
        let mut msgs = vec![
            ChatMessage::user("hi"),
            ChatMessage::assistant(""), // empty, no content/reasoning/tools
            ChatMessage::user("again"),
        ];
        heal_message_sequence(&mut msgs);
        assert_eq!(roles(&msgs), vec!["user", "user"]);
    }

    #[test]
    fn heal_partial_tool_results_removes_assistant() {
        // assistant declares c1+c2 but only c1 has a result → assistant orphaned,
        // and c1's result then becomes an orphan tool (parent removed).
        let mut msgs = vec![
            ChatMessage::user("hi"),
            assistant_with_calls("", &["c1", "c2"]),
            tool_result("c1", "partial"),
            ChatMessage::assistant("trailing"),
        ];
        heal_message_sequence(&mut msgs);
        // The orphaned assistant + its now-parentless tool result are removed.
        assert_eq!(roles(&msgs), vec!["user", "assistant"]);
    }

    #[test]
    fn heal_empty_input_is_noop() {
        let mut msgs: Vec<ChatMessage> = vec![];
        heal_message_sequence(&mut msgs);
        assert!(msgs.is_empty());
    }

    /// finalize_aborted_tool_calls must append a tool-result message for each
    /// UNFINISHED tool_call (keeping the assistant{tool_calls}→tool sequence
    /// valid) and skip the ones already done.
    #[tokio::test]
    async fn finalize_appends_results_for_unfinished_only() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
        let client = DeepSeekClient::new(deepseek_client::deepseek::DeepSeekClientConfig {
            api_key: "placeholder".into(),
            base_url: "https://api.deepseek.com".into(),
            ..Default::default()
        })
        .unwrap();
        let tools = Arc::new(ToolRegistry::new());
        let engine = AgentEngine::new(
            client,
            "system".into(),
            tools,
            Arc::new(AllowAllGate),
            db.clone(),
        );
        let state = engine.create_thread(None, None).unwrap();

        let assembled = vec![api_call("call_a", "read_file"), api_call("call_b", "grep")];
        let mut done = std::collections::HashSet::new();
        done.insert("call_a".to_string()); // a finished; b did not

        let (tx, mut rx) = mpsc::unbounded_channel::<EngineEvent>();
        let trace = Arc::new(Mutex::new(ToolDispatchTrace::default()));
        engine
            .finalize_aborted_tool_calls(
                &state,
                &assembled,
                &done,
                "msg_1",
                Arc::clone(&trace),
                &tx,
            )
            .await
            .unwrap();

        // Only call_b gets an aborted tool-result message in the log.
        let log = state.log.read();
        let tool_msgs: Vec<_> = log.messages().iter().filter(|m| m.role == "tool").collect();
        assert_eq!(tool_msgs.len(), 1, "only the unfinished call gets a result");
        assert_eq!(tool_msgs[0].tool_call_id.as_deref(), Some("call_b"));
        assert!(tool_msgs[0]
            .content
            .as_deref()
            .unwrap_or("")
            .contains("已中止"));

        // An Aborted ToolCallUpdate was emitted for call_b.
        let mut saw_aborted_b = false;
        while let Ok(ev) = rx.try_recv() {
            if let EngineEvent::ToolCallUpdate {
                tool_use_id,
                status: ToolStatusEvent::Aborted,
                ..
            } = ev
            {
                if tool_use_id == "call_b" {
                    saw_aborted_b = true;
                }
            }
        }
        assert!(saw_aborted_b, "expected Aborted update for call_b");
        let summary = trace.lock().summary();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.aborted, 1);
    }

    #[test]
    fn partial_tool_input_extracts_streaming_write_content() {
        let input = partial_tool_input(
            "write_file",
            r#"{"path":"src/main.rs","content":"fn main() {\n    println!(\"hi\");"#,
        );
        assert_eq!(input["path"], "src/main.rs");
        assert_eq!(input["content"], "fn main() {\n    println!(\"hi\");");
    }

    #[test]
    fn partial_tool_input_extracts_streaming_edit_content() {
        let input = partial_tool_input(
            "edit_file",
            r#"{"path":"src/main.rs","old_string":"old\nline","new_string":"new\nline"#,
        );
        assert_eq!(input["old_string"], "old\nline");
        assert_eq!(input["new_string"], "new\nline");
    }

    #[test]
    fn test_assembler_single_complete_call() {
        let mut a = ToolCallAssembler::default();
        a.feed(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: Some("read_file".to_string()),
            arguments_delta: Some("{\"path\":".to_string()),
        });
        a.feed(&ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments_delta: Some("\"/tmp\"}".to_string()),
        });
        let calls = a.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, "{\"path\":\"/tmp\"}");
    }

    #[test]
    fn test_assembler_multiple_parallel_calls() {
        let mut a = ToolCallAssembler::default();
        a.feed(&ToolCallDelta {
            index: 0,
            id: Some("c1".to_string()),
            name: Some("a".to_string()),
            arguments_delta: Some("{".to_string()),
        });
        a.feed(&ToolCallDelta {
            index: 1,
            id: Some("c2".to_string()),
            name: Some("b".to_string()),
            arguments_delta: Some("{".to_string()),
        });
        a.feed(&ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments_delta: Some("\"x\":1}".to_string()),
        });
        a.feed(&ToolCallDelta {
            index: 1,
            id: None,
            name: None,
            arguments_delta: Some("\"y\":2}".to_string()),
        });
        let calls = a.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "c1");
        assert_eq!(calls[0].function.arguments, "{\"x\":1}");
        assert_eq!(calls[1].id, "c2");
        assert_eq!(calls[1].function.arguments, "{\"y\":2}");
    }

    #[test]
    fn test_assembler_skips_incomplete_calls() {
        let mut a = ToolCallAssembler::default();
        a.feed(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: None,
            arguments_delta: Some("{}".to_string()),
        });
        let calls = a.finish();
        assert_eq!(calls.len(), 0, "incomplete call should be filtered");
    }

    #[test]
    fn test_assembler_empty_arguments_defaults_to_empty_object() {
        let mut a = ToolCallAssembler::default();
        a.feed(&ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            name: Some("ping".to_string()),
            arguments_delta: None,
        });
        let calls = a.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments, "{}");
    }

    // ── Concurrent batch sibling abort regression ─────────────────────

    /// Two concurrent tools, one succeeds and one fails — the
    /// failure must cancel the sibling abort token.
    #[tokio::test]
    async fn concurrent_batch_sibling_abort_on_tool_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
        let client = DeepSeekClient::new(deepseek_client::deepseek::DeepSeekClientConfig {
            api_key: "placeholder".into(),
            base_url: "https://api.deepseek.com".into(),
            ..Default::default()
        })
        .unwrap();

        // Register a tool that always errors.
        let tools = Arc::new(ToolRegistry::new());
        tools.register(Arc::new(AlwaysErrorTool));
        tools.register(Arc::new(SlowReadTool));

        let engine = AgentEngine::new(
            client,
            "system".into(),
            tools,
            Arc::new(AllowAllGate),
            db.clone(),
        );
        let state = engine.create_thread(None, None).unwrap();
        let (tx, _rx) = mpsc::unbounded_channel::<EngineEvent>();

        let trace = Arc::new(Mutex::new(ToolDispatchTrace::default()));
        let ledger = Arc::new(Mutex::new(ToolFailureLedger::default()));

        let sibling = CancellationToken::new();
        let main_abort = CancellationToken::new();
        let t1 = api_call("call_err", "always_error");
        let t2 = api_call("call_slow", "slow_read");

        // Run both concurrently.
        let futures: Vec<_> = [(&t1, &sibling), (&t2, &sibling)]
            .iter()
            .map(|(c, sab)| {
                engine.execute_one_tool_call(
                    c,
                    &state,
                    &main_abort,
                    "msg_1",
                    0,
                    Arc::clone(&ledger),
                    Arc::clone(&trace),
                    &tx,
                    (*sab).clone(),
                )
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        for r in results {
            assert!(r.is_ok(), "all paths return Ok(()) (recoverable)");
        }

        // Both tool results must exist in the log.
        let log = state.log.read();
        let tool_ids: Vec<_> = log
            .messages()
            .iter()
            .filter(|m| m.role == "tool")
            .map(|m| m.tool_call_id.as_deref().unwrap_or("").to_string())
            .collect();
        assert!(
            tool_ids.contains(&"call_err".to_string()),
            "failing tool must have result"
        );
        assert!(
            tool_ids.contains(&"call_slow".to_string()),
            "sibling must have a result (either synthetic abort or completed)"
        );
    }

    /// Three concurrent tools, all succeed — no sibling abort should fire.
    #[tokio::test]
    async fn concurrent_batch_all_succeed_no_sibling_abort() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(tmp.path().join("state.db")).unwrap());
        let client = DeepSeekClient::new(deepseek_client::deepseek::DeepSeekClientConfig {
            api_key: "placeholder".into(),
            base_url: "https://api.deepseek.com".into(),
            ..Default::default()
        })
        .unwrap();

        let tools = Arc::new(ToolRegistry::new());
        tools.register(Arc::new(AlwaysOkTool));
        tools.register(Arc::new(AlwaysOkTool));

        let engine = AgentEngine::new(
            client,
            "system".into(),
            tools,
            Arc::new(AllowAllGate),
            db.clone(),
        );
        let state = engine.create_thread(None, None).unwrap();
        let (tx, _rx) = mpsc::unbounded_channel::<EngineEvent>();

        let trace = Arc::new(Mutex::new(ToolDispatchTrace::default()));
        let ledger = Arc::new(Mutex::new(ToolFailureLedger::default()));
        let sab = CancellationToken::new();

        let main_abort2 = CancellationToken::new();
        let t1 = api_call("call_a", "always_ok");
        let t2 = api_call("call_b", "always_ok");

        let futures: Vec<_> = [(&t1, &sab), (&t2, &sab)]
            .iter()
            .map(|(c, s)| {
                engine.execute_one_tool_call(
                    c,
                    &state,
                    &main_abort2,
                    "msg_2",
                    0,
                    Arc::clone(&ledger),
                    Arc::clone(&trace),
                    &tx,
                    (*s).clone(),
                )
            })
            .collect();
        let results = futures::future::join_all(futures).await;
        assert_eq!(results.len(), 2);
        for r in results {
            assert!(r.is_ok());
        }

        // No abort should have been triggered.
        let log = state.log.read();
        let tool_msgs: Vec<_> = log.messages().iter().filter(|m| m.role == "tool").collect();
        assert_eq!(tool_msgs.len(), 2);
        for msg in &tool_msgs {
            assert!(
                !msg.content_text().unwrap_or("").contains("sibling"),
                "no sibling abort in success case"
            );
        }
    }

    // ── Test tool implementations ────────────────────────────────────

    use std::time::Duration;

    use deepseek_tools::types::ToolError;
    use deepseek_tools::{Tool, ToolContext};
    use serde_json::Value;

    struct AlwaysErrorTool;

    #[async_trait]
    impl Tool for AlwaysErrorTool {
        fn name(&self) -> &str {
            "always_error"
        }
        fn is_read_only(&self) -> bool {
            true
        }
        fn timeout(&self) -> Duration {
            Duration::from_secs(1)
        }
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
            Err(ToolError::ExecutionFailed("always fails".into()))
        }
    }

    struct SlowReadTool;

    #[async_trait]
    impl Tool for SlowReadTool {
        fn name(&self) -> &str {
            "slow_read"
        }
        fn is_read_only(&self) -> bool {
            true
        }
        fn timeout(&self) -> Duration {
            Duration::from_millis(2000)
        }
        async fn execute(&self, _args: Value, ctx: &ToolContext) -> Result<String, ToolError> {
            // Sleep briefly so sibling abort has time to fire if applicable.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => {},
                _ = ctx.abort.cancelled() => {},
            }
            if ctx.abort.is_cancelled() {
                return Err(ToolError::Aborted);
            }
            Ok("slow ok".into())
        }
    }

    struct AlwaysOkTool;

    #[async_trait]
    impl Tool for AlwaysOkTool {
        fn name(&self) -> &str {
            "always_ok"
        }
        fn is_read_only(&self) -> bool {
            true
        }
        fn timeout(&self) -> Duration {
            Duration::from_secs(1)
        }
        async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, ToolError> {
            Ok("ok".into())
        }
    }
}
