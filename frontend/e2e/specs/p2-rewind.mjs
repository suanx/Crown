/**
 * P2：回溯（对话截断，UI 可见部分）。
 *
 * 真实链路：发消息①（产生一轮对话）→ 发消息②→ 悬浮消息②的用户气泡点
 * 「回到这里」→ 确认对话框自动接受 → getThread 重建 → 断言消息②及之后被截断。
 *
 * 注：文件还原由后端 rewind_test.rs 集成测试覆盖；此处验证 UI 触发的对话回溯。
 */
import {
  assert,
  gotoNewChat,
  sendAndRunTurn,
  bodyText,
} from "../lib.mjs";

export const name = "P2 回溯（对话截断）";

export async function run(page) {
  await gotoNewChat(page);

  // 第一轮：一个独特标记。
  const m1 = "REWIND-KEEP-" + Date.now().toString().slice(-4);
  await sendAndRunTurn(page, `记住并回复这个词：${m1}`);

  // 第二轮：另一个标记（这一轮将被回溯删除）。
  const m2 = "REWIND-DROP-" + Date.now().toString().slice(-4);
  await sendAndRunTurn(page, `记住并回复这个词：${m2}`);

  let body = await bodyText(page);
  assert(body.includes(m1), "第一轮标记应在");
  assert(body.includes(m2), "第二轮标记应在");

  // 悬浮第二条用户消息 → 点「回到这里」。用户消息按 DOM 顺序，第二条 index=1。
  const userMsgs = page.locator('[data-testid="user-message"]');
  const count = await userMsgs.count();
  assert(count >= 2, `期望至少 2 条用户消息，实际 ${count}`);
  const second = userMsgs.nth(1);
  await second.hover();
  await page.waitForTimeout(300);
  // 该消息内的「回到这里」按钮。
  const backBtn = second.locator('button[aria-label="回到这里"]');
  await backBtn.waitFor({ state: "attached", timeout: 5000 });
  await backBtn.click({ force: true }); // confirm 对话框由 lib 的 dialog 处理器自动接受

  await page.waitForTimeout(2500);
  body = await bodyText(page);

  // 回溯到第二条用户消息 → 它及之后被删 → m2 应消失，m1 保留。
  assert(body.includes(m1), `回溯后第一轮标记 ${m1} 应保留`);
  assert(
    !body.includes(m2),
    `回溯后第二轮标记 ${m2} 应被删除（对话截断），但仍出现。`,
  );

  return { detail: `回溯删除第二轮(${m2})，保留第一轮(${m1}) ✓` };
}
