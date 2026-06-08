import { useMemo, useState } from "react";
import { useSessionStore } from "@/stores/sessionStore";
import { useRouterStore } from "@/stores/routerStore";
import { groupByBucket, BUCKET_LABELS } from "@/shared/lib/time";
import { SessionItem } from "./SessionItem";

/**
 * 会话列表 — 按时间桶分组.
 *
 * 间距档:
 *   - 列表区横向 padding: 8 (px-2)
 *   - 分组之间垂直 padding: 8 (mb-2 后再 mt-1 头部)
 *   - 分组标签横向 padding: 8 (px-2),与 item 对齐
 */
export function SessionList() {
  const threads = useSessionStore((s) => s.threads);
  const navigate = useRouterStore((s) => s.navigate);
  const route = useRouterStore((s) => s.current);
  const activeId = route.page === "chat" ? route.threadId : null;

  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const groups = useMemo(() => groupByBucket(threads), [threads]);

  return (
    <div className="flex-1 min-h-0 scrollable px-2 pb-2">
      {groups.map((g, gi) => (
        <div key={g.bucket} className={gi > 0 ? "mt-3" : ""}>
          <div className="sticky top-0 bg-elevated px-2 pb-1 pt-1 text-xs text-text-tertiary z-10">
            {BUCKET_LABELS[g.bucket]}
          </div>
          <div className="space-y-0.5">
            {g.items.map((t) => (
              <SessionItem
                key={t.id}
                thread={t}
                active={t.id === activeId}
                menuOpen={openMenuId === t.id}
                onClick={() => navigate({ page: "chat", threadId: t.id })}
                onToggleMenu={() =>
                  setOpenMenuId(openMenuId === t.id ? null : t.id)
                }
                onCloseMenu={() => setOpenMenuId(null)}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
