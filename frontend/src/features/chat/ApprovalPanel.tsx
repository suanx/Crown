import { useEffect, useMemo } from "react";
import { useChatStore } from "@/stores/chatStore";
import type {
  ApprovalRequestEvent,
  ApproveToolDecision,
  PermissionUpdate,
  ToolName,
} from "@/api";
import { Icon } from "@/shared/icons/Icon";
import { WarningIcon, CheckIcon, LockIcon, CloseIcon } from "@/shared/icons/set";
import { toolAction } from "./toolMeta";
import { summarizeToolInput } from "./toolSummary";
import { cn } from "@/shared/lib/cn";

interface ApprovalPanelProps {
  threadId: string;
}

/**
 * 审批面板 —— 紧贴输入框上沿浮出（对齐 Claude Code：审批放在输入框上方，
 * 不是全屏弹窗）。取当前 thread 首项待审批，左侧文案说明要批准什么，右侧
 * 三个紧凑按钮：拒绝 / 始终允许 / 允许。
 *
 * 决策语义（Claude PermissionDecision 形态）：
 *   - 允许一次：{ behavior:"allow", updatedInput=原 input, permissionUpdates:[] }
 *   - 始终允许：同上 + addRules（session 级，后续同名工具不再问）
 *   - 拒绝：    { behavior:"deny", message:null }
 *
 * Esc ≠ 拒绝 —— 走 abortTurn（中止整个回合），与 QuestionPanel 一致。
 */
export function ApprovalPanel({ threadId }: ApprovalPanelProps) {
  const pending = useChatStore((s) => s.pendingApprovals);
  const req = useMemo(
    () => pending.find((a) => a.threadId === threadId),
    [pending, threadId],
  );

  const approveTool = useChatStore((s) => s.approveTool);
  const abortTurn = useChatStore((s) => s.abortTurn);

  // 监听 Esc → 中止回合（不算拒绝）。仅在有待审批时挂监听。
  useEffect(() => {
    if (!req) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void abortTurn(threadId);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [req, threadId, abortTurn]);

  if (!req) return null;

  const decide = (kind: "allow" | "deny" | "always") => {
    void approveTool(threadId, req.toolUseId, buildDecision(kind, req));
  };

  const dangerous =
    req.toolName === "run_command" ||
    req.toolName === "write_file" ||
    req.toolName === "edit_file";
  const obj = summarizeToolInput(req.toolName, req.input);

  return (
    <div
      data-testid="approval-panel"
      className={cn(
        "rounded-2xl border border-warning/30 bg-elevated mb-2",
        "shadow-[0_8px_24px_-4px_rgba(0,0,0,0.35)]",
        "animate-slide-up",
        "flex items-center gap-3 px-4 py-3",
      )}
    >
      <Icon icon={WarningIcon} size={16} weight="fill" className="text-warning shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="text-sm text-text-primary">
          {dangerous ? "需要你批准这个操作" : "等待批准"}
          <span className="ml-2 text-text-secondary font-medium">
            {toolAction(req.toolName)}
          </span>
          {obj && (
            <span className="ml-1.5 text-xs text-text-tertiary font-mono truncate">
              {obj}
            </span>
          )}
        </div>
        <div className="text-xs text-text-tertiary mt-0.5">Esc 取消整个回合</div>
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        <ApprovalBtn icon={CloseIcon} label="拒绝" tone="ghost" onClick={() => decide("deny")} />
        <ApprovalBtn icon={LockIcon} label="始终允许" tone="secondary" onClick={() => decide("always")} />
        <ApprovalBtn icon={CheckIcon} label="允许" tone="primary" onClick={() => decide("allow")} />
      </div>
    </div>
  );
}

function ApprovalBtn({
  icon,
  label,
  tone,
  onClick,
}: {
  icon: typeof CheckIcon;
  label: string;
  tone: "primary" | "secondary" | "ghost";
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "h-7 px-2.5 inline-flex items-center gap-1 rounded-md text-xs font-medium transition-colors focus-ring",
        tone === "primary" && "bg-brand text-white hover:bg-brand-hover",
        tone === "secondary" &&
          "bg-canvas border border-border-subtle text-text-primary hover:bg-hover",
        tone === "ghost" &&
          "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
    >
      <Icon icon={icon} size={11} weight="bold" />
      {label}
    </button>
  );
}

/** 按钮 kind → 协议层 decision 对象（Claude PermissionDecision 形态）。 */
function buildDecision(
  kind: "allow" | "deny" | "always",
  req: ApprovalRequestEvent,
): ApproveToolDecision {
  if (kind === "deny") {
    return { behavior: "deny", message: null };
  }
  const permissionUpdates: PermissionUpdate[] =
    kind === "always"
      ? [
          {
            type: "addRules",
            rules: [{ toolName: req.toolName as ToolName, ruleContent: null }],
            behavior: "allow",
            destination: "session",
          },
        ]
      : [];
  return {
    behavior: "allow",
    updatedInput: req.input,
    permissionUpdates,
  };
}
