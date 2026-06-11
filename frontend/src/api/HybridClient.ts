/**
 * ============================================================================
 * HybridClient — 运行时分流 + 自动 fallback
 * ============================================================================
 *
 * 行为:
 *   - 端点状态 == "mock"        → 走 mock,devtools 记录 source="mock"
 *   - 端点状态 == "connected"   → 优先 tauri,失败 fallback mock + warn
 *   - 端点状态 == "verified"    → 仅 tauri,失败让错误冒泡
 *
 * UI 永远拿到数据 (即使后端崩),不白屏.
 * 失败 / 形状不匹配通过 devtools 收集,DevtoolsPanel 展示.
 * ----------------------------------------------------------------------------
 */

import type { AgentClient, EndpointKey, Unsubscribe } from "./AgentClient";
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
} from "./contracts";
import { CONTRACT_STATUS, type ContractStatus } from "./status";
import { devtools } from "./devtools";

export class HybridClient implements AgentClient {
  constructor(
    private readonly mock: AgentClient,
    private readonly tauri: AgentClient,
  ) {}

  /** 命令分流核心 — 所有 command 走它. */
  private async invokeCmd<K extends EndpointKey, T>(
    key: K,
    realCall: () => Promise<T>,
    mockCall: () => Promise<T>,
  ): Promise<T> {
    const status: ContractStatus = CONTRACT_STATUS[key];
    const start = performance.now();

    if (status === "mock") {
      const out = await mockCall();
      devtools.recordCall({
        endpoint: key,
        source: "mock",
        timestamp: Date.now(),
        durationMs: performance.now() - start,
        errorMessage: null,
      });
      return out;
    }

    try {
      const out = await realCall();
      devtools.recordCall({
        endpoint: key,
        source: "real-ok",
        timestamp: Date.now(),
        durationMs: performance.now() - start,
        errorMessage: null,
      });
      return out;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (status === "verified") {
        devtools.recordCall({
          endpoint: key,
          source: "real-failed-fallback",
          timestamp: Date.now(),
          durationMs: performance.now() - start,
          errorMessage: msg,
        });
        throw err; // verified 状态下不 fallback,让错误冒泡
      }
      devtools.recordCall({
        endpoint: key,
        source: "real-failed-fallback",
        timestamp: Date.now(),
        durationMs: performance.now() - start,
        errorMessage: msg,
      });
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn(
          `[ipc] ${key} (${status}) failed, fallback to mock:`,
          msg,
        );
      }
      return mockCall();
    }
  }

  /** 事件订阅分流 — events 不能 fallback (订阅是单向流),只能选其一. */
  private subscribe<K extends EndpointKey, E>(
    key: K,
    realSub: (cb: (e: E) => void) => Unsubscribe,
    mockSub: (cb: (e: E) => void) => Unsubscribe,
    cb: (e: E) => void,
  ): Unsubscribe {
    const status = CONTRACT_STATUS[key];
    if (status === "mock") return mockSub(cb);
    try {
      const unsub = realSub(cb);
      devtools.recordCall({
        endpoint: key,
        source: "real-ok",
        timestamp: Date.now(),
        durationMs: null,
        errorMessage: null,
      });
      return unsub;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      devtools.recordCall({
        endpoint: key,
        source: "real-failed-fallback",
        timestamp: Date.now(),
        durationMs: null,
        errorMessage: msg,
      });
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn(`[ipc] ${key} subscription failed, fallback mock:`, msg);
      }
      return mockSub(cb);
    }
  }

  // ── Commands ────────────────────────────────────────────────────────────
  listThreads = (): Promise<ThreadSummary[]> =>
    this.invokeCmd("listThreads", () => this.tauri.listThreads(), () =>
      this.mock.listThreads(),
    );

  getThread = (id: string): Promise<Thread> =>
    this.invokeCmd("getThread", () => this.tauri.getThread(id), () =>
      this.mock.getThread(id),
    );

  createThread = (input?: CreateThreadInput): Promise<ThreadSummary> =>
    this.invokeCmd("createThread", () => this.tauri.createThread(input), () =>
      this.mock.createThread(input),
    );

  updateThread = (input: UpdateThreadInput): Promise<void> =>
    this.invokeCmd("updateThread", () => this.tauri.updateThread(input), () =>
      this.mock.updateThread(input),
    );

  deleteThread = (id: string): Promise<void> =>
    this.invokeCmd("deleteThread", () => this.tauri.deleteThread(id), () =>
      this.mock.deleteThread(id),
    );

  searchThreads = (q: string): Promise<ThreadSummary[]> =>
    this.invokeCmd("searchThreads", () => this.tauri.searchThreads(q), () =>
      this.mock.searchThreads(q),
    );

  exportThread = (id: string): Promise<string> =>
    this.invokeCmd("exportThread", () => this.tauri.exportThread(id), () =>
      this.mock.exportThread(id),
    );

  listProjects = (): Promise<ProjectSummary[]> =>
    this.invokeCmd("listProjects", () => this.tauri.listProjects(), () =>
      this.mock.listProjects(),
    );

  pickProjectDirectory = (): Promise<string | null> =>
    this.invokeCmd(
      "pickProjectDirectory",
      () => this.tauri.pickProjectDirectory(),
      () => this.mock.pickProjectDirectory(),
    );

  createProject = (input: CreateProjectInput): Promise<ProjectSummary> =>
    this.invokeCmd("createProject", () => this.tauri.createProject(input), () =>
      this.mock.createProject(input),
    );

  updateProject = (input: UpdateProjectInput): Promise<void> =>
    this.invokeCmd("updateProject", () => this.tauri.updateProject(input), () =>
      this.mock.updateProject(input),
    );

  deleteProject = (projectId: string): Promise<void> =>
    this.invokeCmd("deleteProject", () => this.tauri.deleteProject(projectId), () =>
      this.mock.deleteProject(projectId),
    );

  sendMessage = (input: SendMessageInput): Promise<void> =>
    this.invokeCmd("sendMessage", () => this.tauri.sendMessage(input), () =>
      this.mock.sendMessage(input),
    );

  startBrainstorm = (
    input: StartBrainstormInput,
  ): Promise<StartBrainstormResult> =>
    this.tauri.startBrainstorm(input);

  continueBrainstorm = (
    input: ContinueBrainstormInput,
  ): Promise<StartBrainstormResult> =>
    this.tauri.continueBrainstorm(input);

  stopBrainstorm = (runId: string): Promise<void> =>
    this.tauri.stopBrainstorm(runId);

  abortTurn = (id: string): Promise<void> =>
    this.invokeCmd("abortTurn", () => this.tauri.abortTurn(id), () =>
      this.mock.abortTurn(id),
    );

  approveTool = (input: ApproveToolInput): Promise<void> =>
    this.tauri.approveTool(input);

  submitAnswers = (input: SubmitAnswersInput): Promise<void> =>
    this.invokeCmd("submitAnswers", () => this.tauri.submitAnswers(input), () =>
      this.mock.submitAnswers(input),
    );

  listModels = (): Promise<ModelInfo[]> =>
    this.invokeCmd("listModels", () => this.tauri.listModels(), () =>
      this.mock.listModels(),
    );

  switchModel = (
    threadId: string,
    modelId: string,
    providerId?: string,
  ): Promise<void> =>
    this.invokeCmd(
      "switchModel",
      () => this.tauri.switchModel(threadId, modelId, providerId),
      () => this.mock.switchModel(threadId, modelId, providerId),
    );

  getConfig = (): Promise<AppConfig> =>
    this.invokeCmd("getConfig", () => this.tauri.getConfig(), () =>
      this.mock.getConfig(),
    );

  setConfig = (patch: ConfigPatch): Promise<AppConfig> =>
    this.invokeCmd("setConfig", () => this.tauri.setConfig(patch), () =>
      this.mock.setConfig(patch),
    );

  saveProviders = (input: SaveProvidersInput): Promise<AppConfig> =>
    this.invokeCmd("saveProviders", () => this.tauri.saveProviders(input), () =>
      this.mock.saveProviders(input),
    );

  saveWebSearchConfig = (
    input: SaveWebSearchConfigInput,
  ): Promise<AppConfig> =>
    this.invokeCmd(
      "saveWebSearchConfig",
      () => this.tauri.saveWebSearchConfig(input),
      () => this.mock.saveWebSearchConfig(input),
    );

  fetchProviderModels = (
    input: ProviderModelsInput,
  ): Promise<ProviderModel[]> =>
    this.invokeCmd(
      "fetchProviderModels",
      () => this.tauri.fetchProviderModels(input),
      () => this.mock.fetchProviderModels(input),
    );

  testProviderConnection = (
    input: ProviderModelsInput,
  ): Promise<ProviderTestResult> =>
    this.invokeCmd(
      "testProviderConnection",
      () => this.tauri.testProviderConnection(input),
      () => this.mock.testProviderConnection(input),
    );

  debugTestProvider = (
    providerId: string,
    model: string,
    message: string,
  ): Promise<string> =>
    this.invokeCmd(
      "debugTestProvider",
      () => this.tauri.debugTestProvider(providerId, model, message),
      () => this.mock.debugTestProvider(providerId, model, message),
    );


  listHookEvents = (): Promise<HookEventInfo[]> =>
    this.invokeCmd("listHookEvents", () => this.tauri.listHookEvents(), () =>
      this.mock.listHookEvents(),
    );

  getHooksConfig = (input: HooksScopeInput): Promise<HookConfigFile> =>
    this.invokeCmd("getHooksConfig", () => this.tauri.getHooksConfig(input), () =>
      this.mock.getHooksConfig(input),
    );

  saveHooksConfig = (input: SaveHooksConfigInput): Promise<HookConfigFile> =>
    this.invokeCmd("saveHooksConfig", () => this.tauri.saveHooksConfig(input), () =>
      this.mock.saveHooksConfig(input),
    );

  testHook = (input: TestHookInput): Promise<HookTraceEntry> =>
    this.invokeCmd("testHook", () => this.tauri.testHook(input), () =>
      this.mock.testHook(input),
    );

  getProjectHooksTrust = (projectPath: string): Promise<ProjectHooksTrust> =>
    this.invokeCmd(
      "getProjectHooksTrust",
      () => this.tauri.getProjectHooksTrust(projectPath),
      () => this.mock.getProjectHooksTrust(projectPath),
    );

  setProjectHooksTrust = (
    projectPath: string,
    trusted: boolean,
  ): Promise<ProjectHooksTrust> =>
    this.invokeCmd(
      "setProjectHooksTrust",
      () => this.tauri.setProjectHooksTrust(projectPath, trusted),
      () => this.mock.setProjectHooksTrust(projectPath, trusted),
    );

  listMcpServers = (): Promise<McpServer[]> =>
    this.invokeCmd("listMcpServers", () => this.tauri.listMcpServers(), () =>
      this.mock.listMcpServers(),
    );

  listMcpTools = (name: string): Promise<McpToolInfo[]> =>
    this.invokeCmd(
      "listMcpTools",
      () => this.tauri.listMcpTools(name),
      () => this.mock.listMcpTools(name),
    );

  restartMcpServer = (name: string): Promise<void> =>
    this.invokeCmd(
      "restartMcpServer",
      () => this.tauri.restartMcpServer(name),
      () => this.mock.restartMcpServer(name),
    );

  toggleMcpServer = (name: string, enabled: boolean): Promise<void> =>
    this.invokeCmd(
      "toggleMcpServer",
      () => this.tauri.toggleMcpServer(name, enabled),
      () => this.mock.toggleMcpServer(name, enabled),
    );

  mcpAddServer = (name: string, config: unknown): Promise<void> =>
    this.invokeCmd(
      "mcpAddServer",
      () => this.tauri.mcpAddServer(name, config),
      () => this.mock.mcpAddServer(name, config),
    );

  mcpRemoveServer = (name: string): Promise<void> =>
    this.invokeCmd(
      "mcpRemoveServer",
      () => this.tauri.mcpRemoveServer(name),
      () => this.mock.mcpRemoveServer(name),
    );

  mcpReload = (): Promise<void> =>
    this.invokeCmd("mcpReload", () => this.tauri.mcpReload(), () =>
      this.mock.mcpReload(),
    );

  skillList = (threadId?: string): Promise<Skill[]> =>
    this.invokeCmd("skillList", () => this.tauri.skillList(threadId), () =>
      this.mock.skillList(threadId),
    );

  skillRead = (
    name: string,
    threadId?: string,
    args?: string,
  ): Promise<string> =>
    this.invokeCmd(
      "skillRead",
      () => this.tauri.skillRead(name, threadId, args),
      () => this.mock.skillRead(name, threadId, args),
    );

  skillReload = (threadId?: string): Promise<number> =>
    this.invokeCmd("skillReload", () => this.tauri.skillReload(threadId), () =>
      this.mock.skillReload(threadId),
    );
  skillDelete = (name: string): Promise<void> =>
    this.invokeCmd("skillDelete", () => this.tauri.skillDelete(name), () =>
      this.mock.skillDelete(name),
    );


  listOutputStyles = (): Promise<OutputStyle[]> =>
    this.invokeCmd(
      "listOutputStyles",
      () => this.tauri.listOutputStyles(),
      () => this.mock.listOutputStyles(),
    );

  readOutputStyle = (name: string): Promise<string> =>
    this.invokeCmd(
      "readOutputStyle",
      () => this.tauri.readOutputStyle(name),
      () => this.mock.readOutputStyle(name),
    );

  saveOutputStyle = (name: string, content: string): Promise<void> =>
    this.invokeCmd(
      "saveOutputStyle",
      () => this.tauri.saveOutputStyle(name, content),
      () => this.mock.saveOutputStyle(name, content),
    );

  setActiveOutputStyle = (name: string | null): Promise<void> =>
    this.invokeCmd(
      "setActiveOutputStyle",
      () => this.tauri.setActiveOutputStyle(name),
      () => this.mock.setActiveOutputStyle(name),
    );

  deleteOutputStyle = (name: string): Promise<void> =>
    this.invokeCmd(
      "deleteOutputStyle",
      () => this.tauri.deleteOutputStyle(name),
      () => this.mock.deleteOutputStyle(name),
    );

  readGlobalMemory = (): Promise<string> =>
    this.invokeCmd("readGlobalMemory", () => this.tauri.readGlobalMemory(), () =>
      this.mock.readGlobalMemory(),
    );



  rewindThread = (threadId: string, messageSeq: number): Promise<Thread> =>
    this.invokeCmd(
      "rewindThread",
      () => this.tauri.rewindThread(threadId, messageSeq),
      () => this.mock.rewindThread(threadId, messageSeq),
    );

  listRewindPoints = (threadId: string): Promise<RewindPoint[]> =>
    this.invokeCmd(
      "listRewindPoints",
      () => this.tauri.listRewindPoints(threadId),
      () => this.mock.listRewindPoints(threadId),
    );

  getUsageStats = (input?: GetUsageStatsInput): Promise<UsageStats> =>
    this.invokeCmd("getUsageStats", () => this.tauri.getUsageStats(input), () =>
      this.mock.getUsageStats(input),
    );

  getUserBalance = (
    input?: GetUserBalanceInput,
  ): Promise<UserBalance | null> =>
    this.invokeCmd(
      "getUserBalance",
      () => this.tauri.getUserBalance(input),
      () => this.mock.getUserBalance(input),
    );

  getUsageChart = (): Promise<UsageChartPoint[]> =>
    this.invokeCmd("getUsageChart", () => this.tauri.getUsageChart(), () =>
      this.mock.getUsageChart(),
    );

  fsGetWorkspaceRoot = (): Promise<string> =>
    this.invokeCmd(
      "fsGetWorkspaceRoot",
      () => this.tauri.fsGetWorkspaceRoot(),
      () => this.mock.fsGetWorkspaceRoot(),
    );

  fsListDirectory = (
    path: string,
    showHidden?: boolean,
  ): Promise<FsEntry[]> =>
    this.invokeCmd(
      "fsListDirectory",
      () => this.tauri.fsListDirectory(path, showHidden),
      () => this.mock.fsListDirectory(path, showHidden),
    );

  fsReadFile = (path: string, maxBytes?: number): Promise<FsFile> =>
    this.invokeCmd(
      "fsReadFile",
      () => this.tauri.fsReadFile(path, maxBytes),
      () => this.mock.fsReadFile(path, maxBytes),
    );

  fsGrep = (pattern: string, path?: string, glob?: string, maxResults?: number): Promise<GrepMatch[]> =>
    this.invokeCmd(
      "fsGrep",
      () => this.tauri.fsGrep(pattern, path, glob, maxResults),
      () => this.mock.fsGrep(pattern, path, glob, maxResults),
    );

  fsGlob = (pattern: string, path?: string, maxResults?: number): Promise<FsEntry[]> =>
    this.invokeCmd(
      "fsGlob",
      () => this.tauri.fsGlob(pattern, path, maxResults),
      () => this.mock.fsGlob(pattern, path, maxResults),
    );

  searchMessages = (query: string, maxResults?: number): Promise<MessageSearchResult[]> =>
    this.invokeCmd(
      "searchMessages",
      () => this.tauri.searchMessages(query, maxResults),
      () => this.mock.searchMessages(query, maxResults),
    );

  ptySpawn = (input: PtySpawnInput): Promise<string> =>
    this.invokeCmd("ptySpawn", () => this.tauri.ptySpawn(input), () =>
      this.mock.ptySpawn(input),
    );

  ptyList = (): Promise<PtySession[]> =>
    this.invokeCmd("ptyList", () => this.tauri.ptyList(), () =>
      this.mock.ptyList(),
    );

  ptySnapshot = (ptyId: string): Promise<PtySnapshot> =>
    this.invokeCmd("ptySnapshot", () => this.tauri.ptySnapshot(ptyId), () =>
      this.mock.ptySnapshot(ptyId),
    );

  ptyWrite = (ptyId: string, data: string): Promise<void> =>
    this.invokeCmd("ptyWrite", () => this.tauri.ptyWrite(ptyId, data), () =>
      this.mock.ptyWrite(ptyId, data),
    );

  ptyResize = (ptyId: string, cols: number, rows: number): Promise<void> =>
    this.invokeCmd(
      "ptyResize",
      () => this.tauri.ptyResize(ptyId, cols, rows),
      () => this.mock.ptyResize(ptyId, cols, rows),
    );

  ptyKill = (ptyId: string): Promise<void> =>
    this.invokeCmd("ptyKill", () => this.tauri.ptyKill(ptyId), () =>
      this.mock.ptyKill(ptyId),
    );

  exportDiagnostics = (): Promise<string> =>
    this.invokeCmd(
      "exportDiagnostics",
      () => this.tauri.exportDiagnostics(),
      () => this.mock.exportDiagnostics(),
    );

  // ── Permissions (P4 新增) ──────────────────────────────────────────────
  listPermissionRules = (threadId: string): Promise<PermissionRule[]> =>
    this.invokeCmd(
      "listPermissionRules",
      () => this.tauri.listPermissionRules(threadId),
      () => this.mock.listPermissionRules(threadId),
    );

  removePermissionRule = (
    threadId: string,
    rule: PermissionRule,
  ): Promise<void> =>
    this.invokeCmd(
      "removePermissionRule",
      () => this.tauri.removePermissionRule(threadId, rule),
      () => this.mock.removePermissionRule(threadId, rule),
    );

  getPermissionContext = (
    threadId: string,
  ): Promise<ToolPermissionContextDto> =>
    this.invokeCmd(
      "getPermissionContext",
      () => this.tauri.getPermissionContext(threadId),
      () => this.mock.getPermissionContext(threadId),
    );

  cyclePermissionMode = (
    input: CyclePermissionModeInput,
  ): Promise<CyclePermissionModeResult> =>
    this.invokeCmd(
      "cyclePermissionMode",
      () => this.tauri.cyclePermissionMode(input),
      () => this.mock.cyclePermissionMode(input),
    );

  // ── Events ──────────────────────────────────────────────────────────────
  onContentDelta = (cb: (e: ContentDeltaEvent) => void): Unsubscribe =>
    this.subscribe("onContentDelta", (c) => this.tauri.onContentDelta(c), (c) =>
      this.mock.onContentDelta(c), cb);

  onReasoningDelta = (cb: (e: ReasoningDeltaEvent) => void): Unsubscribe =>
    this.subscribe("onReasoningDelta", (c) => this.tauri.onReasoningDelta(c),
      (c) => this.mock.onReasoningDelta(c), cb);

  onToolCallStart = (cb: (e: ToolCallStartEvent) => void): Unsubscribe =>
    this.subscribe("onToolCallStart", (c) => this.tauri.onToolCallStart(c),
      (c) => this.mock.onToolCallStart(c), cb);

  onToolCallUpdate = (cb: (e: ToolCallUpdateEvent) => void): Unsubscribe =>
    this.subscribe("onToolCallUpdate", (c) => this.tauri.onToolCallUpdate(c),
      (c) => this.mock.onToolCallUpdate(c), cb);

  onTurnComplete = (cb: (e: TurnCompleteEvent) => void): Unsubscribe =>
    this.subscribe("onTurnComplete", (c) => this.tauri.onTurnComplete(c),
      (c) => this.mock.onTurnComplete(c), cb);

  onApprovalRequest = (cb: (e: ApprovalRequestEvent) => void): Unsubscribe =>
    this.subscribe("onApprovalRequest", (c) => this.tauri.onApprovalRequest(c),
      (c) => this.mock.onApprovalRequest(c), cb);

  onQuestionRequest = (cb: (e: QuestionRequestEvent) => void): Unsubscribe =>
    this.subscribe("onQuestionRequest", (c) => this.tauri.onQuestionRequest(c),
      (c) => this.mock.onQuestionRequest(c), cb);

  onStreamError = (cb: (e: StreamErrorEvent) => void): Unsubscribe =>
    this.subscribe("onStreamError", (c) => this.tauri.onStreamError(c),
      (c) => this.mock.onStreamError(c), cb);

  onStreamAborted = (cb: (e: StreamAbortedEvent) => void): Unsubscribe =>
    this.subscribe("onStreamAborted", (c) => this.tauri.onStreamAborted(c),
      (c) => this.mock.onStreamAborted(c), cb);

  onBudgetWarning = (cb: (e: BudgetWarningEvent) => void): Unsubscribe =>
    this.subscribe("onBudgetWarning", (c) => this.tauri.onBudgetWarning(c),
      (c) => this.mock.onBudgetWarning(c), cb);

  onModelEscalated = (cb: (e: ModelEscalatedEvent) => void): Unsubscribe =>
    this.subscribe("onModelEscalated", (c) => this.tauri.onModelEscalated(c),
      (c) => this.mock.onModelEscalated(c), cb);

  onTodosUpdated = (cb: (e: TodosUpdatedEvent) => void): Unsubscribe =>
    this.subscribe("onTodosUpdated", (c) => this.tauri.onTodosUpdated(c),
      (c) => this.mock.onTodosUpdated(c), cb);

  onContextUsage = (cb: (e: ContextUsageEvent) => void): Unsubscribe =>
    this.subscribe("onContextUsage", (c) => this.tauri.onContextUsage(c),
      (c) => this.mock.onContextUsage(c), cb);

  onBrainstormRunStarted = (
    cb: (e: BrainstormRunStartedEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormRunStarted",
      (c) => this.tauri.onBrainstormRunStarted(c),
      (c) => this.mock.onBrainstormRunStarted(c),
      cb,
    );

  onBrainstormAgentStatus = (
    cb: (e: BrainstormAgentStatusEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormAgentStatus",
      (c) => this.tauri.onBrainstormAgentStatus(c),
      (c) => this.mock.onBrainstormAgentStatus(c),
      cb,
    );

  onBrainstormMessageStart = (
    cb: (e: BrainstormMessageStartEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormMessageStart",
      (c) => this.tauri.onBrainstormMessageStart(c),
      (c) => this.mock.onBrainstormMessageStart(c),
      cb,
    );

  onBrainstormMessageDelta = (
    cb: (e: BrainstormMessageDeltaEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormMessageDelta",
      (c) => this.tauri.onBrainstormMessageDelta(c),
      (c) => this.mock.onBrainstormMessageDelta(c),
      cb,
    );

  onBrainstormReasoningDelta = (
    cb: (e: BrainstormReasoningDeltaEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormReasoningDelta",
      (c) => this.tauri.onBrainstormReasoningDelta(c),
      (c) => this.mock.onBrainstormReasoningDelta(c),
      cb,
    );

  onBrainstormToolCallStart = (
    cb: (e: BrainstormToolCallStartEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormToolCallStart",
      (c) => this.tauri.onBrainstormToolCallStart(c),
      (c) => this.mock.onBrainstormToolCallStart(c),
      cb,
    );

  onBrainstormToolCallUpdate = (
    cb: (e: BrainstormToolCallUpdateEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormToolCallUpdate",
      (c) => this.tauri.onBrainstormToolCallUpdate(c),
      (c) => this.mock.onBrainstormToolCallUpdate(c),
      cb,
    );

  onBrainstormMessageDone = (
    cb: (e: BrainstormMessageDoneEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormMessageDone",
      (c) => this.tauri.onBrainstormMessageDone(c),
      (c) => this.mock.onBrainstormMessageDone(c),
      cb,
    );

  onBrainstormRunDone = (
    cb: (e: BrainstormRunDoneEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onBrainstormRunDone",
      (c) => this.tauri.onBrainstormRunDone(c),
      (c) => this.mock.onBrainstormRunDone(c),
      cb,
    );

  onBrainstormError = (cb: (e: BrainstormErrorEvent) => void): Unsubscribe =>
    this.subscribe(
      "onBrainstormError",
      (c) => this.tauri.onBrainstormError(c),
      (c) => this.mock.onBrainstormError(c),
      cb,
    );

  onMcpServerStatusChanged = (
    cb: (e: McpServerStatusChangedEvent) => void,
  ): Unsubscribe =>
    this.subscribe(
      "onMcpServerStatusChanged",
      (c) => this.tauri.onMcpServerStatusChanged(c),
      (c) => this.mock.onMcpServerStatusChanged(c),
      cb,
    );

  onMcpToolsChanged = (cb: (e: McpToolsChangedEvent) => void): Unsubscribe =>
    this.subscribe(
      "onMcpToolsChanged",
      (c) => this.tauri.onMcpToolsChanged(c),
      (c) => this.mock.onMcpToolsChanged(c),
      cb,
    );

  onPtyData = (cb: (e: PtyDataEvent) => void): Unsubscribe =>
    this.subscribe("onPtyData", (c) => this.tauri.onPtyData(c), (c) =>
      this.mock.onPtyData(c), cb);

  onPtyExit = (cb: (e: PtyExitEvent) => void): Unsubscribe =>
    this.subscribe("onPtyExit", (c) => this.tauri.onPtyExit(c), (c) =>
      this.mock.onPtyExit(c), cb);
}
