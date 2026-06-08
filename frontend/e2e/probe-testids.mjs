import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts()) for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no app page"); process.exit(1); }
console.log("url:", pg.url());
for (const id of ["compose-input", "compose-send", "compose-stop", "tool-card", "user-message"]) {
  const n = (await pg.$$(`[data-testid="${id}"]`)).length;
  console.log(`  ${id}: ${n}`);
}
await b.close();
