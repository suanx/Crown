/**
 * ============================================================================
 * AgentClient v2 — IPC 接口定义 (Claude Code 对齐)
 * ============================================================================
 *
 * UI 层永远只依赖这个接口,不直接调 @tauri-apps/api.
 * 实现见:
 *   - src/api/mock/MockAgentClient.ts
 *   - src/api/tauri/TauriAgentClient.ts
 *   - src/api/HybridClient.ts
 * ----------------------------------------------------------------------------
 */

import type {
  Thread,
  ThreadSummary,
  ProjectSummary,
  ModelInfo,
  McpServer,
  McpToolInfo,
  Skill,
  OutputStyle,
  RewindPoint,
  UsageStats,
  UsageChartPoint,
  AppConfig,
  PermissionRule,
  ToolPermissionContextDto,
  SendMessageInput,
  StartBrainstormInput,
  ContinueBrainstormInput,
  StartBrainstormResult,
  ApproveToolInput,
  SubmitAnswersInput,
  CreateProjectInput,
  CreateThreadInput,
  UpdateThreadInput,
  UpdateProjectInput,
  ConfigPatch,
  ProviderModel,
  ProviderModelsInput,
  ProviderTestResult,
  SaveProvidersInput,
  SaveWebSearchConfigInput,
  GetUsageStatsInput,
  GetUserBalanceInput,
  UserBalance,
  HookConfigFile,
  HookEventInfo,
  HookTraceEntry,
  HooksScopeInput,
  ProjectHooksTrust,
  SaveHooksConfigInput,
  TestHookInput,
  CyclePermissionModeInput,
  CyclePermissionModeResult,
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
  FsEntry,
  FsFile,
  GrepMatch,
  MessageSearchResult,
  PtySpawnInput,
  PtySession,
  PtySnapshot,
  PtyDataEvent,
  PtyExitEvent,
  McpServerStatusChangedEvent,
  McpToolsChangedEvent,
} from "./contracts";

export type Unsubscribe = () => void;

export interface AgentClient {
  // ── Threads ─────────────────────────────────────────────────────────────
  listThreads(): Promise<ThreadSummary[]>;
  getThread(threadId: string): Promise<Thread>;
  createThread(input?: CreateThreadInput): Promise<ThreadSummary>;
  updateThread(input: UpdateThreadInput): Promise<void>;
  deleteThread(threadId: string): Promise<void>;
  searchThreads(query: string): Promise<ThreadSummary[]>;
  exportThread(threadId: string): Promise<string>;

  // ── 项目 ───────────────────────────────────────────────────────────────
  listProjects(): Promise<ProjectSummary[]>;
  pickProjectDirectory(): Promise<string | null>;
  createProject(input: CreateProjectInput): Promise<ProjectSummary>;
  updateProject(input: UpdateProjectInput): Promise<void>;
  deleteProject(projectId: string): Promise<void>;

  // ── Conversation ────────────────────────────────────────────────────────
  sendMessage(input: SendMessageInput): Promise<void>;
  startBrainstorm(input: StartBrainstormInput): Promise<StartBrainstormResult>;
  continueBrainstorm(input: ContinueBrainstormInput): Promise<StartBrainstormResult>;
  stopBrainstorm(runId: string): Promise<void>;
  abortTurn(threadId: string): Promise<void>;
  approveTool(input: ApproveToolInput): Promise<void>;
  /** 提交结构化问答答案（ask_user_question 工具的回灌通道）。 */
  submitAnswers(input: SubmitAnswersInput): Promise<void>;

  // ── Models / Config ─────────────────────────────────────────────────────
  listModels(): Promise<ModelInfo[]>;
  switchModel(
    threadId: string,
    modelId: string,
    providerId?: string,
  ): Promise<void>;
  getConfig(): Promise<AppConfig>;
  setConfig(patch: ConfigPatch): Promise<AppConfig>;
  saveProviders(input: SaveProvidersInput): Promise<AppConfig>;
  saveWebSearchConfig(input: SaveWebSearchConfigInput): Promise<AppConfig>;
  fetchProviderModels(input: ProviderModelsInput): Promise<ProviderModel[]>;
  testProviderConnection(
    input: ProviderModelsInput,
  ): Promise<ProviderTestResult>;
  debugTestProvider(
    providerId: string,
    model: string,
    message: string,
  ): Promise<string>;
  listHookEvents(): Promise<HookEventInfo[]>;
  getHooksConfig(input: HooksScopeInput): Promise<HookConfigFile>;
  saveHooksConfig(input: SaveHooksConfigInput): Promise<HookConfigFile>;
  testHook(input: TestHookInput): Promise<HookTraceEntry>;
  getProjectHooksTrust(projectPath: string): Promise<ProjectHooksTrust>;
  setProjectHooksTrust(
    projectPath: string,
    trusted: boolean,
  ): Promise<ProjectHooksTrust>;

  // ── MCP ─────────────────────────────────────────────────────────────────
  listMcpServers(): Promise<McpServer[]>;
  /** List tools with full input schemas for a specific MCP server. */
  listMcpTools(name: string): Promise<McpToolInfo[]>;
  restartMcpServer(name: string): Promise<void>;
  toggleMcpServer(name: string, enabled: boolean): Promise<void>;
  /** 把一个 server 配置写进全局 mcp.json 并热重载连接. config 是 MCP 标准配置对象. */
  mcpAddServer(name: string, config: unknown): Promise<void>;
  /** 从 mcp.json 删除并断开 server. */
  mcpRemoveServer(name: string): Promise<void>;
  /** 重新读取 mcp.json 并重连所有 server (手动刷新). */
  mcpReload(): Promise<void>;

  // ── Skills ──────────────────────────────────────────────────────────────
  /** 发现所有可用 skill (全局 + 该 thread 的项目作用域). */
  skillList(threadId?: string): Promise<Skill[]>;
  /** 按名读取 skill 正文 (预览用,等价模型调 skill 工具拿到的文本). */
  skillRead(name: string, threadId?: string, args?: string): Promise<string>;
  /** 重新扫描 skill 目录,返回发现数量 (刷新信号). */
  skillReload(threadId?: string): Promise<number>;
  skillDelete(name: string): Promise<void>;

  // ── 长期记忆 ──────────────────────────────────────────────────────────────
  /** 读取全局记忆文件 (AGENTS.md), 不存在时返回空字符串. */
  readGlobalMemory(): Promise<string>;

  writeGlobalMemory(content: string): Promise<void>;
  // ── Output Styles (Phase 2) ───────────────────────────────────────────────
  /** 列出所有输出风格 (含当前生效标记). */
  listOutputStyles(): Promise<OutputStyle[]>;
  /** 读取某个输出风格的 Markdown 正文. */
  readOutputStyle(name: string): Promise<string>;
  /** 创建或覆盖一个输出风格文件. */
  saveOutputStyle(name: string, content: string): Promise<void>;
  /** 设置 (或用 null 清除) 当前生效的输出风格,立即对后续回合生效. */
  setActiveOutputStyle(name: string | null): Promise<void>;
  /** 删除一个输出风格文件 (若它正生效则一并清除生效状态). */
  deleteOutputStyle(name: string): Promise<void>;

  // ── Rewind (P2) ───────────────────────────────────────────────────────────
  /** 回溯到某条用户消息：截断对话 + 还原文件，返回回溯后的 thread. */
  rewindThread(threadId: string, messageSeq: number): Promise<Thread>;
  /** 列出可回溯的用户消息点 (预览 + 改了几个文件). */
  listRewindPoints(threadId: string): Promise<RewindPoint[]>;

  // ── Stats ───────────────────────────────────────────────────────────────
  getUsageChart(): Promise<UsageChartPoint[]>;

  getUsageStats(input?: GetUsageStatsInput): Promise<UsageStats>;

  /**
   * 用户余额查询 (P3a task 7).
   * 失败 (网络 / 认证 / 不支持 provider) 时返回 null,UI 应隐藏 Balance cell.
   */
  getUserBalance(input?: GetUserBalanceInput): Promise<UserBalance | null>;

  // ── 文件系统 ───────────────────────────────────────────────────────────
  fsGetWorkspaceRoot(): Promise<string>;
  fsListDirectory(path: string, showHidden?: boolean): Promise<FsEntry[]>;
  fsReadFile(path: string, maxBytes?: number): Promise<FsFile>;
  fsGrep(pattern: string, path?: string, glob?: string, maxResults?: number): Promise<GrepMatch[]>;
  fsGlob(pattern: string, path?: string, maxResults?: number): Promise<FsEntry[]>;
  searchMessages(query: string, maxResults?: number): Promise<MessageSearchResult[]>;


  // ── 终端 PTY ───────────────────────────────────────────────────────────
  ptyList(): Promise<PtySession[]>;
  ptySnapshot(ptyId: string): Promise<PtySnapshot>;
  ptySpawn(input: PtySpawnInput): Promise<string>;
  ptyWrite(ptyId: string, data: string): Promise<void>;
  ptyResize(ptyId: string, cols: number, rows: number): Promise<void>;
  ptyKill(ptyId: string): Promise<void>;

  // ── Diagnostics ─────────────────────────────────────────────────────────
  exportDiagnostics(): Promise<string>;

  // ── Permissions (P4 新增) ──────────────────────────────────────────────
  debugTestStream(providerId: string, model: string, message: string): Promise<string>;
  polishPrompt(text: string): Promise<string>;
  /** 列出当前 thread session 内"始终允许"的规则 (Settings 撤销 UI 用). */
  listPermissionRules(threadId: string): Promise<PermissionRule[]>;
  /** 撤销之前的"允许并记住". */
  removePermissionRule(threadId: string, rule: PermissionRule): Promise<void>;
  /** 调试用:返回完整 ToolPermissionContext. */
  getPermissionContext(threadId: string): Promise<ToolPermissionContextDto>;
  /** P4b: 循环切换 per-thread 权限模式 (ComposerModeSelector). */
  cyclePermissionMode(
    input: CyclePermissionModeInput,
  ): Promise<CyclePermissionModeResult>;

  // ── Events ──────────────────────────────────────────────────────────────
  onContentDelta(cb: (e: ContentDeltaEvent) => void): Unsubscribe;
  onReasoningDelta(cb: (e: ReasoningDeltaEvent) => void): Unsubscribe;
  onToolCallStart(cb: (e: ToolCallStartEvent) => void): Unsubscribe;
  onToolCallUpdate(cb: (e: ToolCallUpdateEvent) => void): Unsubscribe;
  onTurnComplete(cb: (e: TurnCompleteEvent) => void): Unsubscribe;
  onApprovalRequest(cb: (e: ApprovalRequestEvent) => void): Unsubscribe;
  /** 结构化问答请求（agent 调 ask_user_question）。 */
  onQuestionRequest(cb: (e: QuestionRequestEvent) => void): Unsubscribe;
  onStreamError(cb: (e: StreamErrorEvent) => void): Unsubscribe;
  onStreamAborted(cb: (e: StreamAbortedEvent) => void): Unsubscribe;
  onBudgetWarning(cb: (e: BudgetWarningEvent) => void): Unsubscribe;
  onModelEscalated(cb: (e: ModelEscalatedEvent) => void): Unsubscribe;
  onTodosUpdated(cb: (e: TodosUpdatedEvent) => void): Unsubscribe;
  onContextUsage(cb: (e: ContextUsageEvent) => void): Unsubscribe;
  onBrainstormRunStarted(cb: (e: BrainstormRunStartedEvent) => void): Unsubscribe;
  onBrainstormAgentStatus(cb: (e: BrainstormAgentStatusEvent) => void): Unsubscribe;
  onBrainstormMessageStart(cb: (e: BrainstormMessageStartEvent) => void): Unsubscribe;
  onBrainstormMessageDelta(cb: (e: BrainstormMessageDeltaEvent) => void): Unsubscribe;
  onBrainstormReasoningDelta(cb: (e: BrainstormReasoningDeltaEvent) => void): Unsubscribe;
  onBrainstormToolCallStart(cb: (e: BrainstormToolCallStartEvent) => void): Unsubscribe;
  onBrainstormToolCallUpdate(cb: (e: BrainstormToolCallUpdateEvent) => void): Unsubscribe;
  onBrainstormMessageDone(cb: (e: BrainstormMessageDoneEvent) => void): Unsubscribe;
  onBrainstormRunDone(cb: (e: BrainstormRunDoneEvent) => void): Unsubscribe;
  onBrainstormError(cb: (e: BrainstormErrorEvent) => void): Unsubscribe;
  onMcpServerStatusChanged(
    cb: (e: McpServerStatusChangedEvent) => void,
  ): Unsubscribe;
  onMcpToolsChanged(cb: (e: McpToolsChangedEvent) => void): Unsubscribe;
  onPtyData(cb: (e: PtyDataEvent) => void): Unsubscribe;
  onPtyExit(cb: (e: PtyExitEvent) => void): Unsubscribe;
}

/**
 * 端点列表 — 用于 ContractStatus map 强制完整覆盖.
 * 加新端点:接口加 → 这里加 → status.ts 加状态 → mock + tauri + hybrid 都补.
 */
export const COMMAND_KEYS = [
  "listThreads",
  "getThread",
  "createThread",
  "updateThread",
  "deleteThread",
  "searchThreads",
  "exportThread",
  "listProjects",
  "pickProjectDirectory",
  "createProject",
  "updateProject",
  "deleteProject",
  "sendMessage",
  "startBrainstorm",
  "continueBrainstorm",
  "stopBrainstorm",
  "abortTurn",
  "approveTool",
  "submitAnswers",
  "listModels",
  "switchModel",
  "getConfig",
  "setConfig",
  "saveProviders",
  "saveWebSearchConfig",
  "fetchProviderModels",
  "testProviderConnection",
  "listHookEvents",
  "getHooksConfig",
  "saveHooksConfig",
  "testHook",
  "getProjectHooksTrust",
  "setProjectHooksTrust",
  "listMcpServers",
  "listMcpTools",
  "restartMcpServer",
  "toggleMcpServer",
  "mcpAddServer",
  "mcpRemoveServer",
  "mcpReload",
  "skillList",
  "skillRead",
  "skillReload",
  "skillDelete",
  "listOutputStyles",
  "readOutputStyle",
  "saveOutputStyle",
  "setActiveOutputStyle",
  "deleteOutputStyle",
  "readGlobalMemory",
  "writeGlobalMemory",
  "rewindThread",
  "getUsageChart",
  "listRewindPoints",
  "getUsageStats",
  "getUserBalance",
  "fsGetWorkspaceRoot",
  "fsListDirectory",
  "fsReadFile",
  "fsGrep",
  "fsGlob",
  "searchMessages",
  "ptyList",
  "ptySnapshot",
  "ptySpawn",
  "ptyWrite",
  "ptyResize",
  "debugTestProvider",
  "debugTestStream",
  "ptyKill",
  "exportDiagnostics",
  "polishPrompt",
  "listPermissionRules",
  "removePermissionRule",
  "getPermissionContext",
  "cyclePermissionMode",
] as const;

export const EVENT_KEYS = [
  "onContentDelta",
  "onReasoningDelta",
  "onToolCallStart",
  "onToolCallUpdate",
  "onTurnComplete",
  "onApprovalRequest",
  "onQuestionRequest",
  "onStreamError",
  "onStreamAborted",
  "onBudgetWarning",
  "onModelEscalated",
  "onTodosUpdated",
  "onContextUsage",
  "onBrainstormRunStarted",
  "onBrainstormAgentStatus",
  "onBrainstormMessageStart",
  "onBrainstormMessageDelta",
  "onBrainstormReasoningDelta",
  "onBrainstormToolCallStart",
  "onBrainstormToolCallUpdate",
  "onBrainstormMessageDone",
  "onBrainstormRunDone",
  "onBrainstormError",
  "onMcpServerStatusChanged",
  "onMcpToolsChanged",
  "onPtyData",
  "onPtyExit",
] as const;

export type CommandKey = (typeof COMMAND_KEYS)[number];
export type EventKey = (typeof EVENT_KEYS)[number];
export type EndpointKey = CommandKey | EventKey;
