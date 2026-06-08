/**
 * P1：abort 穿透 + 中止后历史合法。
 *
 * 真实链路：让 agent 跑一个长命令（自动批准）→ 运行中点「停止」→
 * 断言 turn 结束、能立刻再发消息（历史合法，不 400）。
 */
import {
  assert,
  gotoNewChat,
  sendMessage,
  startAutoApprove,
  waitForTurnEnd,
  bodyText,
} from "../lib.mjs";

export const name = "P1 abort 中止";

export async function run(page) {
  await gotoNewChat(page);

  const auto = startAutoApprove(page);
  // 让模型跑一个会持续十多秒的命令。
  await sendMessage(
    page,
    "用 run_command 运行：ping -n 20 127.0.0.1（这会跑约20秒）。直接运行，不要解释。",
  );

  // 等到出现「停止」按钮（streaming/running 中）。
  const stop = page.locator('[data-testid="compose-stop"]');
  await stop.waitFor({ state: "visible", timeout: 60_000 });

  // 让它真的跑起来一会，再中止。
  await page.waitForTimeout(3000);
  await stop.click();

  // turn 应结束（发送按钮回来）。
  await waitForTurnEnd(page, 60_000);
  auto.stop();

  // 关键：中止后能立刻再发一条消息且正常回复（历史合法，不报错）。
  const probe = "PING-AFTER-ABORT-" + Date.now().toString().slice(-4);
  const auto2 = startAutoApprove(page);
  await sendMessage(page, `回复这一个词：${probe}`);
  const start = Date.now();
  let ok = false;
  while (Date.now() - start < 90_000) {
    const b = await bodyText(page);
    if (b.split(probe).length - 1 >= 2) {
      ok = true;
      break;
    }
    await page.waitForTimeout(1500);
  }
  auto2.stop();
  assert(ok, "中止后再发消息未能正常回复 — 可能历史不合法（assistant tool_calls 缺结果导致 400）");

  return { detail: "长命令中止 + 中止后对话仍可继续（历史合法）✓" };
}
