import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts()) for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no page"); process.exit(1); }
const btns = await pg.$$eval("button", (els) =>
  els.map((e) => ({ text: (e.innerText || "").trim().slice(0, 24), aria: e.getAttribute("aria-label"), testid: e.getAttribute("data-testid") }))
    .filter((b) => b.text || b.aria),
);
console.log(JSON.stringify(btns, null, 1));
await b.close();
