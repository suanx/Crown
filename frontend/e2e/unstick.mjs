// 临时：点掉当前挂起的审批（点「允许」），让遗留 turn 走完。
import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts()) for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no page"); process.exit(1); }
// 审批 bar 里的「允许」按钮（文案精确匹配，避开「始终允许」）。
const allow = pg.locator('button', { hasText: /^允许$/ });
const n = await allow.count();
console.log("allow buttons:", n);
if (n > 0) {
  await allow.first().click();
  console.log("clicked 允许");
}
await b.close();
