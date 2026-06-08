/**
 * 把 ISO timestamp 列表按 "Today / Yesterday / This Week / This Month / Older"
 * 分组,用于 Sidebar 会话列表.
 */

export type TimeBucket = "today" | "yesterday" | "thisWeek" | "thisMonth" | "older";

export const BUCKET_LABELS: Record<TimeBucket, string> = {
  today: "今天",
  yesterday: "昨天",
  thisWeek: "本周",
  thisMonth: "本月",
  older: "更早",
};

const DAY_MS = 24 * 60 * 60 * 1000;

export function bucketOf(timestamp: string | number, now = Date.now()): TimeBucket {
  const t = typeof timestamp === "string" ? new Date(timestamp).getTime() : timestamp;
  const diff = now - t;
  const today0 = new Date(now);
  today0.setHours(0, 0, 0, 0);
  const t0 = today0.getTime();

  if (t >= t0) return "today";
  if (t >= t0 - DAY_MS) return "yesterday";
  if (diff <= 7 * DAY_MS) return "thisWeek";
  if (diff <= 30 * DAY_MS) return "thisMonth";
  return "older";
}

export function formatRelative(timestamp: string | number, now = Date.now()): string {
  const t = typeof timestamp === "string" ? new Date(timestamp).getTime() : timestamp;
  const diff = Math.max(0, now - t);
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "刚刚";
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d} 天前`;
  const date = new Date(t);
  return `${date.getMonth() + 1}/${date.getDate()}`;
}

/** 群组化 + 保持组内倒序 (新 → 旧). */
export function groupByBucket<T extends { updatedAt: string }>(
  items: T[],
  now = Date.now(),
): Array<{ bucket: TimeBucket; items: T[] }> {
  const buckets: Record<TimeBucket, T[]> = {
    today: [],
    yesterday: [],
    thisWeek: [],
    thisMonth: [],
    older: [],
  };
  for (const it of items) {
    buckets[bucketOf(it.updatedAt, now)].push(it);
  }
  const order: TimeBucket[] = ["today", "yesterday", "thisWeek", "thisMonth", "older"];
  return order
    .map((b) => ({ bucket: b, items: buckets[b] }))
    .filter((g) => g.items.length > 0);
}
