import { useEffect, useRef, useState, memo } from "react";
import type { ToolSegment } from "@/api";
import { Icon } from "@/shared/icons/Icon";
import { CaretRightIcon, CheckCircleIcon, GlobeIcon, ToolIcon } from "@/shared/icons/set";
import { ShimmerText } from "@/shared/ui/ShimmerText";
import { cn } from "@/shared/lib/cn";
import { ToolRow } from "./ToolRow";

/**
 * 工具组 —— 把一段连续的工具调用收成一个折叠单元。
 *
 * memo 避免父组件重渲染导致工具组展开状态丢失或重复计算 allDone。
 */
export const ToolGroup = memo(function ToolGroup({ tools }: { tools: ToolSegment[] }) {
  const allDone = tools.every(
    (t) => t.status !== "running" && t.status !== "pending_approval",
  );
  const hasError = tools.some((t) => t.status === "error");

  // 网搜专属文案：整组都是 web_search 时显示「搜索网络」。
  const allWebSearch = tools.every((t) => t.name === "web_search");

  const [open, setOpen] = useState(true);
  const userTouched = useRef(false);
  const wasDone = useRef(allDone);

  // 完成边沿（running→done）：若用户没手动操作过，保持展开（不自动收，
  // 让用户看到结果）；用户手动开/关后尊重其选择。
  useEffect(() => {
    if (!wasDone.current && allDone && !userTouched.current) {
      // 完成时不强制折叠，仅记录边沿。要自动收起可在此 setOpen(false)。
    }
    wasDone.current = allDone;
  }, [allDone]);

  function toggle() {
    userTouched.current = true;
    setOpen((v) => !v);
  }

  const headerLabel = allWebSearch
    ? "搜索网络"
    : `使用了 ${tools.length} 个工具`;

  const HeaderIcon = allWebSearch ? GlobeIcon : ToolIcon;

  return (
    <div
      className={cn(
        "rounded-lg transition-colors",
        !allDone && "bg-black/[0.03] dark:bg-white/[0.04]",
      )}
    >
      <button
        onClick={toggle}
        className="group/group w-full flex items-center gap-2 py-1 -mx-1.5 px-1.5 text-left rounded-md hover:bg-hover active:scale-[0.99] transition-all focus-ring"
      >
        <Icon
          icon={allDone && !hasError ? CheckCircleIcon : HeaderIcon}
          size={14}
          weight="duotone"
          className={cn(
            "shrink-0",
            hasError ? "text-danger" : allDone ? "text-text-tertiary" : "text-brand",
          )}
        />
        {allDone ? (
          <span className="text-sm text-text-secondary">{headerLabel}</span>
        ) : (
          <ShimmerText
            baseColor="rgba(255,255,255,0.55)"
            highlightColor="#ffffff"
            className="text-sm"
          >
            {headerLabel}
          </ShimmerText>
        )}
        <Icon
          icon={CaretRightIcon}
          size={12}
          className={cn(
            "ml-auto shrink-0 opacity-50 transition-transform duration-200",
            open && "rotate-90",
          )}
        />
      </button>

      {open && (
        <div className="pl-[6px] py-1">
          {/* 左竖线 + 缩进体。contain 限制 layout 范围，减少重绘 */}
          <div className="tool-rail pl-5 space-y-0.5" style={{ contain: 'layout paint' }}>
            {tools.map((seg) => (
              <ToolRow key={seg.callId} seg={seg} />
            ))}
            {allDone && (
              <div className="flex items-center gap-1.5 pt-1 text-text-tertiary">
                <Icon
                  icon={CheckCircleIcon}
                  size={13}
                  weight="fill"
                  className={hasError ? "text-danger" : "text-success"}
                />
                <span className="text-xs">{hasError ? "完成（有错误）" : "完成"}</span>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
});
