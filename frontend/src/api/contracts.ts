/**
 * ============================================================================
 * IPC Contract v2 — DeepSeek Agent (Claude Code 对齐版)
 * ============================================================================
 *
 * 任何字段调整必须同步本文件与 Rust DTO.
 *
 * 核心约定:
 *   1. 全栈 camelCase. Rust struct: #[serde(rename_all = "camelCase")]
 *   2. 可空字段: Rust Option<T>,序列化为显式 null,不要 skip_serializing_if
 *   3. 时间戳: RFC3339 字符串
 *   4. 金额: f64 USD; token / 字节计数: u64 (< 2^53)
 *   5. id 命名: toolUseId (不是 toolCallId / toolId,对齐 Claude tool_use_id)
 *   6. 工具入参字段: input (不是 args / arguments,对齐 Claude tool_use.input)
 * ----------------------------------------------------------------------------
 */

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ 基础枚举                                                                  │
// └─────────────────────────────────────────────────────────────────────────┘

export type Role = "user" | "assistant" | "system" | "tool";

/**
 * 权限模式 — 完整 5 值 (对齐 Claude src/types/permissions.ts:PERMISSION_MODES).
 *
 * UI label 见 shared/lib/permissionMode.ts (default→Agent / plan→Plan /
 * acceptEdits→Auto-Edit / bypassPermissions→YOLO / dontAsk→Strict).
 *
 * P4 阶段 ComposerModeSelector 仅暴露三档:default / plan / bypassPermissions.
 * acceptEdits / dontAsk 类型必须支持 (后端可推),但暂不暴露切换 UI.
 */
export type PermissionMode =
  | "default"
  | "plan"
  | "acceptEdits"
  | "bypassPermissions"
  | "dontAsk";

export type ToolStatus =
  | "pending_approval"
  | "running"
  | "success"
  | "error"
  | "aborted";

export type ToolName =
  | "read_file"
  | "view_file"
  | "list_directory"
  | "list_dir"
  | "write_file"
  | "write_to_file"
  | "edit_file"
  | "replace_file_content"
  | "multi_replace_file_content"
  | "run_command"
  | "web_search"
  | "web_fetch"
  | "grep" // P6:正则内容搜索（取代旧 search_content）
  | "grep_search"
  | "glob" // P6:文件查找（取代旧 search_files）
  | "todo_write" // P6:任务列表
  | "skill" // 技能调用
  | "task" // P4:子代理委派
  | "ask_user_question" // EPIC 1:结构化澄清提问
  | "mcp_tool"
  | string;

/**
 * 任务清单 — todo_write 工具产物.
 * stream:todos_updated 整列表全量替换,不做增量 merge.
 */
export type TodoStatus = "pending" | "in_progress" | "completed";

export interface TodoItem {
  /** 祈使句,pending/completed 时显示 —— "运行测试". */
  content: string;
  /** 进行时,in_progress 时显示 —— "正在运行测试". */
  activeForm: string;
  status: TodoStatus;
}

export type ThemeMode = "light" | "dark" | "system";

export type ThinkingEffort = "low" | "medium" | "high" | "ultra";

/**
 * MCP server 状态 — 对齐后端 `deepseek_mcp::types::ServerStatus`
 * (snake_case serde).
 *   - connected:  已连接,工具可用
 *   - pending:    连接中 / 重连中
 *   - disabled:   被用户禁用 (config.disabled = true)
 *   - failed:     连接失败 (看 errorMessage)
 *   - needs_auth: 需要授权 (远程 server OAuth,P5+)
 */
export type McpServerStatus =
  | "connected"
  | "pending"
  | "disabled"
  | "failed"
  | "needs_auth";

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ 权限相关结构 (Claude 对齐)                                                │
// └─────────────────────────────────────────────────────────────────────────┘

export type PermissionBehavior = "allow" | "deny" | "ask";

/**
 * 权限规则源.
 * P4 仅支持 "session". 其他值后端可能 emit (前端必须能解析,UI 可暂不区分渲染).
 */
export type PermissionRuleSource =
  | "session"
  | "userSettings"
  | "projectSettings"
  | "localSettings"
  | "cliArg"
  | "policySettings"
  | "flagSettings"
  | "command";

/**
 * 单条规则的匹配值. P4 永远 ruleContent: null (仅 toolName 颗粒度).
 * Claude 用 ruleContent 做 Bash(git status:*) 这类细粒度匹配.
 */
export interface PermissionRuleValue {
  toolName: string;
  ruleContent: string | null;
}

export interface PermissionRule {
  source: PermissionRuleSource;
  ruleBehavior: PermissionBehavior;
  ruleValue: PermissionRuleValue;
}

/**
 * 决策原因 — discriminated union by `type`.
 * P4 后端只 emit "rule" | "mode" | "other" | "workingDir" 四种.
 * 其他 type 留作 Roadmap 占位,前端必须能解析未知 type → fallback 渲染.
 */
export type DecisionReason =
  | { type: "rule"; rule: PermissionRule }
  | { type: "mode"; mode: PermissionMode }
  | { type: "hook"; hookName: string; reason: string | null }
  | { type: "safetyCheck"; reason: string; classifierApprovable: boolean }
  | { type: "workingDir"; reason: string }
  | { type: "other"; reason: string }
  | { type: "asyncAgent"; reason: string }
  | {
      type: "subcommandResults";
      reasons: Array<[string, PermissionResult]>;
    }
  | { type: "permissionPromptTool"; permissionPromptToolName: string }
  | { type: "sandboxOverride" }
  | { type: "classifier"; classifier: string; reason: string };

/**
 * 权限决策结果 — discriminated union by `behavior`.
 * `passthrough` 仅在 Rust 端工具自身 check_permissions 返回时使用,
 * 主决策流转换为 `ask` 后才送上 IPC,前端实际不会收到 passthrough.
 */
export type PermissionResult =
  | {
      behavior: "allow";
      updatedInput: Record<string, unknown>;
      decisionReason?: DecisionReason;
      userModified?: boolean;
    }
  | {
      behavior: "deny";
      message: string;
      decisionReason?: DecisionReason;
    }
  | {
      behavior: "ask";
      message: string;
      decisionReason?: DecisionReason;
      suggestions?: PermissionUpdate[];
    }
  | {
      behavior: "passthrough";
      message: string;
    };

/**
 * 权限更新指令 — "Allow always"/批量管理规则的载体,discriminated by `type`.
 * P4 后端只接受 type="addRules" + destination="session",其他类型 log 警告并忽略.
 */
export type PermissionUpdate =
  | {
      type: "addRules";
      rules: PermissionRuleValue[];
      behavior: PermissionBehavior;
      destination: PermissionRuleSource;
    }
  | {
      type: "replaceRules";
      rules: PermissionRuleValue[];
      behavior: PermissionBehavior;
      destination: PermissionRuleSource;
    }
  | {
      type: "removeRules";
      rules: PermissionRuleValue[];
      behavior: PermissionBehavior;
      destination: PermissionRuleSource;
    }
  | {
      type: "setMode";
      mode: PermissionMode;
      destination: PermissionRuleSource;
    }
  | {
      type: "addDirectories";
      directories: string[];
      destination: PermissionRuleSource;
    }
  | {
      type: "removeDirectories";
      directories: string[];
      destination: PermissionRuleSource;
    };

/**
 * 用户对单次审批请求的决策 — 对齐 Claude PermissionDecision.
 *
 *  - "Allow once"   → { behavior:"allow", updatedInput=原 input, permissionUpdates: [] }
 *  - "Allow always" → { behavior:"allow", updatedInput=原 input, permissionUpdates: [{
 *                       type:"addRules", rules:[{toolName, ruleContent:null}],
 *                       behavior:"allow", destination:"session" }] }
 *  - "Deny"         → { behavior:"deny", message: null }    P4 永远 null
 *
 * Esc / 关闭弹窗 ≠ Deny,改为 invoke abortTurn (见 ApproveToolInput JSDoc).
 */
export type ApproveToolDecision =
  | {
      behavior: "allow";
      updatedInput: Record<string, unknown>;
      permissionUpdates: PermissionUpdate[];
    }
  | {
      behavior: "deny";
      message: string | null;
    };

/**
 * 工具权限上下文 — listPermissionRules / getPermissionContext 返回值.
 * 字段对齐 Claude Tool.ts:ToolPermissionContext.
 *
 * P4 阶段:
 *  - mode + alwaysAllowRules 实际填充
 *  - alwaysDenyRules / alwaysAskRules / additionalWorkingDirectories 永远空数组
 *  - isBypassPermissionsModeAvailable 永远 false (Roadmap GAP-PERM-007)
 *
 * 多预留字段不渲染,后端逐步填充时不需要再改 contracts.ts.
 */
export interface ToolPermissionContextDto {
  mode: PermissionMode;
  alwaysAllowRules: PermissionRule[];
  alwaysDenyRules: PermissionRule[];
  alwaysAskRules: PermissionRule[];
  additionalWorkingDirectories: string[];
  isBypassPermissionsModeAvailable: boolean;
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ 数据结构                                                                 │
// └─────────────────────────────────────────────────────────────────────────┘

/**
 * 工具调用记录.
 *
 * id 即 tool_use_id (Claude SDK 用 id 字段,IPC 边界则一律 toolUseId).
 *
 * @rust crates/core/src/types.rs::ToolCall
 */
export interface ToolCall {
  id: string;
  name: ToolName;
  /** 对齐 Claude tool_use.input,不是 args / arguments. */
  input: Record<string, unknown>;
  status: ToolStatus;
  result: string | null;
  durationMs: number | null;
  diff: ToolDiff | null;
  errorMessage: string | null;
  /**
   * 子代理活动 (P4) — 仅 name === "task" 的卡片有。带 agentId 的 stream 事件
   * 被 chatStore 聚合到这里,供 ToolCallCard 展开显示子代理的嵌套工具调用。
   */
  subAgent?: SubAgentActivity | null;
}

/**
 * 一个子代理的嵌套活动 (P4)。挂在 task 工具卡片上。
 */
export interface SubAgentActivity {
  /** 子代理 thread id (= 事件 agentId)。 */
  agentId: string;
  /** 子代理流式产出的文本 (拼接 contentDelta)。 */
  text: string;
  /** 子代理的工具调用 (嵌套卡片)。 */
  toolCalls: ToolCall[];
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ Segments — 交错渲染模型 (对齐 Reasonix)                                    │
// └─────────────────────────────────────────────────────────────────────────┘

export interface TextSegment {
  kind: "text";
  text: string;
}

export interface ReasoningSegment {
  kind: "reasoning";
  text: string;
}

export interface ToolSegment {
  kind: "tool";
  callId: string;
  name: ToolName;
  input: Record<string, unknown>;
  status: ToolStatus;
  result?: string;
  durationMs?: number;
  diff?: ToolDiff | null;
  errorMessage?: string;
  /** 子代理活动 (P4) — 仅 name === "task" 的段有。 */
  subAgent?: SubAgentActivity | null;
}

export type Segment = TextSegment | ReasoningSegment | ToolSegment;

export interface ToolDiff {
  path: string;
  before: string;
  after: string;
}

/**
 * 单条消息.
 *
 * segments[] 是按实际时序交错的渲染单元 (text/reasoning/tool).
 * content/reasoning/toolCalls 仅在后端 getThread 返回旧格式时暂存,
 * loadThread 时通过 legacyToSegments 转写到 segments.
 *
 * @rust crates/core/src/types.rs::Message
 */
export interface Message {
  id: string;
  threadId: string;
  /** Sequence within the thread (0-based). Present on persisted messages
   *  (getThread); absent on optimistic local/streaming messages. Used by
   *  rewind to target a position. */
  seq?: number;
  role: Role;
  /** @deprecated 由 segments 取代,仅做 legacy 兼容解析用. */
  content: string;
  timestamp: string;
  /** @deprecated 由 segments 取代. */
  reasoning: string | null;
  /** @deprecated 由 segments 取代. */
  toolCalls: ToolCall[] | null;
  /** 交错渲染段落 — 渲染层唯一数据源. */
  segments: Segment[];
  usage: MessageUsage | null;
  isStreaming: boolean;
  interrupted: boolean;
  brainstorm?: BrainstormMessageMeta | null;
  /** File attachment names sent with this message. */
  attachments?: string[];
}

export interface BrainstormParticipant {
  id: string;
  name: string;
  role: string;
  color: string;
}

export interface BrainstormMessageMeta {
  runId: string;
  messageId: string;
  participant: BrainstormParticipant;
}

/**
 * 回溯点 — 一条用户消息对应一个可回溯位置 (P2).
 * 对齐后端 `RewindPointDto`.
 */
export interface RewindPoint {
  messageSeq: number;
  preview: string;
  filesChanged: number;
}

export interface MessageUsage {
  /**
   * 命中前缀缓存的输入 tokens (折扣价).
   * Provider 映射:
   *   DeepSeek    prompt_cache_hit_tokens
   *   OpenAI      cached_tokens
   *   Anthropic   cache_read_input_tokens
   */
  cacheReadTokens: number;
  /** 真正未缓存的输入 tokens (全价). 总 input = cacheRead + cacheMiss + cacheCreation. */
  cacheMissTokens: number;
  /**
   * 写入缓存的 tokens. **Anthropic-only** (cache_creation_input_tokens),
   * DeepSeek / OpenAI 永远 0,但前端必须能解析显式 0.
   */
  cacheCreationTokens: number;
  outputTokens: number;
  /** 后端 P3a task 4 接通真值,task 3 阶段仍为 0. */
  costUsd: number;
}

/**
 * 会话摘要 (列表用,不含完整消息).
 *
 * @rust crates/core/src/types.rs::ThreadSummary
 */
export interface ThreadSummary {
  id: string;
  title: string;
  updatedAt: string;
  messageCount: number;
  isStreaming: boolean;
  isPinned: boolean;
  preview: string | null;
  /** 关联的项目 id;无项目时为 null. */
  projectId: string | null;
  /**
   * Provider 标识 (P3a task 2 加).后端默认 "deepseek",未来支持
   * openai / anthropic / 自托管时切换.前端可在 sidebar 显示 provider 徽章,
   * 不读也无害(JSON 多字段不影响 deserialization).
   */
  providerId: string;
}

/**
 * 完整会话 + 全部消息.
 *
 * @rust crates/core/src/types.rs::Thread
 */
export interface Thread {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  model: string;
  thinkingEffort: ThinkingEffort;
  projectId?: string | null;
  cwd?: string | null;
  /** 与 ThreadSummary.providerId 同步.P3a task 2 加,缺省 "deepseek". */
  providerId: string;
  /**
   * Per-thread 权限模式 (与 Claude state.ts switchSession 行为一致 — 模式跟着
   * thread 走,不跟着 app 走). ComposerModeSelector 切换调 updateThread.
   */
  permissionMode: PermissionMode;
  costUsd: number;
  messages: Message[];
}

/**
 * 项目摘要.
 *
 * @rust crates/core/src/types.rs::ProjectSummary
 */
export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  threadCount: number;
  lastUsedAt: string;
}

export interface FsEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modifiedMs: number;
}

export interface FsFile {
  content: string;
  truncated: boolean;
  size: number;
  isBinary: boolean;
}

export interface PtySpawnInput {
  cwd?: string | null;
  cols: number;
  rows: number;
}

export interface PtySession {
  ptyId: string;
  cwd: string | null;
}

export interface PtySnapshot {
  ptyId: string;
  cwd: string | null;
  output: string;
}

export interface PtyDataEvent {
  ptyId: string;
  data: string;
}

export interface PtyExitEvent {
  ptyId: string;
  code: number | null;
}

export interface ModelInfo {
  id: string;
  label: string;
  description: string;
  pricePerMillionInputUsd: number;
  pricePerMillionOutputUsd: number;
  pricePerMillionCacheHitUsd: number;
  contextWindow: number;
  /** Provider 标识 (P3a task 2 加).后端缺省 "deepseek". */
  providerId: string;
}

export type ProviderKind =
  | "deepseek"
  | "openai"
  | "openai-compatible"
  | "anthropic"
  | "ollama";

export interface ProviderModel {
  id: string;
  label: string;
  enabled: boolean;
}

export interface ProviderConfig {
  id: string;
  name: string;
  providerType: ProviderKind;
  baseUrl: string;
  apiKey: string | null;
  apiKeyPresent: boolean;
  enabled: boolean;
  models: ProviderModel[];
}

export interface SaveProvidersInput {
  providers: ProviderConfig[];
  defaultProviderId: string;
  defaultModel: string;
}

export interface WebSearchProviderConfig {
  id: string;
  name: string;
  apiKey: string | null;
  apiKeyPresent: boolean;
  enabled: boolean;
  implemented: boolean;
  keyRequired: boolean;
  note: string | null;
}

export interface WebSearchConfig {
  defaultProviderId: string;
  providers: WebSearchProviderConfig[];
}

export interface SaveWebSearchConfigInput {
  defaultProviderId: string;
  providers: WebSearchProviderConfig[];
}

export interface ProviderModelsInput {
  provider: ProviderConfig;
}

export interface ProviderTestResult {
  ok: boolean;
  latencyMs: number;
  modelCount: number;
  error: string | null;
}

export interface McpServer {
  name: string;
  command: string;
  args: string[];
  status: McpServerStatus;
  enabled: boolean;
  toolCount: number;
  errorMessage: string | null;
}

/** A single tool exposed by an MCP server, with full input schema. */
export interface McpToolInfo {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/** Skill 作用域 — 全局 (跨项目) 或项目 (随 thread.cwd). */
export type SkillScope = "global" | "project";

/**
 * Skill 来源格式:
 *   - native: 本系统原生目录 (<data>/deepseek-agent/skills 或 <cwd>/.deepseek/skills)
 *   - claude: 兼容读 Claude 目录 (~/.claude/skills 或 <cwd>/.claude/skills)
 *   - mcp:    来自 MCP server 的 prompt (作为伪 skill,P5+)
 */
export type SkillSource = "native" | "claude" | "mcp";

/**
 * 一个已发现的 Skill — 对齐后端 `SkillDto`.
 * 渐进式披露:列表只带 name + description,正文按需经 skillRead 加载.
 */
export interface Skill {
  name: string;
  description: string;
  scope: SkillScope;
  source: SkillSource;
  /** SKILL.md 的绝对路径. */
  path: string;
  /** frontmatter allowed-tools (按空白拆分),可能为空数组. */
  allowedTools: string[];
}

/**
 * 一个输出风格 — 对齐后端 `OutputStyleDto`.
 * 正文存 `<data_root>/output-styles/<name>.md`，按需经 readOutputStyle 加载。
 */
export interface OutputStyle {
  name: string;
  /** 是否为当前生效的风格。 */
  active: boolean;
}

/**
 * 用量统计时间窗 (P3a task 5).
 * P3a task 6 后端按窗口聚合.前端在 BillingPanel 切换下拉.
 */
export type UsageStatsWindow =
  | "session"
  | "today"
  | "7d"
  | "30d"
  | "lifetime";

export interface GetUsageStatsInput {
  window?: UsageStatsWindow;
}

export interface UsageStats {
  totalCostUsd: number;
  /**
   * 缓存命中累计省下的金额 (USD).P3a task 6 起后端用 (cacheRead × full price -
   * cacheRead × cached price) 累加,DeepSeek 卖点 UI 直接读这条.
   */
  cumulativeCacheSavedUsd: number;
  cacheReadTokens: number;
  cacheMissTokens: number;
  cacheCreationTokens: number;
  outputTokens: number;
  cacheHitRatio: number;
  /** 本响应对应的窗口 (与请求时 window 一致). */
  windowLabel: UsageStatsWindow;
  /** P3a 永远 null (预算模块不在 P3a 范围). */
  budgetLimitUsd: number | null;
  /** P3a 永远 null. */
  budgetUsedPct: number | null;
}
export interface UsageChartPoint {
  dayEpochMs: number;
  cacheReadTokens: number;
  cacheMissTokens: number;
  outputTokens: number;
  totalCostUsd: number;
}



// ┌─────────────────────────────────────────────────────────────────────────┐
// │ User Balance (P3a task 7)                                               │
// └─────────────────────────────────────────────────────────────────────────┘

export interface GetUserBalanceInput {
  /** P3a 仅 "deepseek" 实化,默认即可省略. */
  providerId?: string;
}

/**
 * 单个币种的余额信息.
 * granted / toppedUp 在不支持 (e.g. OpenAI) 或后端无法区分时为 null.
 */
export interface BalanceInfo {
  currency: string;
  total: number;
  granted: number | null;
  toppedUp: number | null;
}

/**
 * 用户余额 — DeepSeek /user/balance 等价物.
 *
 * `getUserBalance` 失败 (网络 / API key 错 / 不支持 provider) 时后端**返回 null**
 * 而非抛异常.UI 应隐藏 Balance cell,不显错.
 */
export interface UserBalance {
  isAvailable: boolean;
  primaryCurrency: string;
  balanceInfos: BalanceInfo[];
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ Hooks                                                                    │
// └─────────────────────────────────────────────────────────────────────────┘

export type HookScope = "global" | "project";

export type HookCommandType = "command" | "prompt" | "agent" | "http";

export interface HookCommandConfig {
  id?: string | null;
  type: HookCommandType;
  command: string;
  shell?: string | null;
  timeout?: number | null;
  enabled: boolean;
}

export interface HookMatcherConfig {
  matcher?: string | null;
  hooks: HookCommandConfig[];
}

export interface HookConfigFile {
  disableAllHooks: boolean;
  trustedProjects: string[];
  hooks: Record<string, HookMatcherConfig[]>;
}

export interface HookEventInfo {
  id: string;
  label: string;
  description: string;
}

export interface HooksScopeInput {
  scope: HookScope;
  projectPath?: string | null;
}

export interface SaveHooksConfigInput extends HooksScopeInput {
  config: HookConfigFile;
}

export interface TestHookInput {
  event: string;
  matcher?: string | null;
  cwd?: string | null;
  hook: HookCommandConfig;
  input?: Record<string, unknown>;
}

export interface HookTraceEntry {
  event: string;
  hookId: string | null;
  source: "global" | "project";
  matcher: string | null;
  command: string;
  outcome: string;
  durationMs: number;
  exitCode: number | null;
  blockingReason: string | null;
  stdout: string;
  stderr: string;
}

export interface ProjectHooksTrust {
  trusted: boolean;
}

export interface AppConfig {
  apiKeyPresent: boolean;
  baseUrl: string;
  defaultProviderId: string;
  defaultModel: string;
  providers: ProviderConfig[];
  webSearch: WebSearchConfig;
  /** 全局默认权限模式 — 影响新建 thread 的初始值. 切换当前 thread 用 updateThread. */
  permissionMode: PermissionMode;
  theme: ThemeMode;
  language: "zh" | "en";
  budget: {
    mode: "per_session" | "per_day" | "unlimited";
    limitUsd: number | null;
  };
  compaction: {
    triggerRatio: number;
    keepRecentTurns: number;
  };
  shell: {
    timeoutSecs: number;
    maxOutputBytes: number;
  };
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ Command Inputs                                                           │
// └─────────────────────────────────────────────────────────────────────────┘

export interface SendMessageInput {
  threadId: string;
  content: string;
  attachments: string[];
}

export interface StartBrainstormInput {
  threadId: string;
  topic: string;
  rounds?: number;
}

export interface ContinueBrainstormInput {
  threadId: string;
  runId: string;
  prompt: string;
}

export interface StartBrainstormResult {
  runId: string;
}

/**
 * Esc / 关闭审批 dialog ≠ Deny.
 * 用户中止时,前端调 abortTurn(threadId),后端 emit stream:aborted,
 * 不通过 approveTool 路径.
 */
export interface ApproveToolInput {
  threadId: string;
  /** 对齐 Claude tool_use_id. */
  toolUseId: string;
  decision: ApproveToolDecision;
}

/**
 * 更新 thread 元信息.
 * permissionMode 切换走这里 (per-thread),不走 setConfig.
 */
export interface UpdateThreadInput {
  threadId: string;
  title?: string;
  isPinned?: boolean;
  permissionMode?: PermissionMode;
  thinkingEffort?: ThinkingEffort;
  projectId?: string | null;
}

export interface CreateThreadInput {
  projectId?: string | null;
  cwd?: string | null;
  model?: string;
  providerId?: string;
  thinkingEffort?: ThinkingEffort;
}

export interface CreateProjectInput {
  name: string;
  path: string;
}

export interface UpdateProjectInput {
  projectId: string;
  name?: string;
  path?: string;
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ Permission Cycle (P4b)                                                  │
// └─────────────────────────────────────────────────────────────────────────┘

export interface CyclePermissionModeInput {
  threadId: string;
}

export interface CyclePermissionModeResult {
  newMode: PermissionMode;
}

export interface ConfigPatch {
  apiKey?: string;
  baseUrl?: string;
  defaultModel?: string;
  /**
   * 全局默认 — 影响未来新建的 thread (写到 ~/.config/deepseek-agent/config.toml).
   * 当前 thread 切换不走这里,走 updateThread.
   */
  permissionMode?: PermissionMode;
  theme?: ThemeMode;
  language?: "zh" | "en";
  budget?: AppConfig["budget"];
  compaction?: AppConfig["compaction"];
  shell?: AppConfig["shell"];
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ Event Payloads                                                           │
// └─────────────────────────────────────────────────────────────────────────┘

export interface ContentDeltaEvent {
  threadId: string;
  messageId: string;
  delta: string;
  /** 子代理 thread id (P4);主代理为 null/缺省。前端据此把子代理活动嵌套到 task 卡片下。 */
  agentId?: string | null;
}

export interface ReasoningDeltaEvent {
  threadId: string;
  messageId: string;
  delta: string;
  agentId?: string | null;
}

export interface ToolCallStartEvent {
  threadId: string;
  messageId: string;
  toolCall: ToolCall;
  agentId?: string | null;
}

export interface ToolCallUpdateEvent {
  threadId: string;
  messageId: string;
  /** 对齐 Claude tool_use_id (从 v1 toolCallId 重命名). */
  toolUseId: string;
  status: ToolStatus;
  /**
   * 工具入参.后端会在流式参数增长期间尽力发送可提取到的 partial input,
   * 流结束后再用完整解析结果回填.状态变更 update 可为 null,前端保留已有 input.
   */
  input: Record<string, unknown> | null;
  result: string | null;
  diff: ToolDiff | null;
  durationMs: number | null;
  errorMessage: string | null;
  agentId?: string | null;
}

export interface TurnCompleteEvent {
  threadId: string;
  messageId: string;
  usage: MessageUsage;
  agentId?: string | null;
}

/**
 * 审批请求事件 — 后端 Permission Gate 决策 ask 时 emit.
 * 包含完整 PermissionResult.ask,前端 ApprovalDialog 直接基于此渲染.
 */
export interface ApprovalRequestEvent {
  threadId: string;
  /** 对齐 Claude tool_use_id (从 v1 toolCallId 重命名). */
  toolUseId: string;
  toolName: ToolName;
  /** 对齐 Claude tool_use.input (从 v1 args 重命名). */
  input: Record<string, unknown>;
  /** 后端 tool.description(input) 的人类可读字符串. ApprovalDialog 标题区直接用. */
  description: string;
  cwd: string | null;
  permissionResult: Extract<PermissionResult, { behavior: "ask" }>;
}

/** 单个问题选项（对齐后端 QuestionOption）。 */
export interface QuestionOptionDto {
  label: string;
  description: string;
}

/** 一道题（对齐后端 Question，camelCase）。 */
export interface QuestionDto {
  header: string;
  question: string;
  multiSelect: boolean;
  options: QuestionOptionDto[];
}

/**
 * 结构化问答请求事件 — agent 调 ask_user_question 时后端 emit。
 * 前端 QuestionPanel 基于此在输入框上方浮出问答面板。
 */
export interface QuestionRequestEvent {
  threadId: string;
  /** 对齐 Claude tool_use_id。 */
  toolUseId: string;
  questions: QuestionDto[];
}

/** 单题答案（前端→后端）。 */
export interface AnswerItemDto {
  question: string;
  selected: string[];
  other: string | null;
}

/** `submit_answers` command 入参（前端→后端）。 */
export interface SubmitAnswersInput {
  toolUseId: string;
  cancelled: boolean;
  answers: AnswerItemDto[];
}

export interface StreamErrorEvent {
  threadId: string;
  messageId: string | null;
  error: string;
  retryable: boolean;
}

export interface StreamAbortedEvent {
  threadId: string;
  messageId: string;
}

export interface BudgetWarningEvent {
  threadId: string;
  spentUsd: number;
  limitUsd: number;
  pct: number;
}

export interface ModelEscalatedEvent {
  threadId: string;
  fromModel: string;
  toModel: string;
  reason: string;
}

/**
 * 任务列表更新事件 — 模型每次调 todo_write 后 emit.
 * todos 是整列表全量替换(不是增量).per-thread 内存缓存,不落库.
 */
export interface TodosUpdatedEvent {
  threadId: string;
  todos: TodoItem[];
}

/**
 * 上下文用量事件 — 每次 turn_complete 后 + compaction fold 后 emit.
 * 驱动 ComposeBar 圆环进度指示.
 */
export interface ContextUsageEvent {
  threadId: string;
  usedTokens: number;
  maxTokens: number;
  /** 0~1,usedTokens / maxTokens. */
  ratio: number;
  /** api=精确(回包) / local=本地估算(折叠后). */
  source: "api" | "local";
}

export interface BrainstormRunStartedEvent {
  threadId: string;
  runId: string;
  topic: string;
  participants: BrainstormParticipant[];
}

export interface BrainstormAgentStatusEvent {
  threadId: string;
  runId: string;
  participantId: string;
  status: "idle" | "running" | "done" | "error" | string;
}

export interface BrainstormMessageStartEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participant: BrainstormParticipant;
}

export interface BrainstormMessageDeltaEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participantId: string;
  delta: string;
}

export interface BrainstormReasoningDeltaEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participantId: string;
  delta: string;
}

export interface BrainstormToolCallStartEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participantId: string;
  toolCall: ToolCall;
}

export interface BrainstormToolCallUpdateEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participantId: string;
  toolUseId: string;
  status: ToolStatus;
  input: Record<string, unknown> | null;
  result: string | null;
  durationMs: number | null;
  errorMessage: string | null;
}

export interface BrainstormMessageDoneEvent {
  threadId: string;
  runId: string;
  messageId: string;
  participantId: string;
  content: string;
}

export interface BrainstormRunDoneEvent {
  threadId: string;
  runId: string;
  artifact: string;
}

export interface BrainstormErrorEvent {
  threadId: string;
  runId: string;
  error: string;
}

// ┌─────────────────────────────────────────────────────────────────────────┐
// │ MCP 事件                                                                  │
// └─────────────────────────────────────────────────────────────────────────┘

/**
 * 单个 MCP server 状态变更 — 后端 `mcp:server_status_changed`.
 * 连接/断开/失败时推送,前端据此更新 McpPanel 的状态徽章,无需轮询.
 */
export interface McpServerStatusChangedEvent {
  name: string;
  status: McpServerStatus;
  error: string | null;
}

/**
 * MCP 工具集变更 — 后端 `mcp:tools_changed` (无 payload).
 * 任意 server 的工具上线/下线时推送,前端据此 re-fetch server 列表
 * (刷新 toolCount).
 */
export type McpToolsChangedEvent = Record<string, never>;
