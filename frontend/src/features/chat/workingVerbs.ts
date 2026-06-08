/**
 * 进行时动词池 —— 给"工作中"指示器一点活气，每次随机换一个词。
 *
 * 常规词原创中文（非直译），低概率混入"摸鱼"系彩蛋词。参考 Claude Code 的
 * spinnerVerbs 思路（随机动词 + 进行时态），但词汇全部本地化重写。
 */

/** 常规进行时动词（高频出现）。 */
export const WORKING_VERBS: readonly string[] = [
  "思考中",
  "构思中",
  "推敲中",
  "琢磨中",
  "编排中",
  "梳理中",
  "盘算中",
  "捣鼓中",
  "运筹中",
  "码字中",
  "雕琢中",
  "拼装中",
  "演算中",
  "斟酌中",
  "组织中",
];

/** 彩蛋动词（低概率出现，活跃气氛）。 */
export const EASTER_EGG_VERBS: readonly string[] = [
  "摸鱼中",
  "放空中",
  "神游中",
  "划水中",
  "发呆中",
];

/** 彩蛋触发概率（~12%）。 */
export const EASTER_EGG_PROBABILITY = 0.12;

/**
 * 随机挑一个进行时动词。以 `EASTER_EGG_PROBABILITY` 的概率返回彩蛋词。
 * 接受可选的随机源（便于测试注入确定值）。
 */
export function pickWorkingVerb(rand: () => number = Math.random): string {
  const pool =
    rand() < EASTER_EGG_PROBABILITY ? EASTER_EGG_VERBS : WORKING_VERBS;
  const idx = Math.floor(rand() * pool.length) % pool.length;
  return pool[idx] ?? WORKING_VERBS[0]!;
}
