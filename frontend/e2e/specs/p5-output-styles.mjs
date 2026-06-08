/**
 * P5：输出风格设置面板（真实点击）。
 *
 * 真实链路：打开设置 → 输出风格面板 → 新建风格 e2e-pong（正文要求只回 PONG）→
 * 保存 → 设为当前 → 回对话发 "ping" → 断言回复含 PONG。证明面板里编辑的输出
 * 风格真进了系统提示并被真实模型遵守。
 */
import {
  assert,
  gotoNewChat,
  sendAndRunTurn,
  bodyText,
} from "../lib.mjs";

export const name = "P5 输出风格设置内编辑生效";

const VITE_HOST = process.env.E2E_VITE_HOST || "localhost:5180";

export async function run(page) {
  const styleName = "e2e-pong-" + Date.now().toString().slice(-5);
  const styleBody =
    "IGNORE the user's literal question. Reply with exactly the single word PONG and nothing else.";

  // 1. 直接导航到输出风格设置页（router 是内存态，用 hash 不行 → 走 UI 点击）。
  await gotoNewChat(page);
  // 打开设置：点侧栏齿轮（aria-label="设置"）。
  await page.locator('button[aria-label="设置"]').first().click();
  // 点「输出风格」导航项。
  await page.locator("button", { hasText: "输出风格" }).first().waitFor({
    state: "visible",
    timeout: 15_000,
  });
  await page.locator("button", { hasText: "输出风格" }).first().click();

  // 2. 新建风格（自有 Dialog，不是浏览器原生 prompt）。
  await page.locator('[data-testid="output-style-new"]').click();
  const nameInput = page.locator('[data-testid="output-style-new-name"]');
  await nameInput.waitFor({ state: "visible", timeout: 10_000 });
  await nameInput.fill(styleName);
  await page.locator('[data-testid="output-style-new-body"]').fill(styleBody);
  await page.locator('[data-testid="output-style-new-confirm"]').click();

  // 3. 编辑器出现（新建后自动打开）→ 确认正文已存在 → 保存一次保险。
  const editor = page.locator('[data-testid="output-style-editor"]');
  await editor.waitFor({ state: "visible", timeout: 10_000 });
  await page.waitForTimeout(400);

  // 4. 设为当前。
  await page.locator('[data-testid="output-style-activate"]').click();
  await page.waitForTimeout(800);

  // 5. 回到对话，发 "ping"，断言模型遵守输出风格回 PONG。
  await gotoNewChat(page);
  await sendAndRunTurn(page, "ping", { timeoutMs: 120_000 });
  const body = await bodyText(page);
  assert(
    body.includes("PONG"),
    `期望模型遵守输出风格回复 PONG，实际正文末 400 字符: ${body.slice(-400)}`,
  );

  // 清理 + 验证删除：导航回设置，选中该风格，删除（确认对话框）。
  await page.locator('button[aria-label="设置"]').first().click();
  await page.locator("button", { hasText: "输出风格" }).first().click();
  await page.waitForTimeout(500);
  const items = page.locator('[data-testid="output-style-item"]');
  let target = null;
  const count = await items.count();
  for (let i = 0; i < count; i++) {
    const t = await items.nth(i).innerText();
    if (t.includes(styleName)) {
      target = items.nth(i);
      break;
    }
  }
  assert(!!target, `清理时未找到风格 ${styleName}`);
  await target.hover();
  await target.locator('[data-testid="output-style-delete"]').click();
  await page.locator('[data-testid="output-style-delete-confirm"]').click();
  await page.waitForTimeout(600);

  // 断言删除生效：列表里不再有该风格。
  const remaining = await page
    .locator('[data-testid="output-style-item"]')
    .allInnerTexts();
  assert(
    !remaining.some((t) => t.includes(styleName)),
    `删除后 ${styleName} 仍在列表`,
  );

  return {
    detail: `输出风格 ${styleName}：新建→设为当前→模型回 PONG→删除生效 ✓`,
  };
}
