import { useEffect, useRef, useState } from "react";
import { useActiveThreadTodos } from "@/stores/chatStore";
import { PanelHeader } from "../PanelHeader";
import { Icon } from "@/shared/icons/Icon";
import { CheckCircleIcon, CircleIcon, SpinnerIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import type { TodoItem } from "@/api";

export interface TasksViewProps {
  slot: "right" | "bottom";
}

export function TasksView({ slot }: TasksViewProps) {
  const todos = useActiveThreadTodos();
  const allDone =
    todos.length > 0 && todos.every((t) => t.status === "completed");

  // 完成折叠逻辑 (§5 H+ B 方案)
  const [collapsed, setCollapsed] = useState(false);
  const [userExpanded, setUserExpanded] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 列表内容变化 → 重置本地折叠状态
  const listKey = todos.map((t) => t.content).join("|");
  useEffect(() => {
    setCollapsed(false);
    setUserExpanded(false);
  }, [listKey]);

  // 全部完成后延迟 1.5s 自动折叠(除非用户已手动展开)
  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (allDone && !userExpanded) {
      timerRef.current = setTimeout(() => setCollapsed(true), 1500);
    } else {
      setCollapsed(false);
    }
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [allDone, userExpanded]);

  return (
    <div className="h-full flex flex-col">
      <PanelHeader slot={slot} kind="tasks" />
      <div className="flex-1 min-h-0 scrollable px-2 py-2">
        {todos.length === 0 ? (
          <EmptyTasks />
        ) : allDone && collapsed ? (
          <DoneSummary
            count={todos.length}
            onExpand={() => {
              setCollapsed(false);
              setUserExpanded(true);
            }}
          />
        ) : (
          <ul className="space-y-0.5">
            {todos.map((t, i) => (
              <TaskRow key={i} todo={t} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function TaskRow({ todo }: { todo: TodoItem }) {
  const isCompleted = todo.status === "completed";
  const isProgress = todo.status === "in_progress";

  return (
    <div className="flex items-start gap-2 px-2 py-1.5 rounded-md">
      {isCompleted ? (
        <Icon
          icon={CheckCircleIcon}
          size={15}
          weight="fill"
          className="text-success shrink-0 mt-0.5"
        />
      ) : isProgress ? (
        <Icon
          icon={SpinnerIcon}
          size={15}
          className="text-brand shrink-0 mt-0.5 animate-spin"
        />
      ) : (
        <Icon
          icon={CircleIcon}
          size={15}
          className="text-text-tertiary shrink-0 mt-0.5"
        />
      )}
      <span
        className={cn(
          "text-sm leading-[1.5]",
          isCompleted && "line-through text-text-tertiary",
          isProgress && "text-text-primary font-medium",
          !isCompleted && !isProgress && "text-text-secondary",
        )}
      >
        {isProgress ? todo.activeForm : todo.content}
      </span>
    </div>
  );
}

function DoneSummary({
  count,
  onExpand,
}: {
  count: number;
  onExpand: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-3 py-3 rounded-md bg-elevated border border-border-subtle">
      <Icon
        icon={CheckCircleIcon}
        size={16}
        weight="fill"
        className="text-success shrink-0"
      />
      <span className="text-sm text-text-primary font-medium flex-1">
        全部完成 ({count}/{count})
      </span>
      <button
        onClick={onExpand}
        className="text-xs text-brand hover:underline focus-ring rounded px-1"
      >
        展开
      </button>
    </div>
  );
}

function EmptyTasks() {
  return (
    <div className="h-full flex flex-col items-center justify-center text-center px-6">
      <p className="text-sm text-text-tertiary">暂无任务</p>
      <p className="text-xs text-text-tertiary mt-1">
        Agent 规划多步任务时会显示在这里
      </p>
    </div>
  );
}
