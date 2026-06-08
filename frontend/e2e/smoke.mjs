/**
 * 最小冒烟：连已运行的 CDP 端口（真实 Tauri 窗口），在 ComposeBar 真实输入
 * 一条消息，等真模型回复，断言出现 assistant 文本。
 *
 * 前置：tauri dev 窗口已带 --remote-debugging-port=<port> 启动。
 * 用法：node e2e/smoke.mjs [port]
 */
import { chromium } from "playwright";

const port = process.argv[2] || "9333";
const endpoint = `http://127.0.0.1:${port}`;

function log(...a) {
  console.log("[smoke]", ...a);
}

async function findAppPage(browser, viteHost = "localhost:5180") {
  for (const ctx of browser.contexts()) {
    for (const p of ctx.pages()) {
      if (p.url().includes(viteHost)) return p;
    }
  }
  return null;
}

(async () => {
  const browser = await chromium.connectOverCDP(endpoint);
  const page = await findAppPage(browser);
  if (!page) {
    log("FAIL: app page not found");
    process.exit(2);
  }
  log("connected app page:", await page.title());

  // 1. 找 ComposeBar 的 textarea（placeholder 含"发条消息"或"开始"）。
  const ta = page.locator("textarea").first();
  await ta.waitFor({ state: "visible", timeout: 30_000 });
  log("textarea visible");

  // 2. 输入并发送（Enter）。用一个确定性问题，便于断言。
  const marker = "E2E-PING-" + Date.now().toString().slice(-5);
  await ta.click();
  await ta.fill(`Reply with exactly this token and nothing else: ${marker}`);
  await page.keyboard.press("Enter");
  log("message sent, waiting for assistant reply...");

  // 3. 等待 assistant 回复出现（轮询页面文本含 marker 或出现 assistant 气泡）。
  const start = Date.now();
  let answered = false;
  let lastLen = 0;
  while (Date.now() - start < 120_000) {
    const text = await page.evaluate(() => document.body.innerText).catch(() => "");
    if (text.length !== lastLen) {
      lastLen = text.length;
    }
    if (text.includes(marker) && text.lastIndexOf(marker) !== text.indexOf(marker)) {
      // marker 出现至少两次：一次是用户气泡，一次是 assistant 回复
      answered = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 1500));
  }

  const finalText = await page.evaluate(() => document.body.innerText).catch(() => "");
  log("final body text length:", finalText.length);
  log("marker occurrences:", finalText.split(marker).length - 1);
  log("tail:", finalText.slice(-400).replace(/\s+/g, " "));

  await browser.close();
  if (answered) {
    log("SMOKE_OK: real model replied through the real UI + IPC");
    process.exit(0);
  } else {
    log("SMOKE_FAIL: no assistant reply containing marker within timeout");
    process.exit(3);
  }
})().catch((e) => {
  console.log("[smoke] ERROR:", e.message);
  process.exit(1);
});
