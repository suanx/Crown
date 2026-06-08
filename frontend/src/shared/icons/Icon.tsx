/**
 * Icon 包装 — 统一基于 @phosphor-icons/react.
 * 使用 weight="duotone" 风格,有设计感不烂大街.
 *
 * 不直接 export 每个 icon — 通过 ./set.ts 的 NAMED_ICONS 集中映射,
 * 便于换图标库 / 统一 props.
 */
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { cn } from "@/shared/lib/cn";

export interface IconProps {
  /** 来自 set.ts 的 IconName,见同目录 set.ts. */
  icon: PhIcon;
  size?: number;
  /** 1=thin / 2=light / 3=regular / 4=bold / 5=fill / 6=duotone. */
  weight?: "thin" | "light" | "regular" | "bold" | "fill" | "duotone";
  className?: string;
  /** 默认 currentColor — 颜色随父级文字色,符合主题切换. */
}

export function Icon({
  icon: IconComponent,
  size = 16,
  weight = "regular",
  className,
}: IconProps) {
  return (
    <IconComponent
      size={size}
      weight={weight}
      className={cn("shrink-0", className)}
    />
  );
}
