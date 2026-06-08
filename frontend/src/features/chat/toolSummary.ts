/**
 * ============================================================================
 * 工具卡内容映射
 * ============================================================================
 *
 * 每个工具的卡片应该显示什么:
 *   - header 副标题 (summarizeToolInput): 一眼看出"对什么做了什么"
 *     read_file    → 文件路径
 *     write_file   → 文件路径
 *     edit_file    → 文件路径
 *     run_command  → 命令本身
 *     search_*     → 查询词
 *     web_search   → 查询词
 *     web_fetch    → URL
 *   - body 预览 (summarizeToolResult): 结果摘要
 *     read_file    → N 行 · X KB
 *     write_file   → 写入 N 行 · X KB
 *     edit_file    → 解析 diff (+adds -dels)
 *     list_dir     → N 个条目
 *     search_*     → N 个匹配
 *     run_command  → exit code + 输出
 *
 * 后端工具入参字段 (来自 crates/tools/src):
 *   read_file:      { path, offset?, limit? }
 *   list_directory: { path, recursive?, max_depth? }
 *   grep:           { pattern, path?, glob?, case_sensitive?, max_results? }
 *   glob:           { pattern, path?, max_results? }
 *   write_file:     { path, content }
 *   edit_file:      { path, old_string, new_string, replace_all? }
 *   run_command:    { command, cwd?, timeout_secs? }
 *   web_search:     { query }
 *   web_fetch:      { url }
 * ----------------------------------------------------------------------------
 */

import type { ToolName } from "@/api";

// ── 输入字段读取 helper ─────────────────────────────────────────────────

function str(input: Record<string, unknown>, key: string): string | null {
  const v = input[key];
  return typeof v === "string" ? v : null;
}

/** 取文件路径 — 多数文件工具用 `path`. */
export function extractPath(input: Record<string, unknown>): string | null {
  return str(input, "path");
}

/** 取最后一段文件名 — header 紧凑显示用. */
export function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

// ── 格式化 ───────────────────────────────────────────────────────────────

export function formatBytes(n: number): string {
  if (n < 1000) return `${n} B`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)} KB`;
  return `${(n / 1_000_000).toFixed(1)} MB`;
}

export function countLines(text: string): number {
  if (!text) return 0;
  return text.split(/\r?\n/).length;
}

function clip(s: string, max: number): string {
  const t = s.trim();
  return t.length > max ? t.slice(0, max) + "…" : t;
}

function firstNonEmptyLine(text: string): string {
  for (const line of text.split(/\r?\n/)) {
    if (line.trim()) return line.trim();
  }
  return "";
}

// ── Header 副标题 — "对什么操作" ──────────────────────────────────────────

/**
 * 工具卡 header 上跟在工具名后面的灰色副标题.
 * 一眼看出操作对象 (文件路径 / 命令 / 查询词 / URL).
 */
export function summarizeToolInput(
  tool: ToolName,
  input: Record<string, unknown>,
): string {
  switch (tool) {
    case "read_file":
    case "write_file":
    case "edit_file":
    case "list_directory": {
      const p = extractPath(input);
      return p ?? "";
    }
    case "run_command": {
      const cmd = str(input, "command");
      return cmd ? clip(cmd, 80) : "";
    }
    case "grep":
    case "glob": {
      const pattern = str(input, "pattern") ?? str(input, "query");
      return pattern ? `"${clip(pattern, 60)}"` : "";
    }
    case "web_search": {
      const q = str(input, "query");
      return q ? `"${clip(q, 60)}"` : "";
    }
    case "web_fetch": {
      const url = str(input, "url");
      return url ? clip(url, 70) : "";
    }
    case "todo_write":
      return "";
    case "mcp_tool":
    default: {
      // 通用: 第一个 string 入参
      for (const [k, v] of Object.entries(input)) {
        if (typeof v === "string" && v.length < 80) return `${k}: ${v}`;
      }
      return "";
    }
  }
}

// ── Body 结果摘要 — "做了什么 / 结果是什么" ────────────────────────────────

export interface ToolResultSummary {
  /** 一行摘要 (绿色/中性), e.g. "42 行 · 1.2 KB". */
  summary: string;
  isError: boolean;
}

/**
 * 工具结果的一行摘要 — 折叠状态下显示.
 * 返回 null 表示无特定摘要 (调用方回退到通用预览).
 */
export function summarizeToolResult(
  tool: ToolName,
  result: string,
): ToolResultSummary | null {
  const r = result ?? "";

  switch (tool) {
    case "read_file": {
      // 后端读文件结果带行号 + 可能的 system-reminder
      if (r.includes("<system-reminder>") && r.includes("empty")) {
        return { summary: "空文件", isError: false };
      }
      if (r.startsWith("File unchanged")) {
        return { summary: "内容未变 (已缓存)", isError: false };
      }
      const lines = countLines(r);
      return { summary: `${lines} 行 · ${formatBytes(r.length)}`, isError: false };
    }
    case "write_file": {
      const lines = countLines(r);
      return { summary: `写入 ${lines} 行 · ${formatBytes(r.length)}`, isError: false };
    }
    case "edit_file": {
      const stats = countDiffStats(r);
      if (stats) {
        return {
          summary: `+${stats.add} -${stats.del}`,
          isError: false,
        };
      }
      return { summary: "已修改", isError: false };
    }
    case "list_directory": {
      if (r.startsWith("(empty")) return { summary: "空目录", isError: false };
      const entries = r.split(/\r?\n/).filter((l) => l.trim()).length;
      return { summary: `${entries} 个条目`, isError: false };
    }
    case "glob": {
      if (r.startsWith("No files")) return { summary: "无匹配文件", isError: false };
      const matches = r.split(/\r?\n/).filter((l) => l.trim()).length;
      return { summary: `${matches} 个文件`, isError: false };
    }
    case "grep": {
      if (r.startsWith("No matches")) return { summary: "无匹配", isError: false };
      const matches = r.split(/\r?\n/).filter((l) => l.trim()).length;
      return { summary: `${matches} 处匹配`, isError: false };
    }
    case "run_command": {
      const exit = extractExitCode(r);
      const firstLine = firstNonEmptyLine(stripExitPreamble(r));
      if (exit !== null && exit !== 0) {
        return {
          summary: `exit ${exit}${firstLine ? " · " + clip(firstLine, 50) : ""}`,
          isError: true,
        };
      }
      return {
        summary: firstLine ? clip(firstLine, 60) : "完成",
        isError: false,
      };
    }
    case "web_fetch": {
      return { summary: `${formatBytes(r.length)} 内容`, isError: false };
    }
    case "web_search": {
      const lines = r.split(/\r?\n/).filter((l) => l.trim()).length;
      return { summary: `${lines} 条结果`, isError: false };
    }
    case "todo_write": {
      return { summary: "任务列表已更新", isError: false };
    }
    default:
      return null;
  }
}

// ── Diff 解析 (edit_file 结果文本 → 结构化 diff) ─────────────────────────────

export interface ParsedDiffLine {
  kind: "ctx" | "add" | "del";
  lineNo: number | null;
  text: string;
}

export interface ParsedDiff {
  lines: ParsedDiffLine[];
  add: number;
  del: number;
}

/**
 * 解析后端 edit_file 的结果文本.
 *
 * 后端格式 (format_edit_diff):
 *   The file <path> has been updated. Here's a snippet of the changes:
 *      123 - old line
 *      124 + new line
 *      125   context line
 *
 * 行号右对齐 6 宽,然后 ` - ` / ` + ` / `   ` 标记.
 */
export function parseEditDiff(result: string): ParsedDiff | null {
  const lines = result.split(/\r?\n/);
  const parsed: ParsedDiffLine[] = [];
  let add = 0;
  let del = 0;

  // 匹配:前导空格 + 行号 + 空格 + 标记(-/+/空) + 空格 + 内容
  const re = /^\s*(\d+)\s([-+ ])\s?(.*)$/;

  for (const line of lines) {
    // 跳过 header 描述行
    if (line.startsWith("The file ") || line.startsWith("(no textual")) continue;
    const m = line.match(re);
    if (!m) continue;
    const lineNo = Number(m[1]);
    const mark = m[2];
    const text = m[3] ?? "";
    if (mark === "+") {
      parsed.push({ kind: "add", lineNo, text });
      add++;
    } else if (mark === "-") {
      parsed.push({ kind: "del", lineNo, text });
      del++;
    } else {
      parsed.push({ kind: "ctx", lineNo, text });
    }
  }

  if (parsed.length === 0) return null;
  return { lines: parsed, add, del };
}

function countDiffStats(result: string): { add: number; del: number } | null {
  const parsed = parseEditDiff(result);
  if (!parsed) return null;
  return { add: parsed.add, del: parsed.del };
}

// ── run_command 输出解析 ───────────────────────────────────────────────────

/**
 * 从命令输出里提取 exit code.
 * 后端 shell 工具 preamble 形如 "exit 0:" / "exit 1:" / "[exit N]".
 */
export function extractExitCode(result: string): number | null {
  const m =
    result.match(/^exit\s+(\d+)/i) ?? result.match(/\[exit\s+(\d+)\]/i);
  return m ? Number(m[1]) : null;
}

/** 去掉 exit preamble,返回纯输出. */
export function stripExitPreamble(result: string): string {
  return result.replace(/^exit\s+\d+:\s*/i, "").replace(/^\[exit\s+\d+\]\s*/i, "");
}

// ── 解析命令 (run_command 显示 $ cmd) ───────────────────────────────────────

export function extractCommand(input: Record<string, unknown>): string {
  return str(input, "command") ?? "";
}
