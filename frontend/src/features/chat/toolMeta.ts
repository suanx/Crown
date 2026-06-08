/**
 * ============================================================================
 * 工具展示元信息 — 动作词 / 图标 / 行级统计
 * ============================================================================
 *
 * 对话流去卡片化后，工具以「动作词 + 对象 + 统计」的极简单行呈现（对齐
 * Claude 桌面端 `Write src/x.ts +534` / `Edit src/y.ts +5`）。本模块集中：
 *   - TOOL_ACTION：工具名 → 简短中文动作词（折叠头/单行前缀）。
 *   - TOOL_ICON：工具名 → Phosphor 图标（防御性 fallback 到通用 ToolIcon）。
 *   - lineStats：从 input 推算 +增/-删 行数（Write=内容行数；Edit=diff 解析）。
 *
 * 防御：未知工具名（新工具 / MCP）一律 fallback，绝不返回 undefined 图标
 * （历史白屏根因）。
 */

import type { ToolName, ToolSegment } from "@/api";
import {
  FileIcon,
  EditIcon,
  TerminalIcon,
  GlobeIcon,
  FileSearchIcon,
  ToolIcon,
  AgentIcon,
  QuestionIcon,
  TasksIcon,
  SkillIcon,
} from "@/shared/icons/set";
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { parseEditDiff } from "./toolSummary";

/** 工具名 → 简短中文动作词（单行前缀，加粗显示）。 */
export const TOOL_ACTION: Record<string, string> = {
  read_file: "读取",
  view_file: "查看",
  list_directory: "列目录",
  list_dir: "列目录",
  write_file: "写入",
  write_to_file: "创建",
  edit_file: "编辑",
  replace_file_content: "替换",
  multi_replace_file_content: "多处替换",
  run_command: "运行",
  web_search: "搜索网络",
  web_fetch: "获取网页",
  grep: "搜索",
  grep_search: "全文搜索",
  glob: "查找",
  todo_write: "更新任务",
  skill: "技能",
  task: "子代理",
  ask_user_question: "提问",
  mcp_tool: "MCP",
};

/** 工具名 → 图标（带 fallback）。 */
const ICON_MAP: Record<string, PhIcon> = {
  read_file: FileIcon,
  view_file: FileIcon,
  list_directory: FileIcon,
  list_dir: FileIcon,
  write_file: EditIcon,
  write_to_file: EditIcon,
  edit_file: EditIcon,
  replace_file_content: EditIcon,
  multi_replace_file_content: EditIcon,
  run_command: TerminalIcon,
  web_search: GlobeIcon,
  web_fetch: GlobeIcon,
  grep: FileSearchIcon,
  grep_search: FileSearchIcon,
  glob: FileSearchIcon,
  todo_write: TasksIcon,
  skill: SkillIcon,
  task: AgentIcon,
  ask_user_question: QuestionIcon,
  mcp_tool: ToolIcon,
};

/** 取工具图标，未知名 fallback 到通用扳手图标（绝不返回 undefined）。 */
export function toolIcon(name: ToolName | string): PhIcon {
  return ICON_MAP[name] ?? ToolIcon;
}

/** 取动作词，未知名 fallback 到原始工具名。 */
export function toolAction(name: ToolName | string): string {
  return TOOL_ACTION[name] ?? name;
}

export interface LineStats {
  added: number;
  removed: number;
}

function inputString(input: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = input[key];
    if (typeof value === "string") return value;
  }
  for (const rawKey of ["arguments", "args", "__rawArguments", "__partialArguments"]) {
    const raw = input[rawKey];
    if (typeof raw !== "string") continue;
    for (const key of keys) {
      const parsed = extractJsonStringField(raw, key);
      if (parsed !== null) return parsed;
    }
  }
  return "";
}

function extractJsonStringField(source: string, key: string): string | null {
  const token = `"${key}"`;
  const keyStart = source.indexOf(token);
  if (keyStart < 0) return null;
  const afterKey = source.slice(keyStart + token.length);
  const colon = afterKey.indexOf(":");
  if (colon < 0) return null;
  const rest = afterKey.slice(colon + 1).trimStart();
  if (!rest.startsWith("\"")) return null;

  let out = "";
  for (let i = 1; i < rest.length; i++) {
    const ch = rest[i];
    if (ch === "\"") return out;
    if (ch !== "\\") {
      out += ch;
      continue;
    }
    const next = rest[++i];
    if (next == null) return out;
    switch (next) {
      case "\"":
      case "\\":
      case "/":
        out += next;
        break;
      case "n":
        out += "\n";
        break;
      case "r":
        out += "\r";
        break;
      case "t":
        out += "\t";
        break;
      case "u": {
        const hex = rest.slice(i + 1, i + 5);
        if (hex.length < 4) return out;
        const code = Number.parseInt(hex, 16);
        if (!Number.isNaN(code)) out += String.fromCharCode(code);
        i += 4;
        break;
      }
      default:
        out += next;
        break;
    }
  }
  return out;
}

export function writeContentFromInput(input: Record<string, unknown>): string {
  return inputString(input, ["content", "CodeContent"]);
}

export function editOldContentFromInput(input: Record<string, unknown>): string {
  return inputString(input, ["old_string", "TargetContent"]);
}

export function editNewContentFromInput(input: Record<string, unknown>): string {
  return inputString(input, ["new_string", "ReplacementContent"]);
}

export function isLineStatsTool(name: ToolName | string): boolean {
  return (
    name === "write_file" ||
    name === "write_to_file" ||
    name === "edit_file" ||
    name === "replace_file_content" ||
    name === "multi_replace_file_content"
  );
}

/**
 * 从工具段推算行级统计，用于单行右侧的 `+N -M` 徽章。
 *   - write_file：内容行数 → 全部计为新增。
 *   - edit_file：优先用结果文本解析 diff（parseEditDiff）；无结果时用
 *     input 的 old/new_string 行数估算。
 * 其它工具返回 null（不显示统计）。
 */
export function lineStats(seg: ToolSegment): LineStats | null {
  if (seg.name === "write_file" || seg.name === "write_to_file") {
    const content = writeContentFromInput(seg.input);
    if (!content) return null;
    return { added: content.split("\n").length, removed: 0 };
  }

  if (seg.name === "edit_file" || seg.name === "replace_file_content" || seg.name === "multi_replace_file_content") {
    if (seg.result) {
      const parsed = parseEditDiff(seg.result);
      if (parsed) return { added: parsed.add, removed: parsed.del };
    }

    let added = 0;
    let removed = 0;

    const oldStr = editOldContentFromInput(seg.input);
    const newStr = editNewContentFromInput(seg.input);

    if (oldStr || newStr) {
      added += newStr ? newStr.split("\n").length : 0;
      removed += oldStr ? oldStr.split("\n").length : 0;
    }

    if (Array.isArray(seg.input.ReplacementChunks)) {
      for (const chunk of seg.input.ReplacementChunks) {
        const cOld = typeof chunk.TargetContent === "string" ? chunk.TargetContent : "";
        const cNew = typeof chunk.ReplacementContent === "string" ? chunk.ReplacementContent : "";
        added += cNew ? cNew.split("\n").length : 0;
        removed += cOld ? cOld.split("\n").length : 0;
      }
    }

    if (added === 0 && removed === 0) return null;
    return { added, removed };
  }
  return null;
}
