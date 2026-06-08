// CDP 可行性探针：连接已启动的 WebView2 调试端口，列出页面 + 标题 + 读取 DOM。
// 用法：node e2e/probe-cdp.mjs [port]
import { chromium } from "playwright";

const port = process.argv[2] || "9222";
const endpoint = `http://127.0.0.1:${port}`;

async function listTargets() {
  // CDP 的 /json/version 和 /json/list 是发现入口
  const ver = await fetch(`${endpoint}/json/version`).then((r) => r.json());
  console.log("CDP version:", JSON.stringify(ver));
  const list = await fetch(`${endpoint}/json/list`).then((r) => r.json());
  console.log(`targets: ${list.length}`);
  for (const t of list) {
    console.log(`  - [${t.type}] ${t.title} :: ${t.url}`);
  }
  return list;
}

(async () => {
  try {
    await listTargets();
  } catch (e) {
    console.log("HTTP discovery failed:", e.message);
  }

  try {
    const browser = await chromium.connectOverCDP(endpoint);
    const contexts = browser.contexts();
    console.log(`playwright contexts: ${contexts.length}`);
    let total = 0;
    for (const ctx of contexts) {
      for (const page of ctx.pages()) {
        total++;
        const title = await page.title().catch(() => "(no title)");
        const url = page.url();
        const bodyLen = await page
          .evaluate(() => document.body?.innerText?.length ?? 0)
          .catch(() => -1);
        console.log(`  page: title="${title}" url=${url} bodyTextLen=${bodyLen}`);
      }
    }
    console.log(total > 0 ? "PROBE_OK: connected + found pages" : "PROBE_PARTIAL: connected but no pages");
    await browser.close();
  } catch (e) {
    console.log("PROBE_FAIL connectOverCDP:", e.message);
    process.exit(2);
  }
})();
