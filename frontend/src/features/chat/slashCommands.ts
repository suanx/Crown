/**
 * 斜杠命令框架（最小可扩展）。
 *
 * 用户在 ComposeBar 输入 `/<name> <args>`。匹配到已注册命令则发送前经
 * `transform` 改写实际发给后端的内容；未匹配则原样发送（当普通消息）。
 *
 * 加新命令：往 SLASH_COMMANDS push 一个 SlashCommand 即可。
 */

export interface SlashCommand {
  name: string;
  label: string;
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
  label: "规划",
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
  label: "澄清",
  description: "结构化澄清：让 Agent 把模糊需求拆成多选题逐项问清",
  transform: (args: string) => `${CLARIFY_REMINDER}\n\n${args}`.trimEnd(),
};

export const BRAINSTORM_COMMAND: SlashCommand = {
  name: "brainstorm",
  label: "脑暴",
  description: "多 Agent 头脑风暴：不同专家按群聊消息依次发言",
  transform: (args: string) => `/brainstorm ${args}`.trimEnd(),
};

/**
 * 用最简单的方案解决问题——刚刚好，不多不少。
 * 避免过度设计、过度抽象、过度注释。只做必要的改动。
 */
const SIMPLE_REMINDER =
  "<system-reminder>\n" +
  "IMPORTANT: Use the simplest approach that works. Do not over-engineer.\n" +
  "- No unnecessary abstractions, patterns, or configurations\n" +
  "- No extra comments unless the WHY is non-obvious\n" +
  "- No error handling for impossible scenarios\n" +
  "- No backwards-compatibility hacks\n" +
  "- Three similar lines are better than one prematurely abstracted function\n" +
  "- Only add validation at system boundaries\n" +
  "- Do exactly what was asked, nothing more\n" +
  "</system-reminder>";

export const SIMPLE_COMMAND: SlashCommand = {
  name: "simple",
  label: "极简",
  description: "用最简单的方式实现，不多余设计，不提前抽象",
  transform: (args: string) => `${SIMPLE_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 安全优先——写代码时考虑安全、边界情况、错误处理。
 */
const SAFE_REMINDER =
  "<system-reminder>\n" +
  "IMPORTANT: Prioritize correctness and safety:\n" +
  "- Handle all error paths explicitly\n" +
  "- Validate all external inputs\n" +
  "- Consider edge cases (empty, null, overflow, race conditions)\n" +
  "- Never swallow exceptions silently\n" +
  "- Use types to make illegal states unrepresentable\n" +
  "- Add tests for critical paths and edge cases\n" +
  "- Thread safety: be explicit about shared mutable state\n" +
  "</system-reminder>";

export const SAFE_COMMAND: SlashCommand = {
  name: "safe",
  label: "稳健",
  description: "注重正确性和安全性：完整错误处理、边界条件、防御式编程",
  transform: (args: string) => `${SAFE_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 详细解释——让 Agent 详细解释代码或概念。
 */
const EXPLAIN_REMINDER =
  "<system-reminder>\n" +
  "The user wants a detailed explanation. Be thorough:\n" +
  "- Explain what this does, why it's done this way, and trade-offs\n" +
  "- Include key concepts, patterns, and terminology\n" +
  "- Point out non-obvious implications and gotchas\n" +
  "- Use examples where helpful\n" +
  "</system-reminder>";

export const EXPLAIN_COMMAND: SlashCommand = {
  name: "explain",
  label: "解释",
  description: "详细解释代码或概念：原理、权衡、注意事项",
  transform: (args: string) => `${EXPLAIN_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 代码审查——让 Agent 做代码审查。
 */
const REVIEW_REMINDER =
  "<system-reminder>\n" +
  "You are doing a code review. Check for:\n" +
  "1. Logic correctness and edge cases\n" +
  "2. Security vulnerabilities (OWASP Top 10)\n" +
  "3. Performance issues (N+1, unnecessary allocations, etc.)\n" +
  "4. Maintainability (naming, complexity, duplication)\n" +
  "5. Error handling (are all failure paths covered?)\n" +
  "6. Test coverage\n" +
  "Format: **Severity** → Description → Suggestion\n" +
  "</system-reminder>";

export const REVIEW_COMMAND: SlashCommand = {
  name: "review",
  label: "审查",
  description: "全面代码审查：逻辑/安全/性能/可维护性",
  transform: (args: string) => `${REVIEW_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 写测试——让 Agent 为代码编写测试。
 */
const TEST_REMINDER =
  "<system-reminder>\n" +
  "Write tests for the following code. Follow these principles:\n" +
  "- Test behavior, not implementation\n" +
  "- Cover: happy path, edge cases, error conditions\n" +
  "- Use descriptive test names (should_xxx_when_yyy)\n" +
  "- One assertion pattern per test where practical\n" +
  "- Don't mock what you don't own\n" +
  "</system-reminder>";

export const TEST_COMMAND: SlashCommand = {
  name: "test",
  label: "测试",
  description: "为代码编写完整测试：覆盖正常/边界/异常路径",
  transform: (args: string) => `${TEST_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 重构——让 Agent 安全重构代码。
 */
const REFACTOR_REMINDER =
  "<system-reminder>\n" +
  "Refactor the following code safely:\n" +
  "- Keep behavior unchanged — no bug fixes or feature additions\n" +
  "- Make small, reversible changes\n" +
  "- Run tests after each change\n" +
  "- Use meaningful names and extract clear abstractions\n" +
  "- Reduce complexity and duplication\n" +
  "</system-reminder>";

export const REFACTOR_COMMAND: SlashCommand = {
  name: "refactor",
  label: "重构",
  description: "安全重构代码：提取函数、简化逻辑、改善命名",
  transform: (args: string) => `${REFACTOR_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 优化——让 Agent 优化性能。
 */
const OPTIMIZE_REMINDER =
  "<system-reminder>\n" +
  "Optimize the following code for performance:\n" +
  "- Profile before optimizing — identify actual bottlenecks\n" +
  "- Focus on algorithmic improvements (O(n²) → O(n log n))\n" +
  "- Reduce allocations and copies\n" +
  "- Consider caching strategies\n" +
  "- Benchmark to verify improvements\n" +
  "- Do not sacrifice readability for premature optimization\n" +
  "</system-reminder>";

export const OPTIMIZE_COMMAND: SlashCommand = {
  name: "optimize",
  label: "优化",
  description: "性能优化：算法改进、减少分配、缓存策略",
  transform: (args: string) => `${OPTIMIZE_REMINDER}\n\n${args}`.trimEnd(),
};

/**
 * 翻译——让 Agent 翻译文本。
 */
const TRANSLATE_REMINDER =
  "<system-reminder>\n" +
  "Translate the following text. Rules:\n" +
  "- Maintain original tone and style (formal/casual/technical)\n" +
  "- Code blocks and technical terms keep original\n" +
  "- Markdown formatting stays intact\n" +
  "- Preserve placeholders like {name}, %s, etc.\n" +
  "</system-reminder>";

export const TRANSLATE_COMMAND: SlashCommand = {
  name: "translate",
  label: "翻译",
  description: "翻译文本：保持语气和格式，保留代码占位符",
  transform: (args: string) => `${TRANSLATE_REMINDER}\n\n${args}`.trimEnd(),
};

export const SLASH_COMMANDS: SlashCommand[] = [
  PLAN_COMMAND,
  CLARIFY_COMMAND,
  BRAINSTORM_COMMAND,
  SIMPLE_COMMAND,
  SAFE_COMMAND,
  EXPLAIN_COMMAND,
  REVIEW_COMMAND,
  TEST_COMMAND,
  REFACTOR_COMMAND,
  OPTIMIZE_COMMAND,
  TRANSLATE_COMMAND,
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
