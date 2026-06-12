
<h1 align="center">Crown</h1>

<p align="center">
  <strong>面向桌面的 AI Agent 客户端</strong>
  <br />
  基于 Tauri 2 + Rust + React + TypeScript 构建
</p>

<p align="center">
  <img alt="Version" src="https://img.shields.io/badge/version-1.3.4-111111?style=for-the-badge" />
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows-2B2D2B?style=for-the-badge" />
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2.x-444444?style=for-the-badge" />
  <img alt="License" src="https://img.shields.io/badge/license-MIT-EDEBE3?style=for-the-badge&labelColor=111111&color=EDEBE3" />
</p>

---

## 什么是 Crown？

Crown 是一个桌面 AI Agent 客户端，通过自然语言驱动大模型执行**文件操作、代码编辑、Shell 命令、网页搜索**等任务。它结合了 Tauri 的轻量桌面壳与 Rust 的高性能后端，提供类似 Claude Code 的 Agent 体验，但运行在本地 GUI 环境中。

### 核心能力

- 🧠 **多轮 Agent 对话** — 模型自主规划、调用工具、循环推理直至完成任务
- 🔧 **全栈工具系统** — 文件读写、代码编辑、Shell 执行、正则搜索、网络抓取
- 🌐 **Web Search** — 集成 8 家搜索供应商（Jina、DuckDuckGo、Bocha、Brave、Tavily、Exa、Serper、SerpAPI）
- 🧩 **MCP 协议支持** — 运行时加载 MCP 服务器，工具热插拔
- 📦 **Skills 技能系统** — 遵循 Agent Skills 规范，内置 11 个即用技能
- 🔒 **5 级权限控制** — 从 Default/Plan 到 YOLO/Strict，灵活平衡效率与安全
- 🤖 **子代理委派** — 支持 Explore/Plan/General-purpose 三种子代理并行执行
- 💾 **本地持久化** — SQLite 存储对话、用量、权限规则，数据完全本地可控
- 📐 **自动上下文压缩** — 多阈值折叠策略，控制 Token 消耗，支持超长对话

---

## 架构总览

```
┌──────────────────────────────────────────────────────────────┐
│                   Frontend (React / TypeScript)               │
│  ┌──────────┐ ┌──────────┐ ┌─────────┐ ┌──────────────────┐  │
│  │ ChatPage │ │Settings  │ │ Skills  │ │ Workspace Panel  │  │
│  │          │ │ 14 Panels │ │ Manager │ │ Files/Term/Tasks │  │
│  └────┬─────┘ └────┬─────┘ └────┬────┘ └────────┬─────────┘  │
│       └────────────┴────────────┴───────────────┘            │
│                        │ IPC (Tauri commands)                 │
├────────────────────────┼──────────────────────────────────────┤
│                   Rust Backend (8 crates)                      │
│                                                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐ │
│  │  Core    │  │  Tools   │  │  Client  │  │  State       │ │
│  │  Engine  │  │ Registry │  │  LLM API │  │  SQLite      │ │
│  │  +Prompt │  │ 12 Tools │  │  DeepSeek│  │  8 Tables    │ │
│  └────┬─────┘  └────┬─────┘  └──────────┘  └──────┬───────┘ │
│       └─────────────┴──────────┬───────────────────┘         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                   │
│  │  MCP     │  │  Skill   │  │ Tokenizer│                   │
│  │  Manager │  │ Discovery│  │ BPE Vocab│                   │
│  └──────────┘  └──────────┘  └──────────┘                   │
└──────────────────────────────────────────────────────────────┘
```

### 数据流

```
用户输入 → Engine.send_message() → 构建 Prompt → LLM API 流式响应
    → 解析 Tool Call → Permission 9 步决策流
    → 执行工具 → 结果回注 → 继续循环 (最多 200 轮迭代)
    → 用户可见的 EngineEvent (内容增量 / 工具状态 / 审批请求)
```

---

## 内置技能

| 技能 | 用途 |
|------|------|
| `deep-research` | 系统性多维度调研，生成结构化长篇报告 |
| `web-research` | 联网搜索与事实核查 |
| `file-reader` | PDF/Word/Excel/PPT 文档读取 |
| `code-review` | 代码审查（逻辑/安全/性能） |
| `git-helper` | Git 工作流与冲突解决 |
| `db-helper` | SQL/数据库设计与优化 |
| `image-analyzer` | OCR/图片处理/截图对比 |
| `api-tester` | API 测试与调试 |
| `refactoring` | 代码重构安全指南 |
| `translator` | 翻译与 i18n 本地化 |
| `terminal-wizard` | Shell 脚本与终端技巧 |

---

## Web Search 支持

| 供应商 | 类型 | 说明 |
|--------|------|------|
| Jina | API | 默认搜索供应商 |
| DuckDuckGo | HTML | 免 API Key |
| Bocha AI | API | 中文搜索优化 |
| Brave | API | 隐私优先搜索引擎 |
| Tavily | API | AI 原生搜索 |
| Exa | API | 语义搜索引擎 |
| Serper | API | Google 搜索代理 |
| SerpAPI | API | Google 搜索结果 API |

---

## 技术栈

| 层级 | 技术 |
|------|------|
| **桌面壳** | Tauri 2.x |
| **后端语言** | Rust (edition 2021) |
| **前端框架** | React 18 + TypeScript 5 |
| **状态管理** | Zustand |
| **样式** | Tailwind CSS 3 |
| **构建工具** | Vite 6 |
| **数据库** | SQLite (rusqlite) |
| **LLM 协议** | OpenAI-compatible SSE streaming |
| **MCP 协议** | rmcp SDK (stdio + Streamable HTTP) |
| **终端** | xterm.js + PTY |

### Rust Crate 依赖关系

```
crates/app (Tauri 入口, IPC 命令, 打包)
    ├── crates/core (Agent 引擎, 权限, 提示词, 上下文折叠)
    │   └── crates/client (LLM API 客户端, 流式解析)
    ├── crates/tools (工具注册中心, 12 个内置工具)
    ├── crates/state (SQLite 持久化层)
    ├── crates/mcp (MCP 服务器管理)
    ├── crates/skill (技能发现与加载)
    └── crates/tokenizer (BPE 分词器)
```

---

## 项目结构

```
Crown/
├── crates/
│   ├── app/                  # Tauri 桌面应用
│   │   ├── src/commands/     #   IPC 命令 (40+)
│   │   ├── src/dto.rs        #   IPC 协议 DTO
│   │   ├── src/events.rs     #   事件桥接
│   │   ├── tauri.conf.json   #   Tauri 配置
│   │   └── bundled-skills/   #   11 个内置技能
│   ├── core/                 # Agent 引擎核心
│   │   ├── src/engine.rs     #   执行引擎 (3874 行)
│   │   ├── src/prompt.rs     #   系统提示词构建
│   │   ├── src/compaction.rs #   上下文自动折叠
│   │   ├── src/permission/   #   权限决策流
│   │   ├── src/pricing/      #   多供应商计价
│   │   ├── src/memory.rs     #   长期记忆
│   │   ├── src/hooks.rs      #   生命周期钩子
│   │   └── src/subagent/     #   子代理系统
│   ├── tools/                # 工具系统
│   │   ├── src/filesystem.rs #   文件工具
│   │   ├── src/shell.rs      #   Shell 执行
│   │   ├── src/web/          #   网页搜索与抓取
│   │   └── src/registry.rs   #   工具注册中心
│   ├── client/               # LLM API 客户端
│   │   └── src/deepseek.rs   #   DeepSeek API
│   ├── state/                # 持久化层
│   │   └── src/schema.sql    #   数据库 schema
│   ├── mcp/                  # MCP 协议支持
│   └── tokenizer/            # BPE 分词器
├── frontend/                 # React 前端
│   ├── src/api/              # IPC 客户端层
│   │   ├── contracts.ts      #   协议定义 (1222 行)
│   │   ├── HybridClient.ts   #   运行时分流
│   │   ├── mock/             #   模拟实现
│   │   └── tauri/            #   Tauri 实现
│   ├── src/features/         # 功能模块
│   │   ├── chat/             #   对话 (28 组件)
│   │   ├── settings/         #   设置 (14 面板)
│   │   ├── sidebar/          #   侧边栏
│   │   ├── skills/           #   技能管理
│   │   └── workspace/        #   工作区面板
│   ├── src/stores/           # Zustand 状态
│   └── src/shared/           # 通用组件
├── docs/                     # 文档
├── cargo.toml                # 工作空间配置
└── changelog.md              # 更新日志
```

---

## 本地开发

### 环境要求

- Rust stable (最新版)
- Node.js 18+
- npm
- Tauri CLI (`cargo install tauri-cli --version "^2"`)

### 启动

```bash
# 1. 安装前端依赖
cd frontend
npm install

# 2. 启动前端开发服务器 (端口 5180)
npm run dev

# 3. 新开终端，启动桌面应用
cd crates/app
cargo tauri dev
```

### 构建

```bash
# 构建前端
cd frontend
npm run build

# 构建 Windows 安装包
cd crates/app
cargo tauri build

# 产物路径
target/release/bundle/nsis/Crown_*.exe
```

---

## 配置

Crown 的配置存储在本地应用数据目录：

- **Windows**: `%APPDATA%/crown/config.json`
- **API Key**: 由 Rust 端加密存储，前端仅展示掩码
- **工作目录**: 设置 → 工作目录

---

## 隐私与安全

- 🔐 API Key 保存在本机应用配置中，前端不接触完整密钥
- 🚫 不提交 `.env`、日志、安装包、临时截图到版本库
- 🛡️ 所有工具调用受 9 步权限决策流保护
- 🌐 Web Fetch 含 SSRF 防护（手工处理重定向 + 同主机检查）
- 📍 数据完全本地化，无需注册或云端账号

---

## 许可

MIT License — 详见 [LICENSE](./license)

---

## 更新日志

参见 [changelog.md](./changelog.md)
