/**
 * E2E 共享工具：连 CDP 真实 Tauri 窗口 + 常用 UI 操作 + 断言。
 * 纯 playwright 库（不依赖 @playwright/test），便于连外部 CDP 窗口。
 */
import { chromium } from "playwright";

export const CDP_PORT = process.env.E2E_CDP_PORT || "9333";
export const VITE_HOST = process.env.E2E_VITE_HOST || "localhost:5180";

export class AssertError extends Error {}
export function assert(cond, msg) {
  if (!cond) throw new AssertError(msg);
}

export async function connectApp(port = CDP_PORT) {
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  for (const ctx of browser.contexts()) {
    for (const p of ctx.pages()) {
      if (p.url().includes(VITE_HOST)) {
        // Auto-accept native confirm() dialogs (rewind uses window.confirm).
        p.on("dialog", (d) => d.accept().catch(() => {}));
        return { browser, page: p };
      }
    }
  }
  await browser.close();
  throw new Error("app page not found over CDP — is the Tauri window running?");
}

/** 回到欢迎页（新对话）：点侧栏「新对话」或直接 reload 到根路由。 */
export async function gotoNewChat(page) {
  // 侧栏「新对话」按钮文案可能变；最稳妥是 reload 到根（routerStore 默认 welcome）。
  await page.goto(`http://${VITE_HOST}/`).catch(() => {});
  await page.locator('[data-testid="compose-input"]').first().waitFor({
    state: "visible",
    timeout: 30_000,
  });
}

/** 在 ComposeBar 输入并回车发送。 */
export async function sendMessage(page, text) {
  const ta = page.locator('[data-testid="compose-input"]').first();
  await ta.waitFor({ state: "visible", timeout: 30_000 });
  await ta.click();
  await ta.fill(text);
  await page.keyboard.press("Enter");
}

/** 轮询直到 predicate(bodyText) 为真，或超时。返回最终 bodyText。 */
export async function waitForBody(page, predicate, { timeoutMs = 120_000, intervalMs = 1500, label = "body condition" } = {}) {
  const start = Date.now();
  let body = "";
  while (Date.now() - start < timeoutMs) {
    body = await page.evaluate(() => document.body.innerText).catch(() => "");
    if (predicate(body)) return body;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new AssertError(`timed out waiting for ${label} after ${timeoutMs}ms`);
}

/** 等待出现「停止」按钮（streaming 中）。 */
export async function waitForStreaming(page, timeoutMs = 30_000) {
  await page.locator('[data-testid="compose-stop"]').waitFor({ state: "visible", timeout: timeoutMs });
}

/** 等待 turn 结束（停止按钮消失 → 发送按钮回来）。 */
export async function waitForTurnEnd(page, timeoutMs = 150_000) {
  await page.locator('[data-testid="compose-send"]').waitFor({ state: "visible", timeout: timeoutMs });
}

/**
 * 后台自动审批：在一段时间内持续监视审批 bar，出现就点「允许」。
 * 模拟用户点允许（default 权限模式下每个工具都要批准）。
 * 返回一个 stop() 函数。
 */
export function startAutoApprove(page) {
  let stopped = false;
  let approvals = 0;
  (async () => {
    while (!stopped) {
      try {
        const allow = page.locator("button", { hasText: /^允许$/ });
        if ((await allow.count()) > 0) {
          await allow.first().click({ timeout: 5000 }).catch(() => {});
          approvals++;
        }
      } catch {
        /* ignore */
      }
      await new Promise((r) => setTimeout(r, 800));
    }
  })();
  return {
    stop: () => {
      stopped = true;
    },
    count: () => approvals,
  };
}

/** 当前 DOM 里的用户消息条数。 */
export async function userMessageCount(page) {
  return page.locator('[data-testid="user-message"]').count();
}

/**
 * 发送一条消息并跑完整个 turn（含自动审批 + 等待开始 + 等待结束）。
 * 返回审批次数。
 *
 * 健壮性要点（避免"假结束"竞态）:
 *  1. 记录发送前的用户消息数,发送后**等用户消息真的入列**才继续 —— 这能
 *     覆盖 welcome 页首条消息的 createThread+导航+reload 延迟（否则 send 还
 *     没落到真 thread 就往下走）。
 *  2. 等流式**真正开始**（stop 按钮出现）再等结束;给足 30s 容纳建 thread +
 *     首 token 延迟。极快的纯文本回复可能整轮 <30s 内开始并结束,所以这里
 *     用"stop 出现 或 已有助手输出增长"双信号,任一命中即认为已进入回合。
 *  3. 最后 waitForTurnEnd 等 send 按钮回来。
 */
export async function sendAndRunTurn(page, text, { timeoutMs = 150_000 } = {}) {
  const auto = startAutoApprove(page);
  const before = await userMessageCount(page);
  await sendMessage(page, text);

  // (1) 等用户消息入列（send 被真正接受 + 可能的 welcome→chat 导航完成）。
  const accStart = Date.now();
  while (Date.now() - accStart < 40_000) {
    if ((await userMessageCount(page)) > before) break;
    await new Promise((r) => setTimeout(r, 400));
  }
  if ((await userMessageCount(page)) <= before) {
    auto.stop();
    throw new AssertError(
      `发送后用户消息未入列（send 被吞?）: "${text.slice(0, 40)}"`,
    );
  }

  // (2) 等流式真正开始：stop 按钮出现。容纳建 thread + 首 token 延迟。
  //     若一直没出现（极快纯文本回复在此窗口内已结束）则继续。
  await page
    .locator('[data-testid="compose-stop"]')
    .waitFor({ state: "visible", timeout: 30_000 })
    .catch(() => {});

  // (3) 等 turn 结束（send 按钮回来）。
  await waitForTurnEnd(page, timeoutMs);
  // 给最后一次 tool_call_update / 持久化一点落地时间。
  await page.waitForTimeout(1500);
  auto.stop();
  return auto.count();
}

/** 当前页面所有工具卡片的 {name,status}. */
export async function toolCards(page) {
  return page.$$eval('[data-testid="tool-card"]', (els) =>
    els.map((e) => ({
      name: e.getAttribute("data-tool-name"),
      status: e.getAttribute("data-tool-status"),
      text: e.innerText.slice(0, 300),
    })),
  );
}

export async function bodyText(page) {
  return page.evaluate(() => document.body.innerText).catch(() => "");
}
