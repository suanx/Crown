import type { ButtonHTMLAttributes } from "react";
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { Icon } from "@/shared/icons/Icon";
import { cn } from "@/shared/lib/cn";

export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: PhIcon;
  label: string;
  size?: "sm" | "md";
  variant?: "ghost" | "filled" | "subtle";
  active?: boolean;
}

/**
 * 标准 icon 按钮.
 * 高度档:
 *   - sm: 28 (h-7),配 12px 图标
 *   - md: 32 (h-8),配 14px 图标
 */
export function IconButton({
  icon,
  label,
  size = "md",
  variant = "ghost",
  active = false,
  className,
  ...rest
}: IconButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex items-center justify-center rounded-md transition-colors focus-ring no-drag",
        size === "sm" ? "h-7 w-7" : "h-8 w-8",
        variant === "ghost" &&
          "text-text-secondary hover:bg-hover hover:text-text-primary",
        variant === "subtle" &&
          "bg-elevated text-text-secondary hover:bg-hover hover:text-text-primary",
        variant === "filled" && "bg-brand text-white hover:bg-brand-hover",
        active && variant === "ghost" && "bg-hover text-text-primary",
        rest.disabled && "opacity-40 cursor-not-allowed",
        className,
      )}
      {...rest}
    >
      <Icon icon={icon} size={size === "sm" ? 12 : 14} weight="regular" />
    </button>
  );
}
