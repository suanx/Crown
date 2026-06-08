import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts())
  for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no page"); process.exit(1); }

await pg.goto("http://localhost:5180/").catch(() => {});
await pg.locator('[data-testid="compose-input"]').first().waitFor({ state: "visible", timeout: 30000 });
// wait for __stores to be wired
await pg.waitForFunction(() => !!window.__stores, null, { timeout: 10000 }).catch(() => {});

const ta = pg.locator('[data-testid="compose-input"]').first();
await ta.click();
const marker = "STORE-" + Date.now().toString().slice(-4);
await ta.fill(marker);
await pg.keyboard.press("Enter");
await pg.waitForTimeout(6000);

const dump = await pg.evaluate(() => {
  const s = window.__stores;
  if (!s) return { err: "no __stores" };
  const route = s.router.getState().current;
  const chat = s.chat.getState();
  const tid = route.page === "chat" ? route.threadId : null;
  const t = tid ? chat.threadsById[tid] : null;
  return {
    route,
    activeThreadId: tid,
    threadLoaded: !!t,
    msgRoles: t ? t.messages.map((m) => `${m.role}:${(m.content || "").slice(0, 20)}`) : null,
    msgCount: t ? t.messages.length : null,
    pendingTurn: tid ? chat.pendingTurnByThread[tid] : null,
  };
});
console.log("marker:", marker);
console.log(JSON.stringify(dump, null, 2));
await b.close();
