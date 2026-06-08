/**
 * E2E harness：启动真实 Tauri 窗口（cargo tauri dev）并通过 CDP 连接。
 *
 * 真实链路：Playwright → CDP → 真实 WebView2 窗口 → 前端 (hybrid mode) →
 * Tauri invoke → 真 Rust 引擎 → 真大模型。后端零改动。
 *
 * 关键机制：WebView2 读环境变量 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`
 * 开远程调试端口；`cargo tauri dev` 的子 WebView2 进程继承它。
 */
import { spawn, type ChildProcess } from "node:child_process";
import { chromium, type Browser, type Page } from "playwright";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
/** deepseek-agent/frontend 根目录 */
export const FRONTEND_ROOT = path.resolve(__dirname, "..", "..");
/** deepseek-agent 仓库根目录 */
export const REPO_ROOT = path.resolve(FRONTEND_ROOT, "..");
/** Tauri app crate 根目录 */
export const TAURI_APP_ROOT = path.resolve(REPO_ROOT, "crates", "app");

export const CDP_PORT = Number(process.env.E2E_CDP_PORT ?? 9333);
export const VITE_PORT = Number(process.env.E2E_VITE_PORT ?? 5180);

export interface Harness {
  browser: Browser;
  page: Page;
  /** 主 app 页面（非 DevTools）。 */
  appPage: Page;
  dispose: () => Promise<void>;
}

async function waitFor(
  predicate: () => Promise<boolean>,
  { timeoutMs = 180_000, intervalMs = 1000, label = "condition" } = {},
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      if (await predicate()) return;
    } catch {
      /* keep polling */
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`timed out waiting for ${label} after ${timeoutMs}ms`);
}

async function httpOk(url: string): Promise<boolean> {
  const res = await fetch(url).catch(() => null);
  return !!res && res.ok;
}

/** 是否已有 Vite dev server 在 VITE_PORT 上。 */
async function viteUp(): Promise<boolean> {
  return httpOk(`http://localhost:${VITE_PORT}/`);
}

/** CDP /json/version 可达。 */
async function cdpUp(): Promise<boolean> {
  return httpOk(`http://127.0.0.1:${CDP_PORT}/json/version`);
}

let viteProc: ChildProcess | null = null;
let tauriProc: ChildProcess | null = null;

/**
 * 启动整套环境并连接。若 Vite 已在跑则复用，否则用 hybrid mode 拉起。
 * 然后 `cargo tauri dev` 起真实窗口（带 CDP 端口），最后 connectOverCDP。
 */
export async function launchHarness(): Promise<Harness> {
  // 1. Vite (hybrid mode → Tauri 环境内 invoke 走真后端)
  if (!(await viteUp())) {
    viteProc = spawn("npm", ["run", "dev"], {
      cwd: FRONTEND_ROOT,
      env: { ...process.env, VITE_API_MODE: "hybrid" },
      shell: true,
      stdio: "inherit",
    });
    await waitFor(viteUp, { label: "vite dev server", timeoutMs: 60_000 });
  }

  // 2. Tauri dev（编译最新后端 + 起真窗口 + 开 CDP 端口）
  const env = {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
    WEBVIEW2_USER_DATA_FOLDER:
      process.env.TEMP || process.env.TMP
        ? path.join(process.env.TEMP || process.env.TMP || ".", "dsagent-e2e")
        : undefined,
  } as NodeJS.ProcessEnv;

  tauriProc = spawn("cargo", ["tauri", "dev"], {
    cwd: TAURI_APP_ROOT,
    env,
    shell: true,
    stdio: "inherit",
  });

  // 3. 等 CDP 端口（首次编译可能很久）
  await waitFor(cdpUp, { label: "CDP port (tauri window)", timeoutMs: 600_000 });

  // 4. 连接 + 找主 app 页面
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${CDP_PORT}`);
  let appPage: Page | undefined;
  await waitFor(
    async () => {
      for (const ctx of browser.contexts()) {
        for (const p of ctx.pages()) {
          if (p.url().includes(`localhost:${VITE_PORT}`)) {
            appPage = p;
            return true;
          }
        }
      }
      return false;
    },
    { label: "app page", timeoutMs: 60_000 },
  );

  if (!appPage) throw new Error("app page not found over CDP");
  await appPage.bringToFront().catch(() => {});

  return {
    browser,
    page: appPage,
    appPage,
    dispose: async () => {
      await browser.close().catch(() => {});
      if (tauriProc) tauriProc.kill();
      if (viteProc) viteProc.kill();
    },
  };
}
