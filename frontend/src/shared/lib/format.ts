/**
 * 数字 / 费用 / token 显示工具.
 */

export function formatUsd(value: number): string {
  if (value < 0.01) return `$${value.toFixed(4)}`;
  if (value < 1) return `$${value.toFixed(3)}`;
  return `$${value.toFixed(2)}`;
}

export function formatTokens(n: number): string {
  if (n < 1_000) return n.toString();
  if (n < 1_000_000) return (n / 1_000).toFixed(n < 10_000 ? 1 : 0) + "k";
  return (n / 1_000_000).toFixed(1) + "M";
}

export function formatDuration(ms: number): string {
  if (ms < 1_000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

export function costLevel(usd: number): "ok" | "warn" | "high" {
  if (usd < 0.05) return "ok";
  if (usd < 0.2) return "warn";
  return "high";
}

/**
 * 余额格式化 — 透传 currency 字符串,不做汇率换算 (handoff §不在 P3a 范围).
 *
 *   formatBalance("CNY", 45.32)  → "¥45.32"
 *   formatBalance("USD", 12.5)   → "$12.50"
 *   formatBalance("EUR", 8.1)    → "EUR 8.10"
 */
export function formatBalance(currency: string, amount: number): string {
  const symbols: Record<string, string> = { CNY: "¥", USD: "$", EUR: "€" };
  const sym = symbols[currency.toUpperCase()];
  const fixed = amount.toFixed(2);
  return sym ? `${sym}${fixed}` : `${currency} ${fixed}`;
}

/**
 * 余额阈值 → 显示色调.handoff §UI 接通推荐 <5 红 / <20 黄 / 其他绿.
 * 这里返回 tone 名,UI 自己映射 Tailwind class.
 */
export function balanceTone(amount: number): "danger" | "warning" | "success" {
  if (amount < 5) return "danger";
  if (amount < 20) return "warning";
  return "success";
}

/**
 * USD → CNY 静态汇率.
 * 后端 MessageUsage.costUsd 是 USD,前端为统一显示做线性换算.
 * 后续若后端加 currency 元数据,这里改为真实汇率即可,接口稳定.
 */
const USD_TO_CNY_RATE = 7.2;

/**
 * 把 USD 成本换算成 CNY 字符串.
 * 微小金额保留 4 位小数避免显示成 ¥0.00,中等以上保留 2 位.
 *
 *   formatCostCny(0.000041)  → "¥0.0003"
 *   formatCostCny(0.0034)    → "¥0.024"
 *   formatCostCny(0.42)      → "¥3.02"
 *   formatCostCny(15.3)      → "¥110.16"
 */
export function formatCostCny(usd: number): string {
  const cny = usd * USD_TO_CNY_RATE;
  if (cny < 0.01) return `¥${cny.toFixed(4)}`;
  if (cny < 1) return `¥${cny.toFixed(3)}`;
  return `¥${cny.toFixed(2)}`;
}
