/**
 * ============================================================================
 * TauriAgentClient — 真实后端 IPC 实现
 * ============================================================================
 *
 * 调用 Rust 端 `#[tauri::command]` handlers (snake_case invoke 名),订阅
 * `stream:*` / `approval:*` / `status:*` 事件.
 *
 * 字段命名约定 (Tauri v2 + serde rename_all="camelCase"):
 *   - JS 端调 invoke 时,args object 用 camelCase (e.g. `threadId`),
 *     Tauri runtime 自动映射到 Rust 端的 snake_case 参数名 (e.g. `thread_id`).
 *   - 嵌套 DTO (e.g. SendMessageInput) 内部字段全 camelCase,与
 *     #[serde(rename_all = "camelCase")] 对齐.
 *
 * 事件订阅:
 *   - listen<T>() 返回 Promise<UnlistenFn>,我们包成同步 Unsubscribe.
 *   - 回调收到 Event<T> 对象,从 .payload 取真实 payload.
 *
 * Rust 端命令完整清单:
 *   crates/app/src/main.rs::invoke_handler 中 generate_handler! 注册.
 *
 * 协议字段需与 `frontend/src/api/contracts.ts` 和 Rust DTO 保持一致.
 * ----------------------------------------------------------------------------
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AgentClient, Unsubscribe } from "../AgentClient";
import type {
  AppConfig,
  ApproveToolInput,
  SubmitAnswersInput,
  CreateProjectInput,
  CreateThreadInput,
  ConfigPatch,
  ProviderModel,
  ProviderModelsInput,
  ProviderTestResult,
  SaveProvidersInput,
  SaveWebSearchConfigInput,
  CyclePermissionModeInput,
  CyclePermissionModeResult,
  GetUsageStatsInput,
  GetUserBalanceInput,
  FsEntry,
  FsFile,
  GrepMatch,
  MessageSearchResult,
  HookConfigFile,
  HookEventInfo,
  HookTraceEntry,
  HooksScopeInput,
  ProjectHooksTrust,
  SaveHooksConfigInput,
  TestHookInput,
  PtySpawnInput,
  PtySession,
  PtySnapshot,
  PtyDataEvent,
  PtyExitEvent,
	  McpServer,
	  McpToolInfo,
	  Skill,
  OutputStyle,
  RewindPoint,
  ModelInfo,
  PermissionRule,
  ProjectSummary,
  SendMessageInput,
  StartBrainstormInput,
  ContinueBrainstormInput,
  StartBrainstormResult,
  Thread,
  ThreadSummary,
  ToolPermissionContextDto,
  UpdateProjectInput,
  UpdateThreadInput,
  UsageStats,
  UsageChartPoint,
  UserBalance,
  ContentDeltaEvent,
  ReasoningDeltaEvent,
  ToolCallStartEvent,
  ToolCallUpdateEvent,
  TurnCompleteEvent,
  ApprovalRequestEvent,
  QuestionRequestEvent,
  StreamErrorEvent,
  StreamAbortedEvent,
  BudgetWarningEvent,
  ModelEscalatedEvent,
  TodosUpdatedEvent,
  ContextUsageEvent,
  BrainstormRunStartedEvent,
  BrainstormAgentStatusEvent,
  BrainstormMessageStartEvent,
  BrainstormMessageDeltaEvent,
  BrainstormReasoningDeltaEvent,
  BrainstormToolCallStartEvent,
  BrainstormToolCallUpdateEvent,
  BrainstormMessageDoneEvent,
  BrainstormRunDoneEvent,
  BrainstormErrorEvent,
  McpServerStatusChangedEvent,
  McpToolsChangedEvent,
} from "../contracts";

// ── 事件订阅 helper ──────────────────────────────────────────────────────
//
// listen<T>(name, cb) 是 async (返 Promise<UnlistenFn>),但 AgentClient 接口
// 约定的 onXxx 必须同步返回 Unsubscribe. 我们包一层:
//   - 立即调用 listen(),保留 Promise
//   - 回 Unsubscribe 时,await Promise 拿到真 unlisten 再调
//   - 用户 unsubscribe 太快 (listen 还没 resolve) 也不会 crash
function subscribe<T>(
  eventName: string,
  cb: (payload: T) => void,
): Unsubscribe {
  let unlisten: UnlistenFn | null = null;
  let cancelled = false;
  const promise = listen<T>(eventName, (event) => cb(event.payload));
  void promise.then(
    (u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    },
    // listen() is async; a rejection here (no Tauri runtime, IPC denied, etc.)
    // would otherwise be an unhandled promise rejection AND the subscription
    // would silently never fire. Catch it so it's logged, not swallowed.
    // (HybridClient's try/catch only covers the synchronous path, so this is
    // the only place an async rejection can be observed.)
    (err) => {
      const msg = err instanceof Error ? err.message : String(err);
      // eslint-disable-next-line no-console
      console.error(`[ipc] failed to subscribe to '${eventName}':`, msg);
    },
  );
  return () => {
    cancelled = true;
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  };
}

export class TauriAgentClient implements AgentClient {
  // ── Threads ─────────────────────────────────────────────────────────────
  listThreads(): Promise<ThreadSummary[]> {
    return invoke<ThreadSummary[]>("list_threads");
  }
  getThread(threadId: string): Promise<Thread> {
    return invoke<Thread>("get_thread", { threadId });
  }
  createThread(input?: CreateThreadInput): Promise<ThreadSummary> {
    return invoke<ThreadSummary>("create_thread", input ? { input } : {});
  }
  updateThread(input: UpdateThreadInput): Promise<void> {
    return invoke<void>("update_thread", { input });
  }
  deleteThread(threadId: string): Promise<void> {
    return invoke<void>("delete_thread", { threadId });
  }
  searchThreads(query: string): Promise<ThreadSummary[]> {
    return invoke<ThreadSummary[]>("search_threads", { query });
  }
  exportThread(threadId: string): Promise<string> {
    return invoke<string>("export_thread", { threadId });
  }

  // ── 项目 ───────────────────────────────────────────────────────────────
  listProjects(): Promise<ProjectSummary[]> {
    return invoke<ProjectSummary[]>("list_projects");
  }
  pickProjectDirectory(): Promise<string | null> {
    return invoke<string | null>("pick_project_directory");
  }
  createProject(input: CreateProjectInput): Promise<ProjectSummary> {
    return invoke<ProjectSummary>("create_project", { input });
  }
  updateProject(input: UpdateProjectInput): Promise<void> {
    return invoke<void>("update_project", { input });
  }
  deleteProject(projectId: string): Promise<void> {
    return invoke<void>("delete_project", { projectId });
  }

  // ── Conversation ────────────────────────────────────────────────────────
  sendMessage(input: SendMessageInput): Promise<void> {
    return invoke<void>("send_message", { input });
  }
  startBrainstorm(input: StartBrainstormInput): Promise<StartBrainstormResult> {
    return invoke<StartBrainstormResult>("start_brainstorm", { input });
  }
  continueBrainstorm(
    input: ContinueBrainstormInput,
  ): Promise<StartBrainstormResult> {
    return invoke<StartBrainstormResult>("continue_brainstorm", { input });
  }
  stopBrainstorm(runId: string): Promise<void> {
    return invoke<void>("stop_brainstorm", { runId });
  }
  abortTurn(threadId: string): Promise<void> {
    return invoke<void>("abort_turn", { threadId });
  }
  approveTool(input: ApproveToolInput): Promise<void> {
    return invoke<void>("approve_tool", { input });
  }
  submitAnswers(input: SubmitAnswersInput): Promise<void> {
    return invoke<void>("submit_answers", { input });
  }

  // ── Models / Config ─────────────────────────────────────────────────────
  listModels(): Promise<ModelInfo[]> {
    return invoke<ModelInfo[]>("list_models");
  }
  switchModel(
    threadId: string,
    modelId: string,
    providerId?: string,
  ): Promise<void> {
    return invoke<void>("switch_model", { threadId, modelId, providerId });
  }
  getConfig(): Promise<AppConfig> {
    return invoke<AppConfig>("get_config");
  }
  setConfig(patch: ConfigPatch): Promise<AppConfig> {
    // Rust 端参数名是 `_patch` (P4 暂不消费 patch 内容,前导下划线表示 unused).
    // Tauri v2 invoke 字段映射对前导下划线无特殊处理 — 联调时若反序列化报错,
    // 后端去掉下划线即可,前端无需变更.
    return invoke<AppConfig>("set_config", { patch });
  }
  saveProviders(input: SaveProvidersInput): Promise<AppConfig> {
    return invoke<AppConfig>("save_providers", { input });
  }
  saveWebSearchConfig(input: SaveWebSearchConfigInput): Promise<AppConfig> {
    return invoke<AppConfig>("save_web_search_config", { input });
  }
  fetchProviderModels(input: ProviderModelsInput): Promise<ProviderModel[]> {
    return invoke<ProviderModel[]>("fetch_provider_models", { input });
  }
  testProviderConnection(
    input: ProviderModelsInput,
  ): Promise<ProviderTestResult> {
    return invoke<ProviderTestResult>("test_provider_connection", { input });
  }

  debugTestProvider(
    providerId: string,
    model: string,
    message: string,
  ): Promise<string> {
    return invoke<string>("debug_test_provider", { providerId, model, message });
  }

  listHookEvents(): Promise<HookEventInfo[]> {
    return invoke<HookEventInfo[]>("list_hook_events");
  }
  getHooksConfig(input: HooksScopeInput): Promise<HookConfigFile> {
    return invoke<HookConfigFile>("get_hooks_config", { input });
  }
  saveHooksConfig(input: SaveHooksConfigInput): Promise<HookConfigFile> {
    return invoke<HookConfigFile>("save_hooks_config", { input });
  }
  testHook(input: TestHookInput): Promise<HookTraceEntry> {
    return invoke<HookTraceEntry>("test_hook", { input });
  }
  getProjectHooksTrust(projectPath: string): Promise<ProjectHooksTrust> {
    return invoke<ProjectHooksTrust>("get_project_hooks_trust", { projectPath });
  }
  setProjectHooksTrust(
    projectPath: string,
    trusted: boolean,
  ): Promise<ProjectHooksTrust> {
    return invoke<ProjectHooksTrust>("set_project_hooks_trust", {
      projectPath,
      trusted,
    });
  }

  // ── MCP — 后端 deepseek-mcp (rmcp) 真实现 ────────────────────────────────
  listMcpServers(): Promise<McpServer[]> {
    return invoke<McpServer[]>("list_mcp_servers");
  }
  listMcpTools(name: string): Promise<McpToolInfo[]> {
    return invoke<McpToolInfo[]>("list_mcp_tools", { name });
  }
  restartMcpServer(name: string): Promise<void> {
    return invoke<void>("restart_mcp_server", { name });
  }
  toggleMcpServer(name: string, enabled: boolean): Promise<void> {
    return invoke<void>("toggle_mcp_server", { name, enabled });
  }
  mcpAddServer(name: string, config: unknown): Promise<void> {
    return invoke<void>("mcp_add_server", { name, config });
  }
  mcpRemoveServer(name: string): Promise<void> {
    return invoke<void>("mcp_remove_server", { name });
  }
  mcpReload(): Promise<void> {
    return invoke<void>("mcp_reload");
  }

  // ── Skills — 后端 deepseek-skill 真实现 ──────────────────────────────────
  skillList(threadId?: string): Promise<Skill[]> {
    return invoke<Skill[]>("skill_list", threadId ? { threadId } : {});
  }
  skillRead(name: string, threadId?: string, args?: string): Promise<string> {
    return invoke<string>("skill_read", {
      name,
      ...(threadId ? { threadId } : {}),
      ...(args !== undefined ? { args } : {}),
    });
  }
  skillReload(threadId?: string): Promise<number> {
    return invoke<number>("skill_reload", threadId ? { threadId } : {});
  }
  skillDelete(name: string): Promise<void> {
    return invoke<void>("skill_delete", { name });
  }


  // ── Output Styles (Phase 2) ───────────────────────────────────────────────
  listOutputStyles(): Promise<OutputStyle[]> {
    return invoke<OutputStyle[]>("list_output_styles");
  }
  readOutputStyle(name: string): Promise<string> {
    return invoke<string>("read_output_style", { name });
  }
  saveOutputStyle(name: string, content: string): Promise<void> {
    return invoke<void>("save_output_style", { name, content });
  }
  setActiveOutputStyle(name: string | null): Promise<void> {
    return invoke<void>("set_active_output_style", { name });
  }
  deleteOutputStyle(name: string): Promise<void> {
    return invoke<void>("delete_output_style", { name });
  }

  // ── 长期记忆 ──────────────────────────────────────────────────────────────
  readGlobalMemory(): Promise<string> {
    return invoke<string>("read_global_memory");
  }
  writeGlobalMemory(content: string): Promise<void> {
    return invoke<void>("write_global_memory", { content });
  }


  // ── Rewind (P2) ───────────────────────────────────────────────────────────
  rewindThread(threadId: string, messageSeq: number): Promise<Thread> {
    return invoke<Thread>("rewind_thread", { threadId, messageSeq });
  }
  listRewindPoints(threadId: string): Promise<RewindPoint[]> {
    return invoke<RewindPoint[]>("list_rewind_points", { threadId });
  }

  // ── Stats ───────────────────────────────────────────────────────────────
  getUsageStats(input?: GetUsageStatsInput): Promise<UsageStats> {
    return invoke<UsageStats>("get_usage_stats", input ? { input } : {});
  }

  getUsageChart(): Promise<UsageChartPoint[]> {
    return invoke<UsageChartPoint[]>("get_usage_chart");
  }

  /**
   * P3a task 7. Tauri 端命令名 get_user_balance.失败时后端 return Ok(None),
   * Tauri 序列化为 JSON null,所以这里类型是 UserBalance | null 而非抛异常.
   */
  getUserBalance(
    input?: GetUserBalanceInput,
  ): Promise<UserBalance | null> {
    return invoke<UserBalance | null>(
      "get_user_balance",
      input ? { input } : {},
    );
  }

  // ── 文件系统 ───────────────────────────────────────────────────────────
  fsGetWorkspaceRoot(): Promise<string> {
    return invoke<string>("fs_get_workspace_root");
  }
  fsListDirectory(path: string, showHidden?: boolean): Promise<FsEntry[]> {
    return invoke<FsEntry[]>("fs_list_directory", { path, showHidden });
  }
  fsReadFile(path: string, maxBytes?: number): Promise<FsFile> {
    return invoke<FsFile>("fs_read_file", { path, maxBytes });
  }
  fsGrep(pattern: string, path?: string, glob?: string, maxResults?: number): Promise<GrepMatch[]> {
    return invoke<GrepMatch[]>("fs_grep", { pattern, path, glob, maxResults });
  }
  fsGlob(pattern: string, path?: string, maxResults?: number): Promise<FsEntry[]> {
    return invoke<FsEntry[]>("fs_glob", { pattern, path, maxResults });
  }
  searchMessages(query: string, maxResults?: number): Promise<MessageSearchResult[]> {
    return invoke<MessageSearchResult[]>("search_messages", { query, maxResults });
  }
  polishPrompt(text: string): Promise<string> {
    return invoke<string>("polish_prompt", { text });
  }


  // ── 终端 PTY ───────────────────────────────────────────────────────────
  ptyList(): Promise<PtySession[]> {
    return invoke<PtySession[]>("pty_list");
  }
  ptySnapshot(ptyId: string): Promise<PtySnapshot> {
    return invoke<PtySnapshot>("pty_snapshot", { ptyId });
  }
  ptySpawn(input: PtySpawnInput): Promise<string> {
    return invoke<string>("pty_spawn", {
      cwd: input.cwd,
      cols: input.cols,
      rows: input.rows,
    });
  }
  ptyWrite(ptyId: string, data: string): Promise<void> {
    return invoke<void>("pty_write", { ptyId, data });
  }
  ptyResize(ptyId: string, cols: number, rows: number): Promise<void> {
    return invoke<void>("pty_resize", { ptyId, cols, rows });
  }
  ptyKill(ptyId: string): Promise<void> {
    return invoke<void>("pty_kill", { ptyId });
  }

  // ── Diagnostics ─────────────────────────────────────────────────────────
  exportDiagnostics(): Promise<string> {
    return invoke<string>("export_diagnostics");
  }

  // ── Permissions ─────────────────────────────────────────────────────────
  listPermissionRules(threadId: string): Promise<PermissionRule[]> {
    return invoke<PermissionRule[]>("list_permission_rules", { threadId });
  }
  removePermissionRule(
    threadId: string,
    rule: PermissionRule,
  ): Promise<void> {
    return invoke<void>("remove_permission_rule", { threadId, rule });
  }
  getPermissionContext(threadId: string): Promise<ToolPermissionContextDto> {
    return invoke<ToolPermissionContextDto>("get_permission_context", {
      threadId,
    });
  }
  cyclePermissionMode(
    input: CyclePermissionModeInput,
  ): Promise<CyclePermissionModeResult> {
    return invoke<CyclePermissionModeResult>("cycle_permission_mode", {
      threadId: input.threadId,
    });
  }

  // ── Events ──────────────────────────────────────────────────────────────
  onContentDelta(cb: (e: ContentDeltaEvent) => void): Unsubscribe {
    return subscribe<ContentDeltaEvent>("stream:content_delta", cb);
  }
  onReasoningDelta(cb: (e: ReasoningDeltaEvent) => void): Unsubscribe {
    return subscribe<ReasoningDeltaEvent>("stream:reasoning_delta", cb);
  }
  onToolCallStart(cb: (e: ToolCallStartEvent) => void): Unsubscribe {
    return subscribe<ToolCallStartEvent>("stream:tool_call_start", cb);
  }
  onToolCallUpdate(cb: (e: ToolCallUpdateEvent) => void): Unsubscribe {
    return subscribe<ToolCallUpdateEvent>("stream:tool_call_update", cb);
  }
  onTurnComplete(cb: (e: TurnCompleteEvent) => void): Unsubscribe {
    return subscribe<TurnCompleteEvent>("stream:turn_complete", cb);
  }
  onApprovalRequest(cb: (e: ApprovalRequestEvent) => void): Unsubscribe {
    return subscribe<ApprovalRequestEvent>("approval:request", cb);
  }
  onQuestionRequest(cb: (e: QuestionRequestEvent) => void): Unsubscribe {
    return subscribe<QuestionRequestEvent>("question:request", cb);
  }
  onStreamError(cb: (e: StreamErrorEvent) => void): Unsubscribe {
    return subscribe<StreamErrorEvent>("stream:error", cb);
  }
  onStreamAborted(cb: (e: StreamAbortedEvent) => void): Unsubscribe {
    return subscribe<StreamAbortedEvent>("stream:aborted", cb);
  }
  onBudgetWarning(cb: (e: BudgetWarningEvent) => void): Unsubscribe {
    return subscribe<BudgetWarningEvent>("status:budget_warning", cb);
  }
  onModelEscalated(cb: (e: ModelEscalatedEvent) => void): Unsubscribe {
    return subscribe<ModelEscalatedEvent>("status:model_escalated", cb);
  }
  onTodosUpdated(cb: (e: TodosUpdatedEvent) => void): Unsubscribe {
    return subscribe<TodosUpdatedEvent>("stream:todos_updated", cb);
  }
  onContextUsage(cb: (e: ContextUsageEvent) => void): Unsubscribe {
    return subscribe<ContextUsageEvent>("stream:context_usage", cb);
  }
  onBrainstormRunStarted(
    cb: (e: BrainstormRunStartedEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormRunStartedEvent>("brainstorm:run_started", cb);
  }
  onBrainstormAgentStatus(
    cb: (e: BrainstormAgentStatusEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormAgentStatusEvent>("brainstorm:agent_status", cb);
  }
  onBrainstormMessageStart(
    cb: (e: BrainstormMessageStartEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormMessageStartEvent>("brainstorm:message_start", cb);
  }
  onBrainstormMessageDelta(
    cb: (e: BrainstormMessageDeltaEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormMessageDeltaEvent>("brainstorm:message_delta", cb);
  }
  onBrainstormReasoningDelta(
    cb: (e: BrainstormReasoningDeltaEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormReasoningDeltaEvent>(
      "brainstorm:reasoning_delta",
      cb,
    );
  }
  onBrainstormToolCallStart(
    cb: (e: BrainstormToolCallStartEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormToolCallStartEvent>(
      "brainstorm:tool_call_start",
      cb,
    );
  }
  onBrainstormToolCallUpdate(
    cb: (e: BrainstormToolCallUpdateEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormToolCallUpdateEvent>(
      "brainstorm:tool_call_update",
      cb,
    );
  }
  onBrainstormMessageDone(
    cb: (e: BrainstormMessageDoneEvent) => void,
  ): Unsubscribe {
    return subscribe<BrainstormMessageDoneEvent>("brainstorm:message_done", cb);
  }
  onBrainstormRunDone(cb: (e: BrainstormRunDoneEvent) => void): Unsubscribe {
    return subscribe<BrainstormRunDoneEvent>("brainstorm:run_done", cb);
  }
  onBrainstormError(cb: (e: BrainstormErrorEvent) => void): Unsubscribe {
    return subscribe<BrainstormErrorEvent>("brainstorm:error", cb);
  }
  onMcpServerStatusChanged(
    cb: (e: McpServerStatusChangedEvent) => void,
  ): Unsubscribe {
    return subscribe<McpServerStatusChangedEvent>(
      "mcp:server_status_changed",
      cb,
    );
  }
  onMcpToolsChanged(cb: (e: McpToolsChangedEvent) => void): Unsubscribe {
    return subscribe<McpToolsChangedEvent>("mcp:tools_changed", cb);
  }
  onPtyData(cb: (e: PtyDataEvent) => void): Unsubscribe {
    return subscribe<PtyDataEvent>("pty:data", cb);
  }
  onPtyExit(cb: (e: PtyExitEvent) => void): Unsubscribe {
    return subscribe<PtyExitEvent>("pty:exit", cb);
  }
}
