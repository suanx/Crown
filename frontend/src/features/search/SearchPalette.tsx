import { useEffect, useRef, useState } from "react";
import { useRouterStore } from "@/stores/routerStore";
import { useSessionStore } from "@/stores/sessionStore";
import {
  agentClient,
  type GrepMatch,
  type FsEntry,
  type MessageSearchResult,
  type ThreadSummary,
} from "@/api";
import { Dialog } from "@/shared/ui/Dialog";
import { Icon } from "@/shared/icons/Icon";
import {
  SearchIcon,
  ChatIcon,
  FileSearchIcon,
  FileIcon,
  FolderIcon,
  CaretRightIcon,
} from "@/shared/icons/set";
import { formatRelative } from "@/shared/lib/time";
import { cn } from "@/shared/lib/cn";

type SearchMode = "threads" | "grep" | "glob" | "messages";

const MODES: Array<{ id: SearchMode; label: string; icon: typeof SearchIcon }> = [
  { id: "threads", label: "对话", icon: ChatIcon },
  { id: "messages", label: "消息", icon: SearchIcon },
  { id: "grep", label: "文件内容", icon: FileSearchIcon },
  { id: "glob", label: "文件名", icon: FileIcon },
];

export function SearchPalette() {
  const open = useRouterStore((s) => s.searchOpen);
  const close = () => useRouterStore.getState().toggleSearch(false);
  const navigate = useRouterStore((s) => s.navigate);

  const fallbackThreads = useSessionStore((s) => s.threads);
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("threads");
  const [results, setResults] = useState<(GrepMatch | FsEntry | MessageSearchResult | ThreadSummary)[]>([]);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setMode("threads");
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Debounced search
  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    const q = query.trim();
    if (q.length < 1 || mode === "threads") {
      if (mode === "threads" && !q) { setResults([]); return; }
    }
    if (!q) return;
    setLoading(true);
    timerRef.current = setTimeout(async () => {
      try {
        switch (mode) {
          case "threads": {
            const r = await agentClient.searchThreads(q);
            setResults(r.slice(0, 8));
            break;
          }
          case "messages": {
            const r = await agentClient.searchMessages(q, 30);
            setResults(r);
            break;
          }
          case "grep": {
            const r = await agentClient.fsGrep(q, undefined, undefined, 50);
            setResults(r);
            break;
          }
          case "glob": {
            const r = await agentClient.fsGlob(q, undefined, 100);
            setResults(r);
            break;
          }
        }
      } catch { setResults([]); }
      finally { setLoading(false); }
    }, 200);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [query, mode]);

  const threadResults = mode === "threads" && query.trim()
    ? results as ThreadSummary[]
    : mode === "threads" && !query.trim()
      ? fallbackThreads.slice(0, 8)
      : [];

  const handleSelect = (item: any) => {
    if (mode === "threads") {
      navigate({ page: "chat", threadId: item.id });
    } else if (mode === "messages") {
      navigate({ page: "chat", threadId: item.threadId });
    }
    close();
  };

  function handleKey(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") { e.preventDefault(); setActiveIdx(i => Math.min(totalItems - 1, i + 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setActiveIdx(i => Math.max(0, i - 1)); }
    else if (e.key === "Enter") { e.preventDefault(); handleSelect(allItems[activeIdx]); }
  }

  const allItems = mode === "threads" ? threadResults : results;
  const totalItems = allItems.length;

  useEffect(() => setActiveIdx(0), [totalItems, query, mode]);

  if (!open) return null;

  return (
    <Dialog open onClose={close} className="max-w-[680px] !max-h-[70vh]">
      <div className="flex flex-col">
        {/* Mode tabs */}
        <div className="flex items-center gap-0.5 px-3 pt-2 pb-0 border-b border-border-subtle">
          {MODES.map((m) => (
            <button
              key={m.id}
              onClick={() => { setMode(m.id); setResults([]); setActiveIdx(0); }}
              className={cn(
                "flex items-center gap-1.5 h-8 px-3 text-xs rounded-t-md transition-colors font-medium",
                mode === m.id
                  ? "bg-elevated text-text-primary border border-border-default border-b-transparent -mb-px"
                  : "text-text-tertiary hover:text-text-secondary hover:bg-hover",
              )}
            >
              <Icon icon={m.icon} size={12} />
              {m.label}
            </button>
          ))}
        </div>

        {/* Input */}
        <div className="flex items-center gap-3 px-4 h-11 border-b border-border-subtle">
          <Icon icon={SearchIcon} size={15} className="text-text-tertiary shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKey}
            placeholder={
              mode === "grep" ? "搜索文件内容（支持正则）..." :
              mode === "glob" ? "搜索文件名（支持通配符 *.ts）..." :
              mode === "messages" ? "搜索对话消息内容..." :
              "搜索对话..."
            }
            className="flex-1 bg-transparent outline-none text-sm text-text-primary placeholder:text-text-tertiary"
          />
          {loading && (
            <span className="text-xs text-text-tertiary animate-pulse">搜索中...</span>
          )}
          <kbd className="text-xs font-mono text-text-tertiary">ESC</kbd>
        </div>

        {/* Results */}
        <div className="flex-1 min-h-0 scrollable py-1 max-h-[50vh]">
          {mode !== "threads" && query.trim().length > 0 && results.length === 0 && !loading && (
            <div className="px-4 py-12 text-center text-sm text-text-tertiary">没有匹配项</div>
          )}
          {allItems.length > 0 && allItems.map((item, i) => (
            <button
              key={`${i}`}
              onClick={() => handleSelect(item)}
              onMouseEnter={() => setActiveIdx(i)}
              className={cn(
                "w-full flex items-center gap-3 px-4 min-h-[36px] text-left transition-colors",
                i === activeIdx ? "bg-hover text-text-primary" : "text-text-secondary hover:bg-hover",
              )}
            >
              {mode === "grep" && (
                <>
                  <Icon icon={FileSearchIcon} size={12} className="text-text-tertiary shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-xs text-text-primary truncate font-mono">
                      {(item as GrepMatch).path}
                      <span className="text-text-tertiary ml-1">:{(item as GrepMatch).lineNumber}</span>
                    </div>
                    <div className="text-xs text-text-tertiary truncate mt-0.5 font-mono">
                      {(item as GrepMatch).line}
                    </div>
                  </div>
                </>
              )}
              {mode === "glob" && (
                <>
                  <Icon icon={(item as FsEntry).isDir ? FolderIcon : FileIcon} size={12} className="text-text-tertiary shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-xs text-text-primary truncate font-mono">{(item as FsEntry).path}</div>
                    <div className="text-xs text-text-tertiary">{(item as FsEntry).size} B</div>
                  </div>
                </>
              )}
              {mode === "messages" && (
                <>
                  <Icon icon={SearchIcon} size={12} className="text-text-tertiary shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-xs text-text-primary truncate">
                      <span className="font-medium">{(item as MessageSearchResult).threadTitle || "未命名对话"}</span>
                      <span className="text-text-tertiary ml-2 text-[10px]">{(item as MessageSearchResult).role}</span>
                    </div>
                    <div className="text-xs text-text-tertiary truncate mt-0.5">
                      {(item as MessageSearchResult).contentPreview}
                    </div>
                  </div>
                </>
              )}
              {mode === "threads" && (
                <>
                  <Icon icon={ChatIcon} size={12} className="text-text-tertiary shrink-0" />
                  <span className="flex-1 min-w-0 text-xs truncate">{(item as ThreadSummary).title || "新对话"}</span>
                  <span className="text-[10px] text-text-tertiary shrink-0">{formatRelative((item as ThreadSummary).updatedAt)}</span>
                </>
              )}
              <Icon icon={CaretRightIcon} size={10} className={cn("shrink-0", i === activeIdx ? "text-text-secondary" : "text-text-tertiary opacity-0")} />
            </button>
          ))}
        </div>

        {/* Footer */}
        <div className="px-4 h-8 border-t border-border-subtle flex items-center justify-between text-[10px] text-text-tertiary">
          <span><kbd className="font-mono">↑↓</kbd> 选择 · <kbd className="font-mono">↵</kbd> 打开</span>
          <span>
            {mode === "grep" ? "正则搜索文件内容" :
             mode === "glob" ? "通配符搜索文件名" :
             mode === "messages" ? "搜索历史消息" :
             "搜索对话标题"}
          </span>
          {totalItems > 0 && <span>{totalItems} 项</span>}
        </div>
      </div>
    </Dialog>
  );
}
