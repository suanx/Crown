import { useEffect, useRef, useState } from "react";
import type { PermissionMode } from "@/api";
import { agentClient } from "@/api";
import { useUiStore } from "@/stores/uiStore";
import { useActiveThread, useChatStore } from "@/stores/chatStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CaretDownIcon,
  CaretUpIcon,
  ShieldIcon,
  AgentIcon,
  WarningIcon,
  CheckIcon,
} from "@/shared/icons/set";
import {
  MODE_SWITCHER_VALUES,
  PERMISSION_MODE_DESCRIPTIONS,
  PERMISSION_MODE_LABELS,
  PERMISSION_MODE_TONE,
} from "@/shared/lib/permissionMode";
import { cn } from "@/shared/lib/cn";
import type { Icon as PhIcon } from "@phosphor-icons/react";

/**
 * 嵌入 ComposeBar 底部的权限模式选择器.
 *
 * 切换语义:per-thread 状态. 调 cyclePermissionMode(threadId) 循环到下一模式,
 * 不调 setConfig (后者改的是新建 thread 的全局默认).
 *
 * 暴露 default / acceptEdits / plan / bypassPermissions 四档.
 * Dropdown 显示所有选项 + active check mark,点击任何选项触发 cycle.
 */

const MODE_ICON: Record<PermissionMode, PhIcon> = {
  default: AgentIcon,
  plan: ShieldIcon,
  acceptEdits: AgentIcon,
  bypassPermissions: WarningIcon,
  dontAsk: ShieldIcon,
};

export function ComposerModeSelector() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const current = useUiStore((s) => s.permissionMode);
  const setCurrent = useUiStore((s) => s.setPermissionMode);
  const thread = useActiveThread();

  // 用 thread.permissionMode 做真实展示,uiStore 仅作 fallback
  const active = thread?.permissionMode ?? current;
  const tone = PERMISSION_MODE_TONE[active];
  const ActiveIcon = MODE_ICON[active];

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  async function handlePick(pickedMode: PermissionMode) {
    setOpen(false);
    const nextMode = pickedMode;
    setCurrent(nextMode);

    // WelcomePage 阶段 thread 为 null — 只更新 uiStore 作为默认模式
    // createThread 后 WelcomePage.handleFirstSend 会把这个模式传给新 thread
    if (!thread) return;

    // ChatPage 阶段 — 直接更新当前 thread
    useChatStore.setState((s) => {
      const t = s.threadsById[thread.id];
      if (!t) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [thread.id]: { ...t, permissionMode: nextMode },
        },
      };
    });
    try {
      await agentClient.updateThread({
        threadId: thread.id,
        permissionMode: nextMode,
      });
    } catch {
      // HybridClient fallback
    }
  }

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "h-7 px-2 inline-flex items-center gap-1 rounded-md text-xs transition-colors focus-ring",
          tone === "brand" &&
            (open
              ? "bg-brand-soft text-brand"
              : "text-text-secondary hover:bg-hover hover:text-text-primary"),
          tone === "danger" && "bg-danger-soft text-danger hover:opacity-90",
          tone === "warning" && "bg-warning-soft text-warning hover:opacity-90",
          tone === "neutral" &&
            "text-text-secondary hover:bg-hover hover:text-text-primary",
        )}
      >
        <Icon
          icon={ActiveIcon}
          size={12}
          weight="bold"
          className={cn(
            tone === "brand" && "text-brand",
            tone === "danger" && "text-danger",
            tone === "warning" && "text-warning",
          )}
        />
        <span className="font-medium">{PERMISSION_MODE_LABELS[active]}</span>
        <Icon
          icon={open ? CaretUpIcon : CaretDownIcon}
          size={10}
          className="opacity-60"
        />
      </button>

      {open && (
        <div
          className="absolute bottom-full right-0 mb-1 w-64 py-1 bg-overlay border border-border-default rounded-md z-30 animate-scale-in"
          style={{ boxShadow: "var(--ds-shadow-md)" }}
        >
          {MODE_SWITCHER_VALUES.map((mode) => {
            const ModeIcon = MODE_ICON[mode];
            const t = PERMISSION_MODE_TONE[mode];
            return (
              <button
                key={mode}
                onClick={() => handlePick(mode)}
                className="w-full px-3 py-2 flex items-start gap-3 hover:bg-hover transition-colors text-left"
              >
                <Icon
                  icon={ModeIcon}
                  size={14}
                  weight="bold"
                  className={cn(
                    "mt-0.5",
                    t === "brand" && "text-brand",
                    t === "danger" && "text-danger",
                    t === "warning" && "text-warning",
                    t === "neutral" && "text-text-secondary",
                  )}
                />
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-text-primary font-medium">
                    {PERMISSION_MODE_LABELS[mode]}
                  </div>
                  <div className="text-xs text-text-tertiary leading-snug">
                    {PERMISSION_MODE_DESCRIPTIONS[mode]}
                  </div>
                </div>
                {active === mode && (
                  <Icon
                    icon={CheckIcon}
                    size={14}
                    className="text-brand mt-0.5"
                  />
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
