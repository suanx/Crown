/**
 * 斜杠命令框架（最小可扩展）。
 *
 * 用户在 ComposeBar 输入 `/<name> <args>`。匹配到已注册命令则发送前经
 * `transform` 改写实际发给后端的内容；未匹配则原样发送（当普通消息）。
 *
 * 加新命令：往 SLASH_COMMANDS push 一个 SlashCommand 即可。
 *
 * 注：本仓库未配置 vitest，这些纯函数靠 tsc 类型检查 + dev 端到端验证。
 */

export interface SlashCommand {
  name: string;
  description: string;
  /** 把 `/name args` 的 args 部分转成发给后端的实际 content。 */
  transform(args: string): string;
}

const PLAN_REMINDER =
  "<system-reminder>\n" +
  "The user invoked /plan. First produce a clear, step-by-step plan for the " +
  "task below (investigate relevant code first, then list concrete numbered " +
  "steps). After presenting the plan, immediately proceed to execute it in the " +
  "current permission mode — do not wait for further confirmation unless a step " +
  "genuinely requires the user's input.\n" +
  "</system-reminder>";

export const PLAN_COMMAND: SlashCommand = {
  name: "plan",
  description: "先规划再执行：让 Agent 先列出步骤计划，然后用当前模式继续执行",
  transform: (args: string) => `${PLAN_REMINDER}\n\n${args}`.trimEnd(),
};

const CLARIFY_REMINDER =
  "<system-reminder>\n" +
  "The user invoked /clarify. Use the ask_user_question tool to turn the " +
  "(possibly vague) request below into a small set of structured multiple-choice " +
  "questions that clarify scope, preferences, and key decisions. Ask 1-4 focused " +
  "questions at once; do not ask for confirmation of a plan. If the request below " +
  "is empty, base your questions on the current conversation context.\n" +
  "</system-reminder>";

export const CLARIFY_COMMAND: SlashCommand = {
  name: "clarify",
  description: "结构化澄清：让 Agent 把模糊需求拆成多选题逐项问清",
  transform: (args: string) => `${CLARIFY_REMINDER}\n\n${args}`.trimEnd(),
};

export const BRAINSTORM_COMMAND: SlashCommand = {
  name: "brainstorm",
  description: "多 Agent 头脑风暴：不同专家按群聊消息依次发言",
  transform: (args: string) => `/brainstorm ${args}`.trimEnd(),
};

export const SLASH_COMMANDS: SlashCommand[] = [
  PLAN_COMMAND,
  CLARIFY_COMMAND,
  BRAINSTORM_COMMAND,
];

/** 解析输入开头的命令 token（`/name`），返回匹配的命令列表（前缀匹配）。 */
export function matchSlashCommand(input: string): SlashCommand[] {
  if (!input.startsWith("/")) return [];
  const token = input.slice(1).split(/\s/, 1)[0]?.toLowerCase() ?? "";
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(token));
}

/**
 * 若 input 是 `/<known> <args>` 形式，返回 transform 后的 content；
 * 否则返回 null（调用方原样发送 input）。
 */
export function applySlashCommand(input: string): string | null {
  if (!input.startsWith("/")) return null;
  const space = input.indexOf(" ");
  const name = (space === -1 ? input.slice(1) : input.slice(1, space)).toLowerCase();
  const args = space === -1 ? "" : input.slice(space + 1).trim();
  const cmd = SLASH_COMMANDS.find((c) => c.name === name);
  if (!cmd) return null;
  return cmd.transform(args);
}
