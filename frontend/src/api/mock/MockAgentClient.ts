/**
 * ============================================================================
 * MockAgentClient — 纯前端假后端
 * ============================================================================
 *
 * 实现 AgentClient 接口的全套方法,数据来自 ./data.ts.
 * 设计要点:
 *   - 所有方法都是 async,即使数据是同步的 — 与真实 IPC 行为一致
 *   - 简单的 setTimeout 模拟网络延迟 (50-150ms)
 *   - sendMessage 不实现 streaming 行为 (静态原型阶段够用)
 *   - 所有 onXxx 事件订阅返回空 Unsubscribe (静态阶段没有事件流)
 *
 * 后续如果需要演示动效,可在此扩展定时器驱动的 fake stream.
 * ----------------------------------------------------------------------------
 */

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
  // event types
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
import {
  MOCK_CONFIG,
  MOCK_MCP_SERVERS,
  MOCK_SKILLS,
  MOCK_MODELS,
  MOCK_PROJECTS,
  MOCK_THREAD_SUMMARIES,
  MOCK_THREAD_T1,
  MOCK_USAGE_STATS,
  MOCK_USER_BALANCE,
} from "./data";

const noopUnsub: Unsubscribe = () => {};

const delay = (ms = 80) => new Promise<void>((r) => setTimeout(r, ms));

export class MockAgentClient implements AgentClient {
  private mockHooksConfig: HookConfigFile = {
    disableAllHooks: false,
    trustedProjects: [],
    hooks: {},
  };

  // Output-style mock state (Phase 2).
  private mockOutputStyles: OutputStyle[] = [
    { name: "concise", active: false },
    { name: "explanatory", active: false },
  ];
  private mockOutputStyleBodies: Record<string, string> = {
    concise: "Be extremely concise. One sentence when possible.",
    explanatory: "Explain your reasoning step by step before the answer.",
  };

  // ── Threads ─────────────────────────────────────────────────────────────
  async listThreads(): Promise<ThreadSummary[]> {
    await delay();
    return clone(MOCK_THREAD_SUMMARIES);
  }

  async getThread(threadId: string): Promise<Thread> {
    await delay();
    if (threadId === MOCK_THREAD_T1.id) return clone(MOCK_THREAD_T1);
    // 其它 thread 仅有摘要 — 回个空消息列表的占位
    const summary = MOCK_THREAD_SUMMARIES.find((t) => t.id === threadId);
    if (!summary) throw new Error(`thread not found: ${threadId}`);
    return {
      id: summary.id,
      title: summary.title,
      createdAt: summary.updatedAt,
      updatedAt: summary.updatedAt,
      model: "deepseek-v4-flash",
      thinkingEffort: "medium",
      providerId: "deepseek",
      permissionMode: "default",
      costUsd: 0,
      messages: [],
    };
  }

  async createThread(input?: CreateThreadInput): Promise<ThreadSummary> {
    await delay();
    return {
      id: `thread-${Date.now()}`,
      title: "新对话",
      updatedAt: new Date().toISOString(),
      messageCount: 0,
      isStreaming: false,
      isPinned: false,
      preview: null,
      projectId: input?.projectId ?? null,
      providerId: input?.providerId ?? "deepseek",
    };
  }

  async updateThread(_input: UpdateThreadInput): Promise<void> {
    await delay();
  }

  async deleteThread(_threadId: string): Promise<void> {
    await delay();
  }

  async searchThreads(query: string): Promise<ThreadSummary[]> {
    await delay();
    const q = query.trim().toLowerCase();
    if (!q) return clone(MOCK_THREAD_SUMMARIES);
    return MOCK_THREAD_SUMMARIES.filter((t) =>
      t.title.toLowerCase().includes(q),
    ).map(clone);
  }

  async exportThread(_threadId: string): Promise<string> {
    await delay();
    return "# 已导出 (mock)\n\n这是 mock 导出的占位 markdown。";
  }

  async listProjects(): Promise<ProjectSummary[]> {
    await delay();
    return clone(MOCK_PROJECTS);
  }

  async pickProjectDirectory(): Promise<string | null> {
    await delay();
    return "/mock/crown";
  }

  async createProject(input: CreateProjectInput): Promise<ProjectSummary> {
    await delay();
    return {
      id: `proj-${Date.now()}`,
      name: input.name,
      path: input.path,
      threadCount: 0,
      lastUsedAt: new Date().toISOString(),
    };
  }

  async updateProject(_input: UpdateProjectInput): Promise<void> {
    await delay();
  }

  async deleteProject(_projectId: string): Promise<void> {
    await delay();
  }

  // ── Conversation ────────────────────────────────────────────────────────
  async sendMessage(_input: SendMessageInput): Promise<void> {
    await delay();
  }

  async startBrainstorm(
    _input: StartBrainstormInput,
  ): Promise<StartBrainstormResult> {
    await delay();
    throw new Error("brainstorm 需要真实 Tauri 后端");
  }

  async continueBrainstorm(
    _input: ContinueBrainstormInput,
  ): Promise<StartBrainstormResult> {
    await delay();
    throw new Error("brainstorm 需要真实 Tauri 后端");
  }

  async stopBrainstorm(_runId: string): Promise<void> {
    await delay();
  }

  async abortTurn(_threadId: string): Promise<void> {
    await delay();
  }

  async approveTool(_input: ApproveToolInput): Promise<void> {
    await delay();
  }

  async submitAnswers(_input: SubmitAnswersInput): Promise<void> {
    await delay();
  }

  // ── Models / Config ─────────────────────────────────────────────────────
  async listModels(): Promise<ModelInfo[]> {
    await delay();
    return clone(MOCK_MODELS);
  }

  async switchModel(
    _threadId: string,
    _modelId: string,
    _providerId?: string,
  ): Promise<void> {
    await delay();
  }

  async getConfig(): Promise<AppConfig> {
    await delay();
    return clone(MOCK_CONFIG);
  }

  async setConfig(patch: ConfigPatch): Promise<AppConfig> {
    await delay();
    const next = { ...MOCK_CONFIG, ...patch } as AppConfig;
    return clone(next);
  }

  async saveProviders(input: SaveProvidersInput): Promise<AppConfig> {
    await delay();
    return clone({
      ...MOCK_CONFIG,
      providers: input.providers,
      defaultProviderId: input.defaultProviderId,
      defaultModel: input.defaultModel,
    });
  }

  async saveWebSearchConfig(
    input: SaveWebSearchConfigInput,
  ): Promise<AppConfig> {
    await delay();
    return clone({
      ...MOCK_CONFIG,
      webSearch: {
        defaultProviderId: input.defaultProviderId,
        providers: input.providers,
      },
    });
  }

  async fetchProviderModels(
    input: ProviderModelsInput,
  ): Promise<ProviderModel[]> {
    await delay();
    return clone(input.provider.models.length > 0 ? input.provider.models : [
      { id: "mock-model", label: "mock-model", enabled: true },
    ]);
  }

  async testProviderConnection(
    input: ProviderModelsInput,
  ): Promise<ProviderTestResult> {
    await delay();
    return {
      ok: input.provider.enabled,
      latencyMs: input.provider.enabled ? 68 : 0,
      modelCount: input.provider.models.length,
      error: input.provider.enabled ? null : "供应商未启用",
    };
  }

  async debugTestProvider(
    _providerId: string,
    _model: string,
    _message: string,
  ): Promise<string> {
    await delay();
    return "HTTP 200 (mock)\n\n{\"choices\":[{\"message\":{\"content\":\"Mock response\"}}]}";
  }


  async listHookEvents(): Promise<HookEventInfo[]> {
    await delay();
    return [
      "PreToolUse",
      "PostToolUse",
      "PostToolUseFailure",
      "UserPromptSubmit",
      "Stop",
      "PermissionRequest",
      "PermissionDenied",
    ].map((id) => ({ id, label: id, description: "mock hook 事件" }));
  }

  async getHooksConfig(_input: HooksScopeInput): Promise<HookConfigFile> {
    await delay();
    return clone(this.mockHooksConfig);
  }

  async saveHooksConfig(input: SaveHooksConfigInput): Promise<HookConfigFile> {
    await delay();
    this.mockHooksConfig = clone(input.config);
    return clone(this.mockHooksConfig);
  }

  async testHook(input: TestHookInput): Promise<HookTraceEntry> {
    await delay();
    return {
      event: input.event,
      hookId: input.hook.id ?? null,
      source: "global",
      matcher: input.matcher ?? null,
      command: input.hook.command,
      outcome: input.hook.command.trim() ? "success" : "error",
      durationMs: 12,
      exitCode: input.hook.command.trim() ? 0 : null,
      blockingReason: null,
      stdout: input.hook.command.trim() ? "mock hook ok" : "",
      stderr: input.hook.command.trim() ? "" : "命令为空",
    };
  }

  async getProjectHooksTrust(_projectPath: string): Promise<ProjectHooksTrust> {
    await delay();
    return { trusted: false };
  }

  async setProjectHooksTrust(
    _projectPath: string,
    trusted: boolean,
  ): Promise<ProjectHooksTrust> {
    await delay();
    return { trusted };
  }

  // ── MCP ─────────────────────────────────────────────────────────────────
  async listMcpServers(): Promise<McpServer[]> {
    await delay();
    return clone(MOCK_MCP_SERVERS);
  }

  async listMcpTools(name: string): Promise<McpToolInfo[]> {
    await delay();
    // Return mock tools — in real backend these come from the MCP connection.
    const server = MOCK_MCP_SERVERS.find((s) => s.name === name);
    if (!server) return [];
    return [
      {
        name: "echo",
        description: "Echo back the input text",
        inputSchema: {
          type: "object",
          properties: { text: { type: "string", description: "Text to echo" } },
          required: ["text"],
        },
      },
      {
        name: "add",
        description: "Add two numbers",
        inputSchema: {
          type: "object",
          properties: {
            a: { type: "number", description: "First number" },
            b: { type: "number", description: "Second number" },
          },
          required: ["a", "b"],
        },
      },
    ];
  }

  async restartMcpServer(_name: string): Promise<void> {
    await delay();
  }

  async toggleMcpServer(_name: string, _enabled: boolean): Promise<void> {
    await delay();
  }

  async mcpAddServer(_name: string, _config: unknown): Promise<void> {
    await delay();
  }

  async mcpRemoveServer(_name: string): Promise<void> {
    await delay();
  }

  async mcpReload(): Promise<void> {
    await delay();
  }

  // ── Skills ────────────────────────────────────────────────────────────────
  async skillList(_threadId?: string): Promise<Skill[]> {
    await delay();
    return clone(MOCK_SKILLS);
  }

  async skillRead(name: string, _threadId?: string, args?: string): Promise<string> {
    await delay();
    const skill = MOCK_SKILLS.find((s) => s.name === name);
    if (!skill) throw new Error(`unknown skill '${name}'`);
    return `# ${skill.name}\n\n${skill.description}\n\n(mock body${
      args ? ` · args: ${args}` : ""
    })`;
  }

  async skillReload(_threadId?: string): Promise<number> {
    await delay();
    return MOCK_SKILLS.length;
  }

  async skillDelete(_name: string): Promise<void> { return; }
  // ── Output Styles (Phase 2) ───────────────────────────────────────────────
  async listOutputStyles(): Promise<OutputStyle[]> {
    await delay();
    return clone(this.mockOutputStyles);
  }

  async readOutputStyle(name: string): Promise<string> {
    await delay();
    return this.mockOutputStyleBodies[name] ?? "";
  }

  async saveOutputStyle(name: string, content: string): Promise<void> {
    await delay();
    this.mockOutputStyleBodies[name] = content;
    if (!this.mockOutputStyles.some((s) => s.name === name)) {
      this.mockOutputStyles.push({ name, active: false });
    }
  }

  async setActiveOutputStyle(name: string | null): Promise<void> {
    await delay();
    this.mockOutputStyles = this.mockOutputStyles.map((s) => ({
      ...s,
      active: s.name === name,
    }));
  }

  async deleteOutputStyle(name: string): Promise<void> {
    await delay();
    this.mockOutputStyles = this.mockOutputStyles.filter((s) => s.name !== name);
    delete this.mockOutputStyleBodies[name];
  }

  // ── 长期记忆 ──────────────────────────────────────────────────────────────
  async readGlobalMemory(): Promise<string> {
    await delay();
    return "# Global Memory\n\nThis is the global AGENTS.md file. Edit it in Settings → Agent 指令.";
  }

  // ── Rewind (P2) ───────────────────────────────────────────────────────────
  async rewindThread(threadId: string, _messageSeq: number): Promise<Thread> {
    await delay();
    return this.getThread(threadId);
  }

  async listRewindPoints(_threadId: string): Promise<RewindPoint[]> {
    await delay();
    return [];
  }

  // ── Stats ───────────────────────────────────────────────────────────────
  async getUsageStats(input?: GetUsageStatsInput): Promise<UsageStats> {
    await delay();
    // Mock 不区分窗口数据,只把 windowLabel 透传以体现 UI 切换
    const stats = clone(MOCK_USAGE_STATS);
    stats.windowLabel = input?.window ?? stats.windowLabel;
    return stats;
  }

  async getUserBalance(
    _input?: GetUserBalanceInput,
  ): Promise<UserBalance | null> {
    await delay();
    return clone(MOCK_USER_BALANCE);
  }

  async getUsageChart(): Promise<UsageChartPoint[]> {
    await delay();
    return [];
  }

  async fsGetWorkspaceRoot(): Promise<string> {
    await delay();
    return "/mock/crown";
  }

  async fsListDirectory(path: string, showHidden = false): Promise<FsEntry[]> {
    await delay();
    const entries = MOCK_FS[path] ?? [];
    return entries
      .filter((entry) => showHidden || !entry.name.startsWith("."))
      .map(clone);
  }

  async fsReadFile(path: string, maxBytes?: number): Promise<FsFile> {
    await delay();
    const content = MOCK_FILE_CONTENT[path] ?? `// mock 文件预览\n${path}\n`;
    const cap = maxBytes ?? 256 * 1024;
    const truncated = content.length > cap;
    return {
      content: truncated ? content.slice(0, cap) : content,
      truncated,
      size: content.length,
      isBinary: false,
    };
  }
  async fsGrep(_pattern: string, _path?: string, _glob?: string, _maxResults?: number): Promise<GrepMatch[]> {
    return [];
  }

  async fsGlob(_pattern: string, _path?: string, _maxResults?: number): Promise<FsEntry[]> {
    await delay();
    return [];
  }

  async searchMessages(_query: string, _maxResults?: number): Promise<MessageSearchResult[]> {
    await delay();
    return [];
  }

  async ptySpawn(_input: PtySpawnInput): Promise<string> {
    await delay();
    return `mock-pty-${Date.now()}`;
  }

  async ptyList(): Promise<PtySession[]> {
    await delay();
    return [];
  }

  async ptySnapshot(ptyId: string): Promise<PtySnapshot> {
    await delay();
    return { ptyId, cwd: null, output: "" };
  }

  async ptyWrite(_ptyId: string, _data: string): Promise<void> {
    await delay();
  }

  async ptyResize(_ptyId: string, _cols: number, _rows: number): Promise<void> {
    await delay();
  }

  async ptyKill(_ptyId: string): Promise<void> {
    await delay();
  }

  async exportDiagnostics(): Promise<string> {
    await delay();
    return "/tmp/ds-agent-diagnostics-mock.zip";
  }

  // ── Permissions (P4 新增) ──────────────────────────────────────────────
  async listPermissionRules(_threadId: string): Promise<PermissionRule[]> {
    await delay();
    return [];
  }

  async removePermissionRule(
    _threadId: string,
    _rule: PermissionRule,
  ): Promise<void> {
    await delay();
  }

  async getPermissionContext(
    _threadId: string,
  ): Promise<ToolPermissionContextDto> {
    await delay();
    return {
      mode: "default",
      alwaysAllowRules: [],
      alwaysDenyRules: [],
      alwaysAskRules: [],
      additionalWorkingDirectories: [],
      isBypassPermissionsModeAvailable: false,
    };
  }

  async cyclePermissionMode(
    _input: CyclePermissionModeInput,
  ): Promise<CyclePermissionModeResult> {
    await delay();
    // Mock: 真后端会循环 default → acceptEdits → plan → bypassPermissions → default
    // 但 mock 无状态,永远返回 acceptEdits — 只影响 mock 模式.
    // hybrid 模式下走真后端,不会到这里.
    return { newMode: "acceptEdits" };
  }

  // ── Events (静态原型不发送任何事件) ─────────────────────────────────────
  onContentDelta(_cb: (e: ContentDeltaEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onReasoningDelta(_cb: (e: ReasoningDeltaEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onToolCallStart(_cb: (e: ToolCallStartEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onToolCallUpdate(_cb: (e: ToolCallUpdateEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onTurnComplete(_cb: (e: TurnCompleteEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onApprovalRequest(_cb: (e: ApprovalRequestEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onQuestionRequest(_cb: (e: QuestionRequestEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onStreamError(_cb: (e: StreamErrorEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onStreamAborted(_cb: (e: StreamAbortedEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onBudgetWarning(_cb: (e: BudgetWarningEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onModelEscalated(_cb: (e: ModelEscalatedEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onTodosUpdated(_cb: (e: TodosUpdatedEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onContextUsage(_cb: (e: ContextUsageEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormRunStarted(
    _cb: (e: BrainstormRunStartedEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormAgentStatus(
    _cb: (e: BrainstormAgentStatusEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormMessageStart(
    _cb: (e: BrainstormMessageStartEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormMessageDelta(
    _cb: (e: BrainstormMessageDeltaEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormReasoningDelta(
    _cb: (e: BrainstormReasoningDeltaEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormToolCallStart(
    _cb: (e: BrainstormToolCallStartEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormToolCallUpdate(
    _cb: (e: BrainstormToolCallUpdateEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormMessageDone(
    _cb: (e: BrainstormMessageDoneEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormRunDone(_cb: (e: BrainstormRunDoneEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onBrainstormError(_cb: (e: BrainstormErrorEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onMcpServerStatusChanged(
    _cb: (e: McpServerStatusChangedEvent) => void,
  ): Unsubscribe {
    return noopUnsub;
  }
  onMcpToolsChanged(_cb: (e: McpToolsChangedEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onPtyData(_cb: (e: PtyDataEvent) => void): Unsubscribe {
    return noopUnsub;
  }
  onPtyExit(_cb: (e: PtyExitEvent) => void): Unsubscribe {
    return noopUnsub;
  }
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function dir(path: string, name: string): FsEntry {
  return {
    name,
    path: `${path}/${name}`,
    isDir: true,
    size: 0,
    modifiedMs: Date.now(),
  };
}

function file(path: string, name: string, size = 1024): FsEntry {
  return {
    name,
    path: `${path}/${name}`,
    isDir: false,
    size,
    modifiedMs: Date.now(),
  };
}

const MOCK_FS: Record<string, FsEntry[]> = {
  "/mock/crown": [
    dir("/mock/crown", "crates"),
    dir("/mock/crown", "frontend"),
    file("/mock/crown", "Cargo.toml", 842),
    file("/mock/crown", "README.md", 4218),
  ],
  "/mock/crown/crates": [
    dir("/mock/crown/crates", "app"),
    dir("/mock/crown/crates", "core"),
    dir("/mock/crown/crates", "tools"),
  ],
  "/mock/crown/frontend": [
    dir("/mock/crown/frontend", "src"),
    file("/mock/crown/frontend", "package.json", 1388),
  ],
  "/mock/crown/frontend/src": [
    file("/mock/crown/frontend/src", "main.tsx", 512),
    file("/mock/crown/frontend/src", "App.tsx", 936),
  ],
};

const MOCK_FILE_CONTENT: Record<string, string> = {
  "/mock/crown/README.md":
    "# Crown\n\n这是 mock 模式下的文件预览，占位用来保持原型可运行。\n",
  "/mock/crown/Cargo.toml":
    '[workspace]\nmembers = ["crates/app", "crates/core", "crates/tools"]\n',
  "/mock/crown/frontend/package.json":
    '{\n  "name": "crown-frontend",\n  "private": true\n}\n',
};
