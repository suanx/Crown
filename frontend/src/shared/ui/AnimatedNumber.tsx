import { useEffect, useState } from "react";
import { cn } from "@/shared/lib/cn";

export function AnimatedNumber({ value, className }: { value: number; className?: string }) {
  const [displayValue, setDisplayValue] = useState(value);

  useEffect(() => {
    if (value === displayValue) return;

    const startValue = displayValue;
    const endValue = value;
    const duration = 180; // ms
    const startTime = performance.now();

    let frameId: number;

    const animate = (currentTime: number) => {
      const elapsed = currentTime - startTime;
      const progress = Math.min(elapsed / duration, 1);
      
      // 用 easeOutExpo 做快速但顺滑的收尾。
      const ease = progress === 1 ? 1 : 1 - Math.pow(2, -10 * progress);
      
      const current = Math.round(startValue + (endValue - startValue) * ease);
      setDisplayValue(current);

      if (progress < 1) {
        frameId = requestAnimationFrame(animate);
      }
    };

    frameId = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frameId);
  }, [value]);

  return <span className={cn("inline-block tabular-nums", className)}>{displayValue}</span>;
}
