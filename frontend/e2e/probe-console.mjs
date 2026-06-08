// 连上窗口，监听 console，点一次「允许」，看 IPC 是否成功 / 有无 fallback 警告。
import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts()) for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no page"); process.exit(1); }
pg.on("console", (m) => console.log("[console]", m.type(), m.text().slice(0, 300)));
// 当前若有「允许」按钮，点一次。
const allow = pg.locator("button", { hasText: /^允许$/ });
const n = await allow.count();
console.log("允许 buttons now:", n);
if (n > 0) {
  await allow.first().click();
  console.log("clicked 允许, waiting 8s for IPC/logs...");
  await pg.waitForTimeout(8000);
} else {
  console.log("no approval pending; waiting 5s to capture any logs");
  await pg.waitForTimeout(5000);
}
await b.close();
