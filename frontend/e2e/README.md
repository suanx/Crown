# E2E — 真实点击 · 真窗口 · 真后端 · 真模型

用 Playwright 通过 **CDP** 连上**真实的 Tauri 窗口**自动点击，走真实
前端 → 真实 Tauri IPC → 真实 Rust 引擎 → 真实大模型。**后端零改动。**

单元测试发现不了时序竞态；这套 e2e 用来覆盖工具卡片恢复、中止、回溯、斜杠命令和子代理面板等真实交互。

## 机制

设环境变量 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`
后启动 app，WebView2 子进程继承该参数 → 开 CDP 端口 → Playwright
`connectOverCDP("http://127.0.0.1:9333")` 连上真窗口的 page 自动操作。

用纯 `playwright` 库（不是 `@playwright/test`），因为我们连的是外部已运行的
窗口，不需要 test runner 托管浏览器。

## 前置：两个长驻进程

```powershell
# 终端 A — Vite dev（hybrid：Tauri 内走真后端，浏览器里 fallback mock）
cd deepseek-agent\frontend
$env:VITE_API_MODE='hybrid'; npm run dev          # → http://localhost:5180

# 终端 B — 真实 Tauri 窗口（CDP 端口 + 真实凭据）
cd deepseek-agent\crates\app
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9333'
$env:WEBVIEW2_USER_DATA_FOLDER="$env:TEMP\dsagent-e2e"
$env:DEEPSEEK_API_KEY=[Environment]::GetEnvironmentVariable('DEEPSEEK_API_KEY','User')
$env:BOCHA_API_KEY='<web search key>'
cargo tauri dev
```

- 改 **Rust** 文件：`cargo tauri dev` 自动重编译并重启窗口（约 10s，CDP 9333 仍可用，
  但 thread 状态清空）。
- 改 **前端** 文件：Vite 自动热重载，无需重启。

## 跑

```powershell
cd deepseek-agent\frontend
node e2e/run-e2e.mjs            # 跑全部 specs/*.mjs
node e2e/run-e2e.mjs p2 p4     # 只跑名字含 p2 / p4 的 spec
```

runner 连一次 CDP，依次执行每个 spec 的 `run(page)`，最后汇总 `N/M passed`。
退出码非 0 表示有失败。

## 目录

```
e2e/
  lib.mjs            共享工具（连接/导航/发送/自动审批/断言）
  run-e2e.mjs        runner（连 CDP + 跑 specs + 汇总）
  specs/
    p0-tool-card-restore.mjs   工具卡片重载恢复
    p1-abort.mjs               长命令中止 + 中止后历史合法
    p2-rewind.mjs              回溯（对话截断）
    p3-slash-plan.mjs          /plan 斜杠命令
    p4-subagent.mjs            子代理 task + 嵌套面板
  probe-*.mjs        调试探针（一次性观测，不进汇总）
    probe-store.mjs            读 window.__stores 看 chatStore 真实状态
    probe-buttons.mjs / probe-console.mjs / ...
```

## 写一个新 spec

每个 spec 是一个 `.mjs`，导出 `name` 字符串 + `async run(page)`：

```js
import { assert, gotoNewChat, sendAndRunTurn, toolCards } from "../lib.mjs";

export const name = "我的场景";

export async function run(page) {
  await gotoNewChat(page);
  await sendAndRunTurn(page, "请用 list_directory 列出当前目录");
  const cards = await toolCards(page);
  assert(cards.some((c) => c.name === "list_directory"), "应出现 list_directory 卡片");
  return { detail: "通过 ✓" };
}
```

`run` 抛错（含断言失败）即判 FAIL；正常返回即 PASS，`detail` 会打印在结果行。

## 关键 helper（lib.mjs）

- `connectApp()` — 连 CDP，找到 5180 的 page，自动 accept `window.confirm`（回溯用）。
- `gotoNewChat(page)` — `page.goto` 根路由回到欢迎页。
- `sendMessage(page, text)` — 填输入框 + 回车。
- `sendAndRunTurn(page, text, {timeoutMs})` — 发消息并跑完整轮：自动点「允许」
  审批 + **等用户消息真入列**（覆盖建 thread / 导航延迟）+ 等流式起止。返回审批次数。
- `toolCards(page)` — 当前所有工具卡 `{name, status, text}`。
- `waitForTurnEnd(page)` — 等发送按钮回来（turn 结束）。

## data-testid 约定（非侵入式加在产品组件上）

- ComposeBar：`compose-input` / `compose-send` / `compose-stop`
- ToolCallCard：`tool-card`（+ `data-tool-name` / `data-tool-status`）、`subagent-panel`
- UserMessage：`user-message`，回溯按钮 `aria-label="回到这里"`
- SessionItem：`session-item` / `session-item-open`

## 调试

dev 模式下 `window.__stores = { chat, router }`（见 `src/main.tsx`，仅 DEV），
`probe-store.mjs` 用它直接读 store 真实状态，比靠 DOM 反推快得多。
prod build 不暴露，无副作用。
