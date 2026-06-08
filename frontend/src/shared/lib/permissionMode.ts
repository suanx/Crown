import type { PermissionMode } from "@/api";

/**
 * UI label 映射 — 与协议字符串值解耦.
 *
 * 协议层永远传 PermissionMode 字符串值,UI 显示走这里.
 * i18n 时改这里即可,不动协议.
 */
export const PERMISSION_MODE_LABELS: Record<PermissionMode, string> = {
  default: "Agent",
  plan: "Plan",
  acceptEdits: "Auto-Edit",
  bypassPermissions: "YOLO",
  dontAsk: "Strict",
};

/**
 * 简短描述 (鼠标 hover / dropdown 副标题用).
 */
export const PERMISSION_MODE_DESCRIPTIONS: Record<PermissionMode, string> = {
  default: "默认 — 写工具需审批",
  plan: "计划 — 只读,不修改任何东西",
  acceptEdits: "自动批准文件编辑,其他操作仍需审批",
  bypassPermissions: "YOLO — 跳过审批,deny 规则仍生效",
  dontAsk: "严格 — 所有 ask 自动转 deny",
};

/**
 * 视觉色调 — Pill / button 配色用.
 */
export const PERMISSION_MODE_TONE: Record<
  PermissionMode,
  "neutral" | "brand" | "danger" | "warning"
> = {
  default: "brand",
  plan: "neutral",
  acceptEdits: "brand",
  bypassPermissions: "danger",
  dontAsk: "neutral",
};

/**
 * ComposerModeSelector 暴露给用户的四档.
 * 后端 cyclePermissionMode 按此顺序循环.
 */
export const MODE_SWITCHER_VALUES: PermissionMode[] = [
  "default",
  "acceptEdits",
  "plan",
  "bypassPermissions",
];
