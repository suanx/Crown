import { Icon } from "@/shared/icons/Icon";
import { ReasoningIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

export interface SpinnerProps {
  size?: number;
  className?: string;
}

export function Spinner({ size = 14, className }: SpinnerProps) {
  return (
    <Icon
      icon={ReasoningIcon}
      size={size}
      weight="duotone"
      className={cn("animate-pulse-soft text-brand", className)}
    />
  );
}
