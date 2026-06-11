/**
 * Mock Fixtures — 集中所有假数据.
 *
 * 字段全部按 contracts.ts 形状填充. AI 接 Rust 时,这里就是 Rust serde 必须输出的形状.
 */

import type {
  AppConfig,
  McpServer,
  Skill,
  Message,
  ModelInfo,
  ProjectSummary,
  Thread,
  ThreadSummary,
  ToolCall,
  UsageStats,
  UserBalance,
} from "../contracts";

// ── 时间锚点 ──────────────────────────────────────────────────────────────

const NOW = Date.parse("2026-05-28T16:30:00+08:00");
const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

const at = (offsetMs: number) => new Date(NOW - offsetMs).toISOString();

// ── Models ────────────────────────────────────────────────────────────────

export const MOCK_MODELS: ModelInfo[] = [
  {
    id: "deepseek-v4-flash",
    label: "DeepSeek V4 Flash",
    description: "便宜、快、日常默认,128K 上下文",
    pricePerMillionInputUsd: 0.14,
    pricePerMillionOutputUsd: 0.28,
    pricePerMillionCacheHitUsd: 0.014,
    contextWindow: 128_000,
    providerId: "deepseek",
  },
  {
    id: "deepseek-v4-pro",
    label: "DeepSeek V4 Pro",
    description: "更强推理,适合复杂任务",
    pricePerMillionInputUsd: 0.55,
    pricePerMillionOutputUsd: 2.19,
    pricePerMillionCacheHitUsd: 0.055,
    contextWindow: 128_000,
    providerId: "deepseek",
  },
];

// ── Projects ──────────────────────────────────────────────────────────────

export const MOCK_PROJECTS: ProjectSummary[] = [
  {
    id: "proj-crown",
    name: "crown",
    path: "~/code/crown",
    threadCount: 4,
    lastUsedAt: at(0),
  },
  {
    id: "proj-frontend",
    name: "crown-frontend",
    path: "~/code/crown/frontend",
    threadCount: 3,
    lastUsedAt: at(2 * HOUR),
  },
  {
    id: "proj-blog",
    name: "blog-2026",
    path: "~/Documents/blog-2026",
    threadCount: 1,
    lastUsedAt: at(3 * DAY),
  },
];

// ── Thread Summaries ─────────────────────────────────────────────────────

export const MOCK_THREAD_SUMMARIES: ThreadSummary[] = [
  {
    id: "thread-1",
    title: "Rust HTTP 服务器搭建",
    updatedAt: at(0),
    messageCount: 8,
    isStreaming: true,
    isPinned: true,
    preview: "好的,我来帮你创建一个基础的 axum HTTP 服务器...",
    projectId: "proj-crown",
    providerId: "deepseek",
  },
  {
    id: "thread-2",
    title: "前端 hook 闭包陷阱",
    updatedAt: at(2 * HOUR),
    messageCount: 6,
    isStreaming: false,
    isPinned: false,
    preview: "useCallback 的依赖数组应当包含...",
    projectId: "proj-prototype",
    providerId: "deepseek",
  },
  {
    id: "thread-3",
    title: "Tailwind 主题切换最佳实践",
    updatedAt: at(5 * HOUR),
    messageCount: 12,
    isStreaming: false,
    isPinned: false,
    preview: "用 CSS 变量 + class 切换,不要写两套...",
    projectId: "proj-prototype",
    providerId: "deepseek",
  },
  {
    id: "thread-4",
    title: "整理 GitHub 项目到简历",
    updatedAt: at(1 * DAY + 3 * HOUR),
    messageCount: 4,
    isStreaming: false,
    isPinned: false,
    preview: "我读了你的 repo,这三个项目最值得放...",
    projectId: "proj-crown",
    providerId: "deepseek",
  },
  {
    id: "thread-5",
    title: "Python 数据清洗脚本",
    updatedAt: at(2 * DAY + 4 * HOUR),
    messageCount: 22,
    isStreaming: false,
    isPinned: false,
    preview: "pandas 的 dropna 默认 axis=0...",
    projectId: null,
    providerId: "deepseek",
  },
  {
    id: "thread-6",
    title: "accessToken 验证测试",
    updatedAt: at(4 * DAY + 6 * HOUR),
    messageCount: 9,
    isStreaming: false,
    isPinned: false,
    preview: null,
    projectId: "proj-crown",
    providerId: "deepseek",
  },
  {
    id: "thread-7",
    title: "React 组件重构方案",
    updatedAt: at(6 * DAY + 2 * HOUR),
    messageCount: 14,
    isStreaming: false,
    isPinned: false,
    preview: "拆分原则: 单一职责 + props 不超 5 个",
    projectId: "proj-prototype",
    providerId: "deepseek",
  },
  {
    id: "thread-8",
    title: "claudecli 命令无法识别",
    updatedAt: at(12 * DAY),
    messageCount: 5,
    isStreaming: false,
    isPinned: false,
    preview: "PATH 没加,执行 npm bin -g 看下...",
    projectId: null,
    providerId: "deepseek",
  },
  {
    id: "thread-9",
    title: "博客 RSS 订阅源同步",
    updatedAt: at(20 * DAY),
    messageCount: 7,
    isStreaming: false,
    isPinned: false,
    preview: null,
    projectId: "proj-blog",
    providerId: "deepseek",
  },
  {
    id: "thread-10",
    title: "Unity 像素美术规格指南",
    updatedAt: at(45 * DAY),
    messageCount: 18,
    isStreaming: false,
    isPinned: false,
    preview: null,
    projectId: null,
    providerId: "deepseek",
  },
  {
    id: "thread-11",
    title: "crown 用量统计 SQL",
    updatedAt: at(60 * DAY),
    messageCount: 6,
    isStreaming: false,
    isPinned: false,
    preview: null,
    projectId: "proj-crown",
    providerId: "deepseek",
  },
];

// ── Tool Calls ────────────────────────────────────────────────────────────

const TC_READ_CARGO: ToolCall = {
  id: "tc-1",
  name: "read_file",
  input: { path: "Cargo.toml" },
  status: "success",
  result: `[package]
name = "my-server"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }`,
  durationMs: 45,
  diff: null,
  errorMessage: null,
};

const TC_WRITE_MAIN: ToolCall = {
  id: "tc-2",
  name: "write_file",
  input: { path: "src/main.rs" },
  status: "success",
  result: "File written successfully (612 bytes)",
  durationMs: 12,
  diff: {
    path: "src/main.rs",
    before: `fn main() {
    println!("Hello, world!");
}`,
    after: `use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, axum!" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}`,
  },
  errorMessage: null,
};

const TC_RUN_PENDING: ToolCall = {
  id: "tc-3",
  name: "run_command",
  input: { command: "cargo run", cwd: "/Users/dev/my-server" },
  status: "pending_approval",
  result: null,
  durationMs: null,
  diff: null,
  errorMessage: null,
};

const TC_GREP: ToolCall = {
  id: "tc-4",
  name: "grep",
  input: { pattern: "TODO|FIXME", path: "src/" },
  status: "success",
  result: `src/main.rs:23:    // TODO: 添加优雅关闭
src/handlers/user.rs:48:    // FIXME: N+1 查询
src/handlers/user.rs:67:    // TODO: 加分页
src/middleware/auth.rs:12:    // FIXME: 应当从 KMS 拉密钥`,
  durationMs: 890,
  diff: null,
  errorMessage: null,
};

const TC_READ_RUNNING: ToolCall = {
  id: "tc-5",
  name: "read_file",
  input: { path: "tests/integration_test.rs" },
  status: "running",
  result: null,
  durationMs: null,
  diff: null,
  errorMessage: null,
};

// ── Messages (thread-1) ───────────────────────────────────────────────────

export const MOCK_MESSAGES_T1: Message[] = [
  {
    id: "m-1",
    threadId: "thread-1",
    role: "user",
    content: "帮我写一个 Rust 的 HTTP 服务器,用 axum",
    timestamp: at(15 * 60 * 1000),
    reasoning: null,
    toolCalls: null,
    segments: [],
    usage: null,
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-2",
    threadId: "thread-1",
    role: "assistant",
    content: `好的,我来帮你创建一个基础的 axum HTTP 服务器。

首先看下现有项目结构,然后我会:

1. **更新 Cargo.toml** — 加入 axum 和 tokio 依赖
2. **改写 main.rs** — 写一个最小可运行版本
3. **加个 /health 健康检查路由** — 方便后面部署

让我先读一下你的 \`Cargo.toml\`。`,
    timestamp: at(14 * 60 * 1000),
    reasoning: `用户要 Rust HTTP 服务器,首选 axum (tokio 团队主推,生态最全)。
最小可运行版本应该:
- 用 #[tokio::main] 而不是手动 runtime
- 一个 GET / 路由,返回字符串
- 监听 0.0.0.0:3000 (而不是 127.0.0.1,方便容器部署)
先读 Cargo.toml,确认 edition 和已有依赖,避免重复添加。`,
    toolCalls: [TC_READ_CARGO, TC_WRITE_MAIN],
    segments: [],
    usage: {
      cacheReadTokens: 1100,
      cacheMissTokens: 150,
      cacheCreationTokens: 0,
      outputTokens: 340,
      costUsd: 0.0012,
    },
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-3",
    threadId: "thread-1",
    role: "user",
    content: "运行一下试试",
    timestamp: at(10 * 60 * 1000),
    reasoning: null,
    toolCalls: null,
    segments: [],
    usage: null,
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-4",
    threadId: "thread-1",
    role: "assistant",
    content: "好的,我来运行它。这会启动 cargo,需要你批准命令执行:",
    timestamp: at(9 * 60 * 1000),
    reasoning: null,
    toolCalls: [TC_RUN_PENDING],
    segments: [],
    usage: {
      cacheReadTokens: 1700,
      cacheMissTokens: 120,
      cacheCreationTokens: 0,
      outputTokens: 24,
      costUsd: 0.00018,
    },
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-5",
    threadId: "thread-1",
    role: "user",
    content: "先别跑,搜下代码里的 TODO",
    timestamp: at(7 * 60 * 1000),
    reasoning: null,
    toolCalls: null,
    segments: [],
    usage: null,
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-6",
    threadId: "thread-1",
    role: "assistant",
    content: `找到 4 个待办项,主要分两类:

**功能性 TODO:**
- \`src/main.rs:23\` — 优雅关闭 (推荐用 \`tokio::signal::ctrl_c\`)
- \`src/handlers/user.rs:67\` — 分页

**待修 FIXME:**
- \`src/handlers/user.rs:48\` — N+1 查询,应改 join
- \`src/middleware/auth.rs:12\` — 密钥不应硬编码

按优先级建议先解决两个 FIXME,优雅关闭可以放一起加。要我开始改哪个?`,
    timestamp: at(6 * 60 * 1000),
    reasoning: null,
    toolCalls: [TC_GREP],
    segments: [],
    usage: {
      cacheReadTokens: 1820,
      cacheMissTokens: 160,
      cacheCreationTokens: 0,
      outputTokens: 178,
      costUsd: 0.00029,
    },
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-7",
    threadId: "thread-1",
    role: "user",
    content: "先看看测试覆盖到了哪些路径",
    timestamp: at(2 * 60 * 1000),
    reasoning: null,
    toolCalls: null,
    segments: [],
    usage: null,
    isStreaming: false,
    interrupted: false,
  },
  {
    id: "m-8",
    threadId: "thread-1",
    role: "assistant",
    content: `让我先看下集成测试文件...

`,
    timestamp: at(30 * 1000),
    reasoning: `用户问测试覆盖,先读 integration_test.rs 看测了哪些 endpoint,
再决定要不要跑 cargo tarpaulin 看覆盖率`,
    toolCalls: [TC_READ_RUNNING],
    segments: [],
    usage: null,
    isStreaming: true,
    interrupted: false,
  },
];

// ── Thread (完整) ─────────────────────────────────────────────────────────

export const MOCK_THREAD_T1: Thread = {
  id: "thread-1",
  title: "Rust HTTP 服务器搭建",
  createdAt: at(20 * 60 * 1000),
  updatedAt: at(0),
  model: "deepseek-v4-flash",
  thinkingEffort: "medium",
  providerId: "deepseek",
  permissionMode: "default",
  costUsd: 0.0034,
  messages: MOCK_MESSAGES_T1,
};

// ── MCP / Config / Stats ──────────────────────────────────────────────────

export const MOCK_MCP_SERVERS: McpServer[] = [
  {
    name: "filesystem",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/dev"],
    status: "connected",
    enabled: true,
    toolCount: 11,
    errorMessage: null,
  },
  {
    name: "github",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-github"],
    status: "connected",
    enabled: true,
    toolCount: 26,
    errorMessage: null,
  },
  {
    name: "postgres",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-postgres"],
    status: "failed",
    enabled: true,
    toolCount: 0,
    errorMessage: "connection refused: localhost:5432",
  },
  {
    name: "playwright",
    command: "npx",
    args: ["-y", "@playwright/mcp"],
    status: "disabled",
    enabled: false,
    toolCount: 0,
    errorMessage: null,
  },
];

export const MOCK_SKILLS: Skill[] = [
  {
    name: "commit",
    description:
      "Create a well-formed git commit with a conventional-commits message. Use when the user asks to commit changes.",
    scope: "global",
    source: "native",
    path: "~/.local/share/crown/skills/commit/SKILL.md",
    allowedTools: ["run_command"],
  },
  {
    name: "review-pr",
    description:
      "Review a pull request against Clean Code conventions and emit a blocking/warning/suggestion report.",
    scope: "global",
    source: "native",
    path: "~/.local/share/crown/skills/review-pr/SKILL.md",
    allowedTools: [],
  },
  {
    name: "brainstorming",
    description:
      "Socratic brainstorming before any creative work. Compatible with the superpowers methodology.",
    scope: "project",
    source: "claude",
    path: ".claude/skills/brainstorming/SKILL.md",
    allowedTools: [],
  },
];

export const MOCK_CONFIG: AppConfig = {
  apiKeyPresent: true,
  baseUrl: "https://api.deepseek.com",
  defaultProviderId: "opencode",
  defaultModel: "deepseek-v4-flash",
  providers: [
    {
      id: "opencode",
      name: "OpenCode CLI",
      providerType: "openai-compatible",
      baseUrl: "https://opencode.ai/zen/v1",
      apiKey: null,
      apiKeyPresent: false,
      enabled: true,
      models: [
        { id: "deepseek-v4-flash", label: "DeepSeek V4 Flash", enabled: true },
        { id: "gpt-5.4-nano", label: "GPT 5.4 Nano", enabled: true },
      ],
    },
    {
      id: "xfyun",
      name: "讯飞星辰 MaaS",
      providerType: "openai-compatible",
      baseUrl: "https://maas-api.cn-huabei-1.xf-yun.com/v2",
      apiKey: null,
      apiKeyPresent: false,
      enabled: false,
      models: [],
    },
    {
      id: "openai-compatible",
      name: "OpenAI 通用接口",
      providerType: "openai-compatible",
      baseUrl: "",
      apiKey: null,
      apiKeyPresent: false,
      enabled: false,
      models: [],
    },



    {
      id: "deepseek",
      name: "DeepSeek",
      providerType: "deepseek",
      baseUrl: "https://api.deepseek.com",
      apiKey: null,
      apiKeyPresent: false,
      enabled: true,
      models: [
        { id: "deepseek-v4-flash", label: "v4-flash", enabled: true },
        { id: "deepseek-v4-pro", label: "v4-pro", enabled: true },
      ],
    },
    {
      id: "openai",
      name: "OpenAI",
      providerType: "openai",
      baseUrl: "https://api.openai.com/v1",
      apiKey: null,
      apiKeyPresent: false,
      enabled: false,
      models: [
        { id: "gpt-4.1", label: "GPT-4.1", enabled: true },
        { id: "gpt-4o", label: "GPT-4o", enabled: true },
      ],
    },

    {
      id: "anthropic",
      name: "Anthropic",
      providerType: "anthropic",
      baseUrl: "https://api.anthropic.com/v1",
      apiKey: null,
      apiKeyPresent: false,
      enabled: false,
      models: [
        { id: "claude-3-5-sonnet-latest", label: "Claude Sonnet", enabled: true },
        { id: "claude-3-5-haiku-latest", label: "Claude Haiku", enabled: true },
      ],
    },

    {
      id: "siliconflow",
      name: "硅基流动",
      providerType: "openai-compatible",
      baseUrl: "https://api.siliconflow.cn/v1",
      apiKey: null,
      apiKeyPresent: false,
      enabled: false,
      models: [
        { id: "Qwen/Qwen3-32B", label: "Qwen3 32B", enabled: true },
        { id: "deepseek-ai/DeepSeek-V3", label: "DeepSeek V3", enabled: true },
      ],
    },

    {
      id: "ollama",
      name: "Ollama",
      providerType: "ollama",
      baseUrl: "http://localhost:11434/v1",
      apiKey: "ollama",
      apiKeyPresent: true,
      enabled: false,
      models: [
        { id: "llama3.1", label: "Llama 3.1", enabled: true },
        { id: "qwen2.5-coder", label: "Qwen2.5 Coder", enabled: true },
      ],
    },
  ],
  webSearch: {
    defaultProviderId: "jina",
    providers: [
      {
        id: "jina",
        name: "Jina Search",
        apiKey: null,
        apiKeyPresent: false,
        enabled: true,
        implemented: true,
        keyRequired: false,
        note: "无 key 可用；填入 Jina API key 后使用结构化搜索。",
      },
      {
        id: "duckduckgo",
        name: "DuckDuckGo HTML",
        apiKey: null,
        apiKeyPresent: false,
        enabled: true,
        implemented: true,
        keyRequired: false,
        note: "零配置 fallback，适合 demo 直接可用。",
      },
      {
        id: "bocha",
        name: "Bocha AI Search",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
      {
        id: "brave",
        name: "Brave Search API",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
      {
        id: "tavily",
        name: "Tavily",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
      {
        id: "exa",
        name: "Exa",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
      {
        id: "serper",
        name: "Serper",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
      {
        id: "serpapi",
        name: "SerpAPI",
        apiKey: null,
        apiKeyPresent: false,
        enabled: false,
        implemented: true,
        keyRequired: true,
        note: "后端已接入，需要 API key。",
      },
    ],
  },
  permissionMode: "default",
  theme: "dark",
  language: "zh",
  budget: {
    mode: "per_session",
    limitUsd: 5.0,
  },
  compaction: {
    triggerRatio: 0.7,
    keepRecentTurns: 3,
  },
  shell: {
    timeoutSecs: 120,
    maxOutputBytes: 1_048_576,
  },
  subagent: {
    maxSubtasks: 5,
    model: "",
  },
  workspaceDir: "C:/Projects",
};

export const MOCK_USAGE_STATS: UsageStats = {
  totalCostUsd: 0.0034,
  cumulativeCacheSavedUsd: 0.0042,
  cacheReadTokens: 4620,
  cacheMissTokens: 430,
  cacheCreationTokens: 0,
  outputTokens: 542,
  cacheHitRatio: 0.915,
  windowLabel: "session",
  budgetLimitUsd: null,
  budgetUsedPct: null,
};

/**
 * P3a task 7 mock — DeepSeek 风格 CNY 余额.
 * UI Balance cell 应该看到 ¥45.32,其中 10 元 granted (赠送) + 35.32 元 toppedUp.
 */
export const MOCK_USER_BALANCE: UserBalance = {
  isAvailable: true,
  primaryCurrency: "CNY",
  balanceInfos: [
    { currency: "CNY", total: 45.32, granted: 10.0, toppedUp: 35.32 },
  ],
};
