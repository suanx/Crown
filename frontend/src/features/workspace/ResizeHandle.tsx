import { useRef } from "react";
import { cn } from "@/shared/lib/cn";

export type ResizeAxis = "x" | "y";
export type ResizeSide = "left" | "right" | "top" | "bottom";

export interface ResizeHandleProps {
  /** 拖动方向 */
  axis: ResizeAxis;
  /** 哪一边显示 (定位用) */
  side: ResizeSide;
  /** 当前像素值 (起始基准) */
  current: number;
  /** 拖动结束时调用,传新像素 */
  onResize: (px: number) => void;
  /** 拖动方向反向 (右面板向左拖增宽 → reverse=true) */
  reverse?: boolean;
}

/**
 * 通用拖拽改宽/改高手柄.
 *
 * 实现要点:
 *   - 4px 命中区,但视觉只显示 1px 边线 (更易点中,符合 VS Code/Linear)
 *   - 拖动时整页加 cursor + select-none,避免拖到副作用
 *   - 不在拖动过程中频繁 set state — 内部 ref 累积,mouseup 时统一 onResize
 *   - 支持 axis x/y,reverse 用于"右面板向左拖增宽"
 */
export function ResizeHandle({
  axis,
  side,
  current,
  onResize,
  reverse = false,
}: ResizeHandleProps) {
  const startRef = useRef({ x: 0, y: 0, base: 0 });

  function onMouseDown(e: React.MouseEvent) {
    e.preventDefault();
    startRef.current = { x: e.clientX, y: e.clientY, base: current };

    const isX = axis === "x";
    const cursor = isX ? "col-resize" : "row-resize";
    document.body.style.cursor = cursor;
    document.body.style.userSelect = "none";

    let last = current;

    function onMove(ev: MouseEvent) {
      const delta = isX
        ? ev.clientX - startRef.current.x
        : ev.clientY - startRef.current.y;
      const sign = reverse ? -1 : 1;
      last = Math.round(startRef.current.base + delta * sign);
      onResize(last);
    }

    function onUp() {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    }

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  return (
    <div
      onMouseDown={onMouseDown}
      onDoubleClick={() => onResize(0)} /* 双击重置 — 由调用方决定语义 */
      role="separator"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      className={cn(
        "absolute z-10 group transition-colors",
        // 4px 命中区,负 margin 让它跨在边线上更易点中
        axis === "x"
          ? "top-0 bottom-0 w-1 cursor-col-resize"
          : "left-0 right-0 h-1 cursor-row-resize",
        side === "left" && "left-0 -ml-0.5",
        side === "right" && "right-0 -mr-0.5",
        side === "top" && "top-0 -mt-0.5",
        side === "bottom" && "bottom-0 -mb-0.5",
      )}
    >
      {/* 视觉指示 — hover/active 时高亮 */}
      <div
        className={cn(
          "absolute inset-0 transition-opacity",
          "opacity-0 group-hover:opacity-100 group-active:opacity-100",
          "bg-brand",
        )}
      />
    </div>
  );
}
