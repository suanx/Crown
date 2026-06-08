import { useEffect, type ReactNode } from "react";
import { cn } from "@/shared/lib/cn";

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  /** 阻止点击遮罩关闭 (用于必须做出决定的对话框). */
  modal?: boolean;
  className?: string;
  children: ReactNode;
}

export function Dialog({
  open,
  onClose,
  modal = false,
  className,
  children,
}: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !modal) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, modal, onClose]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-6 animate-fade-in"
      style={{ backgroundColor: "rgba(0, 0, 0, 0.45)" }}
      onClick={() => !modal && onClose()}
    >
      <div
        className={cn(
          "relative bg-overlay rounded-xl shadow-lg max-w-2xl w-full max-h-[80vh] overflow-hidden",
          "border border-border-default animate-scale-in",
          className,
        )}
        style={{ boxShadow: "var(--ds-shadow-lg)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
