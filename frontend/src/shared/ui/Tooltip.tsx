import { useState, type ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

export interface TooltipProps {
  label: string;
  side?: "top" | "bottom" | "left" | "right";
  children: ReactNode;
  className?: string;
}

/**
 * 极简 tooltip — pure CSS hover 显示,不引 popper.
 * 适用于按钮简短说明,不适合富内容.富内容用 Popover.
 */
export function Tooltip({
  label,
  side = "top",
  children,
  className,
}: TooltipProps) {
  const [hover, setHover] = useState(false);
  return (
    <span
      className={cn("relative inline-flex", className)}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {children}
      {hover && (
        <span
          role="tooltip"
          className={cn(
            "absolute z-50 px-2 py-1 text-xs rounded-md whitespace-nowrap pointer-events-none",
            "bg-overlay text-text-primary border border-border-default shadow-md",
            "animate-fade-in",
            sideClass(side),
          )}
          style={{
            boxShadow: "var(--ds-shadow-md)",
          }}
        >
          {label}
        </span>
      )}
    </span>
  );
}

function sideClass(side: TooltipProps["side"]): string {
  switch (side) {
    case "bottom":
      return "left-1/2 -translate-x-1/2 top-full mt-1.5";
    case "left":
      return "right-full mr-1.5 top-1/2 -translate-y-1/2";
    case "right":
      return "left-full ml-1.5 top-1/2 -translate-y-1/2";
    case "top":
    default:
      return "left-1/2 -translate-x-1/2 bottom-full mb-1.5";
  }
}
