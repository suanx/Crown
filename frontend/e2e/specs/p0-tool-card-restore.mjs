/**
 * P0：重启/重载后工具卡片恢复（含执行结果），和工作时一模一样。
 *
 * 真实链路：新对话发一条会触发工具的消息 → 真模型调工具（自动批准）→
 * 卡片含结果 → reload 页面（清内存，触发从 DB 重建）→ 侧栏点回该 thread
 * → getThread 重建 → 断言卡片仍在且含结果。
 */
import {
  assert,
  gotoNewChat,
  sendAndRunTurn,
  toolCards,
} from "../lib.mjs";

export const name = "P0 工具卡片重载恢复";

export async function run(page) {
  await gotoNewChat(page);

  await sendAndRunTurn(
    page,
    "用 list_directory 工具列出当前工作目录的文件，然后用一句话说明。",
  );

  const before = await toolCards(page);
  assert(before.length > 0, `期望至少一个工具卡片，实际 ${before.length}`);
  const withResult = before.filter((c) => c.text.trim().length > 0);
  assert(
    withResult.length > 0,
    `期望工具卡片含执行结果，实际: ${JSON.stringify(before)}`,
  );
  const beforeCount = before.length;

  // reload：清空前端内存（routerStore 回 welcome），thread 仍在 DB。
  await page.reload();
  await page
    .locator('[data-testid="compose-input"]')
    .first()
    .waitFor({ state: "visible", timeout: 30_000 });

  // 侧栏点回最近的 thread（第一条）→ 触发 getThread 从 DB 重建。
  const firstThread = page.locator('[data-testid="session-item-open"]').first();
  await firstThread.waitFor({ state: "visible", timeout: 15_000 });
  await firstThread.click();
  await page.waitForTimeout(2500);

  const after = await toolCards(page);
  assert(
    after.length >= beforeCount,
    `重载后工具卡片消失！重载前 ${beforeCount} 个 → 重载后 ${after.length} 个（这正是 P0 的 bug）。`,
  );
  const afterWithResult = after.filter((c) => c.text.trim().length > 0);
  assert(
    afterWithResult.length > 0,
    `重载后工具卡片是空壳（无结果）。实际: ${JSON.stringify(after)}`,
  );

  return {
    detail: `重载前 ${beforeCount} 卡片(含结果) → 重载后 ${after.length} 卡片(含结果) ✓`,
  };
}
