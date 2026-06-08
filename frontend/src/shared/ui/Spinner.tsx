import { Icon } from "@/shared/icons/Icon";
import { SpinnerIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

export interface SpinnerProps {
  size?: number;
  /** Tailwind 颜色类，默认品牌蓝。stalled 时调用方传琥珀色类。 */
  className?: string;
}

/**
 * 转圈加载指示器。
 *
 * 性能优化：使用 CSS animation（GPU 合成层），无 JS 定时器驱动，
 * 不会触发 React 重渲染。
 */
export function Spinner({ size = 14, className }: SpinnerProps) {
  return (
    <Icon
      icon={SpinnerIcon}
      size={size}
      weight="bold"
      className={cn("ds-spin text-brand", className)}
    />
  );
}
