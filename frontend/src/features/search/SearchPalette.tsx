import { useEffect, useRef, useState } from "react";
import { useRouterStore } from "@/stores/routerStore";
import { useSessionStore } from "@/stores/sessionStore";
import { agentClient, type ThreadSummary } from "@/api";
import { Dialog } from "@/shared/ui/Dialog";
import { Icon } from "@/shared/icons/Icon";
import {
  SearchIcon,
  ChatIcon,
  SkillIcon,
  CaretRightIcon,
} from "@/shared/icons/set";
import { formatRelative } from "@/shared/lib/time";
import { cn } from "@/shared/lib/cn";

/**
 * 全局搜索 — Cmd+K / Ctrl+K 唤起.
 *
 * 搜索对话走 agentClient.searchThreads (后端 SQL 全表扫描,不限已加载列表).
 * 200ms debounce.空 query 时 fallback 到 sessionStore.threads 的前 8 条.
 */
export function SearchPalette() {
  const open = useRouterStore((s) => s.searchOpen);
  const close = () => useRouterStore.getState().toggleSearch(false);
  const navigate = useRouterStore((s) => s.navigate);

  const fallbackThreads = useSessionStore((s) => s.threads);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ThreadSummary[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Debounced search
  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    const q = query.trim();
    if (!q) {
      setResults([]);
      return;
    }
    timerRef.current = setTimeout(() => {
      void agentClient.searchThreads(q).then(setResults);
    }, 200);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [query]);

  // Items: debounced results, or fallback top-8 when empty query
  const displayThreads = query.trim()
    ? results.slice(0, 8)
    : fallbackThreads.slice(0, 8);

  const items = [
    ...displayThreads.map((t) => ({
      kind: "thread" as const,
      id: t.id,
      title: t.title,
      subtitle: formatRelative(t.updatedAt),
      action: () => {
        navigate({ page: "chat", threadId: t.id });
        close();
      },
    })),
    ...[
      { kind: "page" as const, label: "技能", icon: SkillIcon, route: { page: "skills" as const } },
    ]
      .filter((f) => !query.trim() || f.label.includes(query.trim()))
      .map((f) => ({
        kind: "page" as const,
        id: `page-${f.label}`,
        title: `打开 · ${f.label}`,
        subtitle: "页面",
        icon: f.icon,
        action: () => {
          navigate(f.route);
          close();
        },
      })),
  ];

  // 键盘上下选择
  const [activeIdx, setActiveIdx] = useState(0);
  useEffect(() => setActiveIdx(0), [items.length, query]);

  function handleKey(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(items.length - 1, i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(0, i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[activeIdx]?.action();
    }
  }

  if (!open) return null;
  return (
    <Dialog open onClose={close} className="max-w-[640px] !max-h-[60vh]">
      <div className="flex flex-col">
        {/* 输入框 */}
        <div className="flex items-center gap-3 px-4 h-12 border-b border-border-subtle">
          <Icon
            icon={SearchIcon}
            size={16}
            className="text-text-tertiary shrink-0"
          />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKey}
            placeholder="搜索对话 / 项目 / 技能..."
            className="flex-1 bg-transparent outline-none text-base text-text-primary placeholder:text-text-tertiary"
          />
          <kbd className="text-xs font-mono text-text-tertiary">ESC</kbd>
        </div>

        {/* 结果列表 */}
        <div className="flex-1 min-h-0 scrollable py-1">
          {items.length === 0 ? (
            <div className="px-4 py-12 text-center text-sm text-text-tertiary">
              没有匹配项
            </div>
          ) : (
            items.map((it, i) => {
              const ItemIcon =
                it.kind === "thread" ? ChatIcon : (it as any).icon;
              return (
                <button
                  key={it.id}
                  onClick={it.action}
                  onMouseEnter={() => setActiveIdx(i)}
                  className={cn(
                    "w-full flex items-center gap-3 px-4 h-10 text-left transition-colors",
                    i === activeIdx
                      ? "bg-hover text-text-primary"
                      : "text-text-secondary hover:bg-hover",
                  )}
                >
                  <Icon icon={ItemIcon} size={14} className="text-text-tertiary shrink-0" />
                  <span className="flex-1 min-w-0 text-sm truncate">
                    {it.title}
                  </span>
                  <span className="text-xs text-text-tertiary shrink-0">
                    {it.subtitle}
                  </span>
                  <Icon
                    icon={CaretRightIcon}
                    size={11}
                    className={cn(
                      "shrink-0",
                      i === activeIdx ? "text-text-secondary" : "text-text-tertiary opacity-0",
                    )}
                  />
                </button>
              );
            })
          )}
        </div>

        {/* footer hint */}
        <div className="px-4 h-9 border-t border-border-subtle flex items-center justify-between text-xs text-text-tertiary">
          <span>
            <kbd className="font-mono">↑↓</kbd> 选择 ·{" "}
            <kbd className="font-mono">↵</kbd> 打开
          </span>
          <span>{items.length} 项</span>
        </div>
      </div>
    </Dialog>
  );
}
