import { useEffect, useRef } from "react";
import type { SubAgentActivity } from "@/api";
import { Icon } from "@/shared/icons/Icon";
import { AgentIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import { toolAction, toolIcon } from "./toolMeta";
import { summarizeToolInput } from "./toolSummary";

/**
 * 子代理嵌套面板 (P4) — 在 task 工具行展开体内显示子代理的实时活动。
 *
 * 固定尺寸"工作区"：头部常驻，主体固定高度 (240px) 内部滚动，流式内容
 * 增长时自动贴底跟随。避免内容无限往下撑把主对话挤乱，并营造"子代理在
 * 里面刷刷干"的观感。
 *
 * 从原 ToolCallCard 抽出独立文件，供去卡片化后的 ToolRow 复用。
 */
export function SubAgentPanel({ activity }: { activity: SubAgentActivity }) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const toolCount = activity.toolCalls.length;
  const textLen = activity.text.length;
  // 内容长度变化时，若用户停在底部附近则自动滚到底 (贴底跟随流式)。
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distanceFromBottom < 80) {
      el.scrollTop = el.scrollHeight;
    }
  }, [toolCount, textLen]);

  return (
    <div
      data-testid="subagent-panel"
      className="rounded-md border border-border-subtle bg-canvas/50 overflow-hidden"
    >
      <div className="flex items-center gap-1.5 text-xs text-text-tertiary px-2 py-1.5 border-b border-border-subtle bg-elevated/40">
        <Icon icon={AgentIcon} size={12} weight="duotone" />
        <span>子代理活动</span>
        <span className="font-mono opacity-60">
          {activity.agentId.slice(0, 8)}
        </span>
      </div>
      <div
        ref={bodyRef}
        className="scrollable p-2 space-y-1.5"
        style={{ height: 240 }}
      >
        {toolCount > 0 && (
          <div className="space-y-1">
            {activity.toolCalls.map((tc) => (
              <div key={tc.id} className="flex items-center gap-1.5 text-xs pl-1">
                <Icon
                  icon={toolIcon(tc.name)}
                  size={11}
                  className={cn(
                    "shrink-0",
                    tc.status === "success" && "text-success",
                    tc.status === "error" && "text-danger",
                    tc.status === "running" && "text-brand",
                    tc.status === "aborted" && "text-text-tertiary",
                  )}
                />
                <span className="text-text-secondary shrink-0">
                  {toolAction(tc.name)}
                </span>
                <span className="text-text-tertiary font-mono truncate">
                  {summarizeToolInput(tc.name, tc.input)}
                </span>
              </div>
            ))}
          </div>
        )}
        {activity.text && (
          <div className="text-xs text-text-secondary whitespace-pre-wrap leading-relaxed border-t border-border-subtle pt-1.5">
            {activity.text}
          </div>
        )}
        {toolCount === 0 && !activity.text && (
          <div className="text-xs text-text-tertiary">子代理启动中…</div>
        )}
      </div>
    </div>
  );
}
