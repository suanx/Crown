/**
 * ============================================================================
 * Contract Status — 对接进度看板的真相源
 * ============================================================================
 *
 * 每个 IPC 端点的对接状态.AI / 人类都可以通过它一眼看出:
 *   - 哪些还没对接 (mock)
 *   - 哪些已经能用但未联调 (connected)
 *   - 哪些已经验证 (verified)
 *
 * HybridClient 根据这里决定调 mock 还是 tauri,
 * DevtoolsPanel 根据这里展示进度.
 *
 * ⚠ 加新端点时此 map 必须更新,否则 TS 编译报错.
 * ----------------------------------------------------------------------------
 */

import {
  COMMAND_KEYS,
  EVENT_KEYS,
  type CommandKey,
  type EventKey,
  type EndpointKey,
} from "./AgentClient";

export type ContractStatus =
  /** 后端没做,纯 mock — 调用时 devtools 静默记录. */
  | "mock"
  /** 后端已实现,优先调真后端,失败 fallback mock. */
  | "connected"
  /** 已联调验证 — 静默通过,仅在 prod 也启用真后端. */
  | "verified";

/**
 * 当前对接状态 (P4 IPC 集成完成后).
 *
 * 后端 deepseek-agent commit e1a8eda 已实化全部 22 commands + 10 events,
 * BACKEND_REPLY_2026-05-28.md 第 2 节验收通过 — 18 commands + 10 events
 * 翻 "connected", 4 个 long-term mock 保持 "mock".
 *
 * 升级路径: connected (HybridClient: real 优先 + 失败 fallback mock + warn)
 *           → verified (HybridClient: 仅 real, 失败让错误冒泡).
 *
 * 翻 verified 的前提是联调 10 步场景(plan §7.2)逐个端点
 * DevtoolsPanel source = "real-ok" 且 0 shape mismatch.
 *
 * ⚠ 长期 mock (协议 §4.3): list/restart/toggle MCP —
 * P4 后端是 [] / Err("NotImplemented"), 翻 connected 会让 DevtoolsPanel
 * 误染红, 等 P5+ 真实现再翻.
 */
export const CONTRACT_STATUS: Record<EndpointKey, ContractStatus> = {
  // ── Commands ────────────────────────────────────────────────────────────
  // Threads (7) — 全部 P4 实化
  listThreads: "connected",
  getThread: "connected",
  createThread: "connected",
  updateThread: "connected",
  deleteThread: "connected",
  searchThreads: "connected",
  exportThread: "connected",
  getUsageChart: "connected",
  // Projects — 后端 projects 表 + thread.project_id 真实持久化。
  listProjects: "connected",
  pickProjectDirectory: "connected",
  createProject: "connected",
  updateProject: "connected",
  deleteProject: "connected",
  // Conversation (3) — 全部 P4 实化
  sendMessage: "connected",
  startBrainstorm: "connected",
  continueBrainstorm: "connected",
  stopBrainstorm: "connected",
  abortTurn: "connected",
  approveTool: "connected",
  submitAnswers: "connected",
  // Models / Config (4) — 全部 P4 实化(setConfig 不持久化但形态对, P3 补行为)
  listModels: "connected",
  switchModel: "connected",
  getConfig: "connected",
  setConfig: "connected",
  saveProviders: "connected",
  saveWebSearchConfig: "connected",
  fetchProviderModels: "connected",
  testProviderConnection: "connected",
  debugTestProvider: "connected",
  debugTestStream: "connected",
  listHookEvents: "connected",
  getHooksConfig: "connected",
  saveHooksConfig: "connected",
  testHook: "connected",
  getProjectHooksTrust: "connected",
  setProjectHooksTrust: "connected",
  // MCP — 后端 deepseek-mcp 真实现 (rmcp), commit 5bce31b 起全链路验证.
  // list/toggle/restart 接真后端; add/remove/reload 是新命令.
  listMcpServers: "connected",
  listMcpTools: "connected",
  restartMcpServer: "connected",
  toggleMcpServer: "connected",
  mcpAddServer: "connected",
  mcpRemoveServer: "connected",
  mcpReload: "connected",
  // Skills — 后端 deepseek-skill 真实现, skill_list/read/reload 接真后端.
  skillList: "connected",
  skillRead: "connected",
  skillReload: "connected",
  skillDelete: "connected",
  // Output Styles (Phase 2) — 后端 output_styles 命令真实现.
  listOutputStyles: "connected",
  readOutputStyle: "connected",
  saveOutputStyle: "connected",
  setActiveOutputStyle: "connected",
  deleteOutputStyle: "connected",
  // 长期记忆 — 后端 read_global_memory 真实现.
  readGlobalMemory: "connected",
  writeGlobalMemory: "connected",
  // Rewind (P2) — 后端 rewind_thread/list_rewind_points 真实现.
  rewindThread: "connected",
  listRewindPoints: "connected",
  // Stats — getUsageStats P3a task 6 接通真后端 (UsageRepo 多窗口聚合);
  // getUserBalance P3a task 7 接 DeepSeek /user/balance,失败返 null.
  getUsageStats: "connected",
  getUserBalance: "connected",
  // Filesystem — 后端 UI 专用 fs 命令，真实文件树/预览用。
  fsGetWorkspaceRoot: "connected",
  fsListDirectory: "connected",
  fsReadFile: "connected",
  fsGrep: "connected",
  fsGlob: "connected",
  searchMessages: "connected",
  // 终端 PTY — 后端 portable-pty 真实会话 + pty:data/pty:exit 事件。
  ptyList: "connected",
  ptySnapshot: "connected",
  ptySpawn: "connected",
  ptyWrite: "connected",
  ptyResize: "connected",
  ptyKill: "connected",
  exportDiagnostics: "connected",
  polishPrompt: "connected",
  // Permissions P4 新增 (3) — 全部实化
  listPermissionRules: "connected",
  removePermissionRule: "connected",
  getPermissionContext: "connected",
  cyclePermissionMode: "connected",
  // ── Events (10) — 全部 P4 emit ─────────────────────────────────────────
  onContentDelta: "connected",
  onReasoningDelta: "connected",
  onToolCallStart: "connected",
  onToolCallUpdate: "connected",
  onTurnComplete: "connected",
  onApprovalRequest: "connected",
  onQuestionRequest: "connected",
  onStreamError: "connected",
  onStreamAborted: "connected",
  // 注: onBudgetWarning / onModelEscalated 按协议 §5 P4 永远不 emit
  // (P3 实现 cost/escalation 时才会触发 callback). listen 订阅本身形态
  // 正确, 后端 e1a8eda 已 wire 事件名 → connected 状态合理(只是 callback
  // 在 P4 期间永远不会被调用, 不影响其他端点验证).
  onBudgetWarning: "connected",
  onModelEscalated: "connected",
  // P6: todo_write 工具产物,后端 stream:todos_updated 已 emit + 有 shape 测试
  onTodosUpdated: "connected",
  onContextUsage: "connected",
  onBrainstormRunStarted: "connected",
  onBrainstormAgentStatus: "connected",
  onBrainstormMessageStart: "connected",
  onBrainstormMessageDelta: "connected",
  onBrainstormReasoningDelta: "connected",
  onBrainstormToolCallStart: "connected",
  onBrainstormToolCallUpdate: "connected",
  onBrainstormMessageDone: "connected",
  onBrainstormRunDone: "connected",
  onBrainstormError: "connected",
  // MCP 事件 — 后端 dispatch_mcp_status / dispatch_mcp_tools_changed 已 wire.
  onMcpServerStatusChanged: "connected",
  onMcpToolsChanged: "connected",
  onPtyData: "connected",
  onPtyExit: "connected",
};

// ── 派生统计 ──────────────────────────────────────────────────────────────

export interface ContractStats {
  total: number;
  byStatus: Record<ContractStatus, number>;
  pctConnected: number; // (connected + verified) / total
}

export function computeContractStats(): ContractStats {
  const byStatus: Record<ContractStatus, number> = {
    mock: 0,
    connected: 0,
    verified: 0,
  };
  for (const key of Object.keys(CONTRACT_STATUS) as EndpointKey[]) {
    byStatus[CONTRACT_STATUS[key]]++;
  }
  const total = Object.keys(CONTRACT_STATUS).length;
  const ready = byStatus.connected + byStatus.verified;
  return {
    total,
    byStatus,
    pctConnected: total === 0 ? 0 : ready / total,
  };
}

export const ALL_COMMAND_KEYS: readonly CommandKey[] = COMMAND_KEYS;
export const ALL_EVENT_KEYS: readonly EventKey[] = EVENT_KEYS;
