import type { ReactNode, CSSProperties } from "react";
import { cn } from "@/shared/lib/cn";

export interface ShimmerTextProps {
  children: ReactNode;
  /** 基色（CSS 颜色值）。进行时一般用品牌/成功色，stalled 用琥珀色。 */
  baseColor?: string;
  /** 高光色（扫过时的亮色）。默认比基色亮。 */
  highlightColor?: string;
  className?: string;
}

/**
 * 文字扫光组件 —— 一道高光从左到右扫过文案，营造"正在刷新/处理中"的活感。
 *
 * 机制对齐参考实现：`linear-gradient` + `background-clip:text` + 动态
 * `background-position`（见 styles/index.css 的 `.shimmer-text` 与 tailwind
 * 的 `shimmer` keyframes）。颜色通过 CSS 变量注入，调用方切换绿色/琥珀色。
 * 尊重 `prefers-reduced-motion`（CSS 媒体查询里退化为静态色）。
 */
export function ShimmerText({
  children,
  baseColor,
  highlightColor,
  className,
}: ShimmerTextProps) {
  const style: CSSProperties & Record<string, string> = {} as never;
  if (baseColor) style["--shimmer-base"] = baseColor;
  if (highlightColor) style["--shimmer-hi"] = highlightColor;
  return (
    <span className={cn("shimmer-text", className)} style={style}>
      {children}
    </span>
  );
}
