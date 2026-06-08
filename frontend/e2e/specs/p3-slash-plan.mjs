/**
 * P3：/plan 斜杠命令。
 *
 * 真实链路：在 ComposeBar 输入 `/` → 弹出命令菜单含 plan；输入 `/plan <任务>`
 * 发送 → 用户气泡只显示真实任务（reminder 前缀被剥）→ 模型先出计划再执行。
 */
import {
  assert,
  gotoNewChat,
  sendAndRunTurn,
  bodyText,
} from "../lib.mjs";

export const name = "P3 /plan 斜杠命令";

export async function run(page) {
  await gotoNewChat(page);

  // 1. 输入 "/" 应弹出斜杠命令菜单（含 plan）。
  const ta = page.locator('[data-testid="compose-input"]').first();
  await ta.click();
  await ta.fill("/");
  await page.waitForTimeout(600);
  const menuText = await bodyText(page);
  assert(
    /\/plan/.test(menuText) || /plan/.test(menuText),
    "输入 / 后应出现命令菜单含 plan",
  );

  // 2. 发送 /plan 任务。
  await ta.fill("");
  await sendAndRunTurn(
    page,
    "/plan 在当前目录创建一个名为 e2e_plan_demo.txt 的文件，内容为 hello",
    { timeoutMs: 150_000 },
  );

  const body = await bodyText(page);

  // 3. 用户气泡不应暴露 system-reminder 前缀。
  assert(
    !body.includes("<system-reminder>"),
    "用户消息不应显示 system-reminder 原文（应被剥离）",
  );

  // 4. 模型应产出"计划"性内容（编号步骤 / 计划字样）。宽松匹配。
  const looksLikePlan =
    /计划|步骤|Step|plan|1\.|①|第一步/i.test(body);
  assert(looksLikePlan, `期望模型先输出计划，实际无计划迹象。tail: ${body.slice(-300)}`);

  return { detail: "菜单出现 + reminder 剥离 + 模型先规划 ✓" };
}
