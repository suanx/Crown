/**
 * ============================================================================
 * Devtools Recorder — IPC 调用记录中枢
 * ============================================================================
 *
 * HybridClient 每次调用走它,DevtoolsPanel 从它读统计.
 *
 * 只在开发模式下生效;prod 所有 record* 函数 noop.
 * ----------------------------------------------------------------------------
 */

import type { ShapeMismatch } from "./assertShape";
import type { EndpointKey } from "./AgentClient";

export type CallSource =
  | "mock" // 走的是 mock client
  | "real-ok" // 真实后端调用成功
  | "real-failed-fallback" // 真实后端失败,已 fallback mock
  | "real-shape-mismatch"; // 真实后端返回但形状不匹配

export interface CallRecord {
  endpoint: EndpointKey;
  source: CallSource;
  timestamp: number;
  durationMs: number | null;
  errorMessage: string | null;
}

interface DevtoolsState {
  calls: CallRecord[];
  shapeMismatches: ShapeMismatch[];
  // 派生的快速索引
  callCountByEndpoint: Record<string, number>;
  lastCallByEndpoint: Record<string, CallRecord>;
}

const MAX_CALLS = 500;

const state: DevtoolsState = {
  calls: [],
  shapeMismatches: [],
  callCountByEndpoint: {},
  lastCallByEndpoint: {},
};

// 简单订阅机制 — DevtoolsPanel 通过 useSyncExternalStore 拉
type Listener = () => void;
const listeners = new Set<Listener>();

function notify() {
  for (const l of listeners) l();
}

function recordCall(rec: CallRecord) {
  if (!import.meta.env.DEV) return;
  state.calls.push(rec);
  if (state.calls.length > MAX_CALLS) {
    state.calls.splice(0, state.calls.length - MAX_CALLS);
  }
  state.callCountByEndpoint[rec.endpoint] =
    (state.callCountByEndpoint[rec.endpoint] ?? 0) + 1;
  state.lastCallByEndpoint[rec.endpoint] = rec;
  notify();
}

function recordShapeMismatch(endpoint: string, mismatches: ShapeMismatch[]) {
  if (!import.meta.env.DEV) return;
  state.shapeMismatches.push(...mismatches);
  if (state.shapeMismatches.length > MAX_CALLS) {
    state.shapeMismatches.splice(
      0,
      state.shapeMismatches.length - MAX_CALLS,
    );
  }
  // 同时把 endpoint 标记为 shape-mismatch 来源
  recordCall({
    endpoint: endpoint as EndpointKey,
    source: "real-shape-mismatch",
    timestamp: Date.now(),
    durationMs: null,
    errorMessage: `${mismatches.length} field(s) mismatch`,
  });
}

function clear() {
  state.calls = [];
  state.shapeMismatches = [];
  state.callCountByEndpoint = {};
  state.lastCallByEndpoint = {};
  notify();
}

function subscribe(l: Listener): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function getSnapshot(): DevtoolsState {
  return state;
}

/**
 * 导出对接进度报告 markdown — 给 Rust 端 AI 看.
 */
function exportReportMarkdown(): string {
  const lines: string[] = [];
  lines.push("# IPC 运行时对接报告\n");
  lines.push(`生成时间: ${new Date().toISOString()}\n`);
  lines.push(`总调用次数: ${state.calls.length}\n`);
  lines.push(`形状不匹配次数: ${state.shapeMismatches.length}\n\n`);

  lines.push("## 端点调用统计\n\n");
  lines.push("| 端点 | 调用次数 | 来源 | 最近一次 |");
  lines.push("|------|---------:|------|----------|");
  const keys = Object.keys(state.callCountByEndpoint).sort();
  for (const k of keys) {
    const last = state.lastCallByEndpoint[k];
    const ago = last
      ? `${((Date.now() - last.timestamp) / 1000).toFixed(1)}s 前`
      : "—";
    lines.push(
      `| \`${k}\` | ${state.callCountByEndpoint[k]} | ${last?.source ?? "—"} | ${ago} |`,
    );
  }

  if (state.shapeMismatches.length > 0) {
    lines.push("\n## 字段形状不匹配\n\n");
    lines.push("| 端点 | 字段 | 期望 | 实际 |");
    lines.push("|------|------|------|------|");
    for (const m of state.shapeMismatches.slice(-50)) {
      lines.push(`| \`${m.endpoint}\` | \`${m.field}\` | ${m.expected} | ${m.actual} |`);
    }
  }
  return lines.join("\n");
}

export const devtools = {
  recordCall,
  recordShapeMismatch,
  clear,
  subscribe,
  getSnapshot,
  exportReportMarkdown,
};
