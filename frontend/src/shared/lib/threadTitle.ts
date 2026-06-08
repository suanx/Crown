/**
 * 会话显示标题 —— 解决"每个对话都显示 New chat"。
 *
 * 后端 ThreadSummaryDto.title = name.unwrap_or("New chat")：会话还没生成/
 * 回写 name 时就是默认值。但 preview（最后一条消息的预览）后端一直在维护。
 * 这里做 fallback 链：有真实标题用标题，否则退到 preview（用户最近的话），
 * 再否则才显示"新对话"。
 *
 * 默认标题集合与 chatStore.generateTitleFromFirstMessage 保持一致。
 */

const DEFAULT_TITLES = new Set([
  "New Chat",
  "new chat",
  "New chat",
  "新对话",
  "新建对话",
  "",
]);

/** 标题是否是后端/前端的占位默认值（需要 fallback 到 preview）。 */
export function isDefaultTitle(title: string): boolean {
  return DEFAULT_TITLES.has(title.trim());
}

/**
 * 计算会话列表/标题栏要显示的文本。
 *
 * @param title 后端返回的 title（可能是 "New chat" 占位）
 * @param preview 最后一条消息预览（后端维护，可能为 null）
 * @param max 截断长度（默认 40，列表项够用）
 */
export function displayThreadTitle(
  title: string,
  preview: string | null | undefined,
  max = 40,
): string {
  const t = title.trim();
  if (!isDefaultTitle(t)) return clip(t, max);

  const p = (preview ?? "").trim();
  if (p) return clip(p, max);

  return "新对话";
}

function clip(s: string, max: number): string {
  // 按 Unicode 码点截断，避免把 emoji/星体字符切成残缺代理对。
  const cps = Array.from(s);
  if (cps.length <= max) return s;
  return cps.slice(0, max).join("") + "…";
}
