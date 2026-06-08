/**
 * ============================================================================
 * 工具卡 body 渲染组件 — 每个工具类型的专属展示
 * ============================================================================
 *
 * 每个工具类型使用专属渲染:
 *   - CommandBody:  $ command + 带色彩的输出 + exit code
 *   - EditDiffBody: 行号 diff (+绿 -红), +N/-M 统计
 *   - ReadBody:     读取结果代码块
 *   - ListBody:     目录条目列表
 *   - SearchBody:   匹配行 (file:line:content)
 *   - GenericBody:  通用 input + result
 */

import { useState } from "react";
import { Icon } from "@/shared/icons/Icon";
import { TerminalIcon, FileIcon } from "@/shared/icons/set";
import {
  parseEditDiff,
  stripExitPreamble,
  extractExitCode,
  type ParsedDiffLine,
} from "./toolSummary";

const CODE_STYLE = {
  backgroundColor: "var(--ds-code-bg)",
  color: "var(--ds-code-text)",
} as const;

const COLLAPSE_THRESHOLD = 800;

// ── 命令执行 body — $ cmd + 输出 ────────────────────────────────────────────

export function CommandBody({
  command,
  result,
}: {
  command: string;
  result?: string;
}) {
  const exit = result ? extractExitCode(result) : null;
  const output = result ? stripExitPreamble(result) : "";
  const failed = exit !== null && exit !== 0;

  return (
    <div className="rounded-md overflow-hidden text-sm font-mono" style={CODE_STYLE}>
      {/* 命令行 */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b" style={{ borderColor: "rgba(255,255,255,0.08)" }}>
        <Icon icon={TerminalIcon} size={12} className="opacity-60 shrink-0" />
        <span className="text-[#4ade80] shrink-0">$</span>
        <span className="break-all">{command}</span>
      </div>
      {/* 输出 */}
      {output.trim() && (
        <pre
          className="px-3 py-2 whitespace-pre-wrap break-words leading-[1.6] overflow-x-auto"
          style={{ color: failed ? "#f87171" : "var(--ds-code-text)" }}
        >
          {output}
        </pre>
      )}
    </div>
  );
}

// ── 编辑 diff body — 行号 + +/- 标记 ─────────────────────────────────────────

export function EditDiffBody({
  path,
  result,
}: {
  path: string;
  result: string;
}) {
  const parsed = parseEditDiff(result);
  if (!parsed) {
    // 无法解析 diff — 回退到纯文本
    return <ResultPre text={result} />;
  }

  return (
    <div className="rounded-md overflow-hidden border border-border-subtle" style={CODE_STYLE}>
      <div
        className="flex items-center gap-2 px-3 py-1.5 border-b text-xs font-mono"
        style={{ color: "var(--ds-code-text)", borderColor: "rgba(255,255,255,0.08)" }}
      >
        <Icon icon={FileIcon} size={12} className="opacity-60" />
        <span className="truncate">{path}</span>
        <span className="ml-auto shrink-0">
          <span className="text-[#4ade80]">+{parsed.add}</span>{" "}
          <span className="text-[#f87171]">-{parsed.del}</span>
        </span>
      </div>
      <div className="overflow-x-auto text-sm leading-[1.6] font-mono py-1">
        {parsed.lines.map((line, i) => (
          <DiffLineRow key={i} line={line} />
        ))}
      </div>
    </div>
  );
}

function DiffLineRow({ line }: { line: ParsedDiffLine }) {
  const bg =
    line.kind === "add"
      ? "rgba(74, 222, 128, 0.12)"
      : line.kind === "del"
        ? "rgba(248, 113, 113, 0.15)"
        : undefined;
  const fg =
    line.kind === "add"
      ? "#4ade80"
      : line.kind === "del"
        ? "#f87171"
        : "var(--ds-code-text)";
  const mark = line.kind === "add" ? "+" : line.kind === "del" ? "-" : " ";

  return (
    <div className="px-3 flex items-baseline gap-2" style={{ backgroundColor: bg }}>
      <span className="select-none inline-block w-10 text-right opacity-40 shrink-0 text-xs">
        {line.lineNo ?? ""}
      </span>
      <span className="select-none inline-block w-3 text-center opacity-70 shrink-0" style={{ color: fg }}>
        {mark}
      </span>
      <span style={{ color: line.kind === "ctx" ? undefined : fg }} className={line.kind === "ctx" ? "opacity-70" : ""}>
        {line.text || "\u00A0"}
      </span>
    </div>
  );
}

// ── 搜索结果 body — file:line:content 高亮 ──────────────────────────────────

export function SearchBody({ result }: { result: string }) {
  const lines = result.split(/\r?\n/).filter((l) => l.trim());
  if (lines.length === 0 || result.startsWith("No ")) {
    return <div className="text-xs text-text-tertiary px-1">{result || "无结果"}</div>;
  }

  // 解析 path:line:content (grep) 或 纯路径 (glob)
  const visible = lines.slice(0, 20);
  const hidden = lines.length - visible.length;

  return (
    <div className="rounded-md text-sm font-mono overflow-x-auto" style={CODE_STYLE}>
      <div className="px-3 py-2 space-y-0.5">
        {visible.map((line, i) => {
          const m = line.match(/^(.+?):(\d+):(.*)$/);
          if (m) {
            return (
              <div key={i} className="flex gap-2 items-baseline">
                <span className="text-[#7aa2f7] shrink-0">{m[1]}</span>
                <span className="opacity-40 shrink-0">{m[2]}</span>
                <span className="opacity-90 truncate">{m[3].trim()}</span>
              </div>
            );
          }
          return (
            <div key={i} className="opacity-90 truncate">
              {line}
            </div>
          );
        })}
        {hidden > 0 && (
          <div className="text-xs opacity-40 pt-1">… 还有 {hidden} 行</div>
        )}
      </div>
    </div>
  );
}

// ── 目录列表 body ───────────────────────────────────────────────────────────

export function ListBody({ result }: { result: string }) {
  const lines = result.split(/\r?\n/).filter((l) => l.trim());
  const visible = lines.slice(0, 30);
  const hidden = lines.length - visible.length;

  return (
    <div className="rounded-md text-sm font-mono overflow-x-auto" style={CODE_STYLE}>
      <div className="px-3 py-2 space-y-0.5">
        {visible.map((line, i) => {
          const isDir = line.startsWith("dir");
          return (
            <div key={i} className={isDir ? "text-[#7aa2f7]" : "opacity-90"}>
              {line}
            </div>
          );
        })}
        {hidden > 0 && (
          <div className="text-xs opacity-40 pt-1">… 还有 {hidden} 个条目</div>
        )}
      </div>
    </div>
  );
}

// ── 通用结果 pre 块 (可折叠) ─────────────────────────────────────────────────

export function ResultPre({
  text,
  collapsible,
}: {
  text: string;
  collapsible?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const shouldCollapse = collapsible && text.length > COLLAPSE_THRESHOLD;
  const display =
    shouldCollapse && !expanded ? text.slice(0, COLLAPSE_THRESHOLD) + "…" : text;

  return (
    <div
      className="rounded-md text-sm leading-[1.6] font-mono px-3 py-2 overflow-x-auto"
      style={CODE_STYLE}
    >
      <pre className="whitespace-pre-wrap break-words">{display}</pre>
      {shouldCollapse && (
        <button
          onClick={() => setExpanded((v) => !v)}
          className="mt-2 text-xs text-brand hover:underline focus-ring rounded"
        >
          {expanded ? "收起" : `展开全部 (${(text.length / 1000).toFixed(1)}K 字符)`}
        </button>
      )}
    </div>
  );
}
