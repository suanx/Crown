import type { ReactNode } from "react";
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { Icon } from "@/shared/icons/Icon";
import { cn } from "@/shared/lib/cn";

export type PillTone =
  | "neutral"
  | "brand"
  | "success"
  | "warning"
  | "danger"
  | "info";

export interface PillProps {
  tone?: PillTone;
  icon?: PhIcon;
  size?: "sm" | "md";
  children: ReactNode;
  onClick?: () => void;
  className?: string;
  pulseDot?: boolean;
}

/**
 * Pill 标签.
 * 高度档: sm=24 (h-6) / md=28 (h-7)
 * 横向 padding: sm=8 / md=12
 * Icon: sm=12 / md=14
 */
export function Pill({
  tone = "neutral",
  icon,
  size = "sm",
  children,
  onClick,
  className,
  pulseDot = false,
}: PillProps) {
  const Comp = onClick ? "button" : "span";
  return (
    <Comp
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1 rounded-md transition-colors whitespace-nowrap",
        size === "sm" ? "h-6 px-2 text-xs" : "h-7 px-3 text-sm",
        toneClass(tone),
        onClick && "hover:opacity-90 cursor-pointer focus-ring",
        className,
      )}
    >
      {pulseDot && (
        <span className="h-1.5 w-1.5 rounded-full bg-current animate-pulse-soft" />
      )}
      {icon && <Icon icon={icon} size={size === "sm" ? 12 : 14} />}
      <span className="font-medium">{children}</span>
    </Comp>
  );
}

function toneClass(tone: PillTone): string {
  switch (tone) {
    case "brand":
      return "bg-brand-soft text-brand";
    case "success":
      return "bg-success-soft text-success";
    case "warning":
      return "bg-warning-soft text-warning";
    case "danger":
      return "bg-danger-soft text-danger";
    case "info":
      return "bg-brand-soft text-brand";
    case "neutral":
    default:
      return "bg-elevated text-text-secondary border border-border-subtle";
  }
}
