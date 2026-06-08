/**
 * E2E runner：连一次 CDP 真实 Tauri 窗口，依次跑指定 spec，汇总结果。
 *
 * 用法：
 *   node e2e/run-e2e.mjs                 # 跑全部 spec
 *   node e2e/run-e2e.mjs p0 p4           # 只跑名字含 p0 / p4 的 spec
 *
 * 前置：真实 Tauri 窗口已带 --remote-debugging-port=9333 启动（见 e2e/README.md）。
 */
import { connectApp } from "./lib.mjs";
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SPEC_DIR = path.join(__dirname, "specs");

const filters = process.argv.slice(2).map((s) => s.toLowerCase());

function pickSpecs() {
  const files = readdirSync(SPEC_DIR)
    .filter((f) => f.endsWith(".mjs"))
    .sort();
  if (filters.length === 0) return files;
  return files.filter((f) => filters.some((flt) => f.toLowerCase().includes(flt)));
}

(async () => {
  const specs = pickSpecs();
  if (specs.length === 0) {
    console.log("no specs matched", filters);
    process.exit(1);
  }
  console.log(`=== E2E: ${specs.length} spec(s) ===\n`, specs.join("\n "), "\n");

  const { browser, page } = await connectApp();
  const results = [];

  for (const file of specs) {
    const mod = await import(pathToFileURL(path.join(SPEC_DIR, file)).href);
    const label = mod.name || file;
    const t0 = Date.now();
    try {
      const out = await mod.run(page);
      const ms = Date.now() - t0;
      console.log(`✅ PASS  ${label}  (${(ms / 1000).toFixed(1)}s)  ${out?.detail ?? ""}`);
      results.push({ label, ok: true });
    } catch (e) {
      const ms = Date.now() - t0;
      console.log(`❌ FAIL  ${label}  (${(ms / 1000).toFixed(1)}s)\n     ${e.message}`);
      results.push({ label, ok: false, err: e.message });
    }
  }

  await browser.close();

  const passed = results.filter((r) => r.ok).length;
  console.log(`\n=== ${passed}/${results.length} passed ===`);
  process.exit(passed === results.length ? 0 : 1);
})().catch((e) => {
  console.log("RUNNER ERROR:", e.message);
  process.exit(1);
});
