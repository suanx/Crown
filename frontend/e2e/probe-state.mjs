import { chromium } from "playwright";
const b = await chromium.connectOverCDP("http://127.0.0.1:9333");
let pg;
for (const c of b.contexts()) for (const p of c.pages()) if (p.url().includes("5180")) pg = p;
if (!pg) { console.log("no app page"); process.exit(1); }
const txt = await pg.evaluate(() => document.body.innerText);
console.log("URL:", pg.url());
console.log("LEN:", txt.length);
console.log("---- body (first 1200) ----");
console.log(txt.slice(0, 1200).replace(/\n{2,}/g, "\n"));
await b.close();
