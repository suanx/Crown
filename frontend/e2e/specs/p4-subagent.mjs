/**
 * P4：子代理（task 工具）+ 嵌套活动可见。
 *
 * 真实链路：要求 agent 用 task 工具(explore 子代理)调研当前目录 → 出现 task
 * 工具卡片 + 嵌套子代理面板（subagent-panel）显示子代理的工具调用。
 */
import {
  assert,
  gotoNewChat,
  sendAndRunTurn,
  toolCards,
} from "../lib.mjs";

export const name = "P4 子代理 task 工具";

export async function run(page) {
  await gotoNewChat(page);

  await sendAndRunTurn(
    page,
    "请使用 task 工具、agent_type 设为 explore，让子代理调研当前工作目录里有哪些文件，并把结果汇报给我。",
    { timeoutMs: 170_000 },
  );

  const cards = await toolCards(page);
  const taskCard = cards.find((c) => c.name === "task");
  assert(
    !!taskCard,
    `期望出现 task 工具卡片，实际工具: ${JSON.stringify(cards.map((c) => c.name))}`,
  );

  // 嵌套子代理面板应出现（展开 task 卡片后渲染）。先点开 task 卡片头部。
  // task 卡片默认展开（非 aborted），子代理面板应已在 DOM。
  const panels = await page.$$('[data-testid="subagent-panel"]');
  assert(
    panels.length > 0,
    "期望出现嵌套子代理活动面板(subagent-panel)，但未找到 — 子代理事件未冒泡/未渲染。",
  );

  return {
    detail: `task 卡片出现 + ${panels.length} 个子代理嵌套面板 ✓`,
  };
}
