import { useWorkspaceStore, type PanelKind } from "@/stores/workspaceStore";
import { useActiveThreadTodos } from "@/stores/chatStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CloseIcon,
  SwapToRightIcon,
  FolderIcon,
  GlobeIcon,
  DiffIcon,
  TerminalIcon,
  TasksIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import type { Icon as PhIcon } from "@phosphor-icons/react";

const META: Record<PanelKind, { label: string; icon: PhIcon }> = {
  files: { label: "文件", icon: FolderIcon },
  tasks: { label: "任务", icon: TasksIcon },
  browser: { label: "浏览器", icon: GlobeIcon },
  review: { label: "代码审查", icon: DiffIcon },
  terminal: { label: "终端", icon: TerminalIcon },
};

const TAB_KINDS: PanelKind[] = ["files", "tasks"];

export interface PanelHeaderProps {
  slot: "right" | "bottom";
  kind: PanelKind;
  extra?: React.ReactNode;
}

export function PanelHeader({ slot, kind, extra }: PanelHeaderProps) {
  const close = useWorkspaceStore((s) =>
    slot === "right" ? s.closeRight : s.closeBottom,
  );
  const swap = useWorkspaceStore((s) =>
    slot === "right" ? s.swapToBottom : s.swapToRight,
  );
  const openInRight = useWorkspaceStore((s) => s.openInRight);
  const openInBottom = useWorkspaceStore((s) => s.openInBottom);
  const open = slot === "right" ? openInRight : openInBottom;

  const isTabbed = TAB_KINDS.includes(kind);

  return (
    <div className="h-9 px-3 flex items-center gap-2 border-b border-border-subtle shrink-0">
      {isTabbed ? (
        <div className="flex items-center gap-0.5">
          {TAB_KINDS.map((k) => (
            <PanelTab
              key={k}
              active={k === kind}
              icon={META[k].icon}
              label={META[k].label}
              badge={k === "tasks" ? <TaskBadge /> : undefined}
              onClick={() => open(k)}
            />
          ))}
        </div>
      ) : (
        <>
          <Icon icon={META[kind].icon} size={14} weight="duotone" className="text-text-secondary" />
          <span className="text-sm font-medium text-text-primary">{META[kind].label}</span>
        </>
      )}

      <div className="flex-1" />
      {extra}

      <button
        onClick={() => swap(kind)}
        title={slot === "right" ? "移到底部" : "移到右侧"}
        aria-label="切换槽位"
        className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
      >
        <Icon icon={SwapToRightIcon} size={13} />
      </button>

      <button
        onClick={close}
        title="关闭"
        aria-label="关闭面板"
        className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
      >
        <Icon icon={CloseIcon} size={14} />
      </button>
    </div>
  );
}

function PanelTab({
  active,
  icon,
  label,
  badge,
  onClick,
}: {
  active: boolean;
  icon: PhIcon;
  label: string;
  badge?: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "h-7 px-2 inline-flex items-center gap-1.5 rounded-md text-xs transition-colors focus-ring",
        active
          ? "bg-elevated text-text-primary font-medium"
          : "text-text-tertiary hover:text-text-secondary hover:bg-hover",
      )}
    >
      <Icon icon={icon} size={13} weight={active ? "duotone" : "regular"} />
      <span>{label}</span>
      {badge}
    </button>
  );
}

function TaskBadge() {
  const todos = useActiveThreadTodos();
  if (todos.length === 0) return null;
  const done = todos.filter((t) => t.status === "completed").length;
  const allDone = done === todos.length;
  return (
    <span
      className={cn(
        "text-[10px] font-mono tabular-nums ml-0.5",
        allDone ? "text-success" : "text-text-tertiary",
      )}
    >
      {allDone ? "✓" : `${done}/${todos.length}`}
    </span>
  );
}
