import type { ThreadSummary } from "@/api";
import { agentClient } from "@/api";
import { useSessionStore } from "@/stores/sessionStore";
import { useRouterStore } from "@/stores/routerStore";
import { useChatStore } from "@/stores/chatStore";
import {
  EditIcon,
  PinIcon,
  StarIcon,
  TrashIcon,
  MoreVerticalIcon,
  DownloadIcon,
} from "@/shared/icons/set";
import { Icon } from "@/shared/icons/Icon";
import { Spinner } from "@/shared/ui/Spinner";
import { cn } from "@/shared/lib/cn";
import { displayThreadTitle } from "@/shared/lib/threadTitle";

interface SessionItemProps {
  thread: ThreadSummary;
  active: boolean;
  menuOpen: boolean;
  onClick: () => void;
  onToggleMenu: () => void;
  onCloseMenu: () => void;
}

/**
 * 单条会话 — 高 32 (h-8),与 Sidebar 导航项对齐.
 * 横向 padding 8,内部 gap 8.
 */
export function SessionItem({
  thread,
  active,
  menuOpen,
  onClick,
  onToggleMenu,
  onCloseMenu,
}: SessionItemProps) {
  return (
    <div
      data-testid="session-item"
      className={cn(
        "group relative flex items-center h-8 rounded-md transition-colors",
        active ? "bg-hover" : "hover:bg-hover",
      )}
    >
      <button
        onClick={onClick}
        data-testid="session-item-open"
        className="flex-1 min-w-0 flex items-center gap-2 px-2 h-full focus-ring rounded-md text-left"
      >
        {thread.isStreaming && (
          <Spinner size={11} className="shrink-0" />
        )}
        {!thread.isStreaming && thread.isPinned && (
          <Icon
            icon={PinIcon}
            size={12}
            weight="fill"
            className="text-text-tertiary shrink-0"
          />
        )}
        <span
          className={cn(
            "text-sm truncate",
            active
              ? "text-text-primary"
              : "text-text-secondary group-hover:text-text-primary",
          )}
        >
          {displayThreadTitle(thread.title, thread.preview)}
        </span>
      </button>

      <button
        onClick={onToggleMenu}
        aria-label="更多操作"
        className={cn(
          "h-7 w-7 mr-1 flex items-center justify-center rounded-md focus-ring",
          "opacity-0 group-hover:opacity-100 hover:bg-overlay text-text-tertiary",
          menuOpen && "opacity-100 bg-overlay",
        )}
      >
        <Icon icon={MoreVerticalIcon} size={14} />
      </button>

      {menuOpen && (
        <>
          <div
            className="fixed inset-0 z-30"
            onClick={onCloseMenu}
            aria-hidden
          />
          <div
            className="absolute right-1 top-8 z-40 min-w-[140px] py-1 bg-overlay border border-border-default rounded-md animate-scale-in"
            style={{ boxShadow: "var(--ds-shadow-md)" }}
          >
            <MenuItem icon={StarIcon} label="收藏" onClick={() => {
              onCloseMenu();
              void agentClient.updateThread({ threadId: thread.id, isPinned: !thread.isPinned }).then(() => useSessionStore.getState().loadThreads());
            }} />
            <MenuItem icon={PinIcon} label="置顶" onClick={() => {
              onCloseMenu();
              void agentClient.updateThread({ threadId: thread.id, isPinned: !thread.isPinned }).then(() => useSessionStore.getState().loadThreads());
            }} />
            <MenuItem icon={EditIcon} label="重命名" onClick={() => {
              onCloseMenu();
              const current = displayThreadTitle(thread.title, thread.preview);
              const title = window.prompt("重命名对话", current);
              if (title && title !== thread.title) {
                void agentClient.updateThread({ threadId: thread.id, title }).then(() => useSessionStore.getState().loadThreads());
              }
            }} />
            <MenuItem icon={DownloadIcon} label="导出" onClick={async () => {
              onCloseMenu();
              const md = await agentClient.exportThread(thread.id);
              const blob = new Blob([md], { type: "text/markdown" });
              const url = URL.createObjectURL(blob);
              const a = document.createElement("a");
              a.href = url;
              a.download = `thread-${thread.id.slice(0, 8)}.md`;
              a.click();
              URL.revokeObjectURL(url);
            }} />

            <div className="my-1 h-px bg-border-subtle" />
            <MenuItem
              icon={TrashIcon}
              label="删除"
              tone="danger"
              onClick={() => {
                onCloseMenu();
                void agentClient.deleteThread(thread.id).then(() => {
                  // 清掉 chatStore 里该 thread 的所有缓存,防止内存泄漏
                  useChatStore.getState().dropThread(thread.id);
                  useSessionStore.getState().loadThreads();
                  // 如果删的是当前正在看的 thread,跳回 welcome
                  const route = useRouterStore.getState().current;
                  if (route.page === "chat" && route.threadId === thread.id) {
                    useRouterStore.getState().navigate({ page: "welcome" });
                  }
                });
              }}
            />
          </div>
        </>
      )}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  tone = "default",
  onClick,
}: {
  icon: typeof EditIcon;
  label: string;
  tone?: "default" | "danger";
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full flex items-center gap-2 px-3 h-8 text-sm hover:bg-hover transition-colors text-left",
        tone === "danger"
          ? "text-danger"
          : "text-text-secondary hover:text-text-primary",
      )}
    >
      <Icon icon={icon} size={14} />
      {label}
    </button>
  );
}
