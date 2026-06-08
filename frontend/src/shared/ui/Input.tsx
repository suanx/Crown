import type { InputHTMLAttributes } from "react";
import { cn } from "@/shared/lib/cn";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  fullWidth?: boolean;
}

export function Input({
  className,
  fullWidth = true,
  ...rest
}: InputProps) {
  return (
    <input
      className={cn(
        "h-9 px-3 rounded-md text-sm bg-input-bg text-text-primary",
        "border border-border-default placeholder:text-text-tertiary",
        "outline-none focus:border-border-focus transition-colors",
        fullWidth && "w-full",
        className,
      )}
      {...rest}
    />
  );
}
