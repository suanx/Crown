import type { ButtonHTMLAttributes, ReactNode } from "react";
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { Icon } from "@/shared/icons/Icon";
import { cn } from "@/shared/lib/cn";

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  icon?: PhIcon;
  iconRight?: PhIcon;
  fullWidth?: boolean;
  children?: ReactNode;
}

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  iconRight,
  fullWidth = false,
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type="button"
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-md font-medium transition-colors focus-ring no-drag whitespace-nowrap",
        size === "sm" && "h-7 px-3 text-xs",
        size === "md" && "h-9 px-4 text-sm",
        size === "lg" && "h-11 px-5 text-base",
        variant === "primary" &&
          "bg-brand text-white hover:bg-brand-hover active:bg-brand-active",
        variant === "secondary" &&
          "bg-elevated text-text-primary hover:bg-hover border border-border-subtle",
        variant === "ghost" &&
          "text-text-secondary hover:bg-hover hover:text-text-primary",
        variant === "danger" &&
          "bg-danger text-white hover:opacity-90",
        fullWidth && "w-full",
        rest.disabled && "opacity-50 cursor-not-allowed",
        className,
      )}
      {...rest}
    >
      {icon && <Icon icon={icon} size={size === "sm" ? 13 : 15} />}
      {children}
      {iconRight && <Icon icon={iconRight} size={size === "sm" ? 13 : 15} />}
    </button>
  );
}
