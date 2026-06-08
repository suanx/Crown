import { useState } from "react";
import type { ToolSegment } from "@/api";
import { Icon } from "@/shared/icons/Icon";
import { CaretRightIcon } from "@/shared/icons/set";
import { Spinner } from "@/shared/ui/Spinner";
import { cn } from "@/shared/lib/cn";
import {
  toolAction,
  toolIcon,
  lineStats,
  writeContentFromInput,
  editNewContentFromInput,
  isLineStatsTool,
} from "./toolMeta";
import { summarizeToolInput, extractCommand } from "./toolSummary";
import {
  CommandBody,
  EditDiffBody,
  SearchBody,
  ListBody,
  ResultPre,
} from "./toolCardBodies";
import { WebSearchBody } from "./WebSearchBody";
import { DiffView } from "./DiffView";
import { SubAgentPanel } from "./SubAgentPanel";
import { AnimatedNumber } from "@/shared/ui/AnimatedNumber";

/**
 * 组内单工具行 —— 对齐 Claude 桌面端：动作词 + 对象 + `+N` 统计 + 状态，
 * 默认极简一行；可展开看 body（diff / 命令输出 / 搜索结果 / 文件内容）。
 *
 * 无卡片背板：行本身只是 hover 高亮的一条，body 在其下方缩进展开。
 * 颜色克制：仅出错用 danger，其余中性；运行中用品牌色 Spinner。
 */
export function ToolRow({ seg }: { seg: ToolSegment }) {
  const expandable = hasBody(seg);
  // 默认折叠（对齐参考：组展开后子项仍折叠，点单项才看细节）；
  // 出错的默认展开，方便立刻看到错在哪。
  const [open, setOpen] = useState(seg.status === "error");

  const stats = lineStats(seg);
  const action = toolAction(seg.name);
  const obj = summarizeToolInput(seg.name, seg.input);
  const RowIcon = toolIcon(seg.name);
  const running = seg.status === "running" || seg.status === "pending_approval";
  const showSpinner = running && !isLineStatsTool(seg.name);
  const failed = seg.status === "error";

  return (
    <div className="min-w-0">
      <button
        onClick={() => expandable && setOpen((v) => !v)}
        disabled={!expandable}
        className={cn(
          "group/row w-full flex items-center gap-2 py-1 rounded-md -mx-1.5 px-1.5 text-left min-w-0",
          expandable && "hover:bg-hover active:scale-[0.99] transition-all cursor-pointer",
          !expandable && "cursor-default",
        )}
      >
        {/* 展开箭头位（不可展开则留空占位，保持左缘对齐） */}
        {expandable ? (
          <Icon
            icon={CaretRightIcon}
            size={11}
            className={cn(
              "shrink-0 opacity-40 transition-transform duration-200",
              open && "rotate-90",
            )}
          />
        ) : (
          <span className="shrink-0 w-[11px]" />
        )}

        <Icon
          icon={RowIcon}
          size={14}
          weight="duotone"
          className={cn(
            "shrink-0",
            failed ? "text-danger" : "text-text-tertiary",
          )}
        />

        <span className="shrink-0 text-sm font-medium text-text-primary">
          {action}
        </span>

        {obj && (
          <span className="text-xs text-text-tertiary font-mono truncate min-w-0">
            {obj}
          </span>
        )}

        <span className="ml-auto flex items-center gap-2 shrink-0">
          {stats && (stats.added > 0 || stats.removed > 0) && (
            <span className="text-xs font-mono">
              {stats.added > 0 && (
                <span className="text-success">
                  +<AnimatedNumber value={stats.added} />
                </span>
              )}
              {stats.added > 0 && stats.removed > 0 && " "}
              {stats.removed > 0 && (
                <span className="text-danger">
                  -<AnimatedNumber value={stats.removed} />
                </span>
              )}
            </span>
          )}
          {showSpinner && <Spinner size={12} />}
          {failed && <span className="text-xs text-danger">失败</span>}
        </span>
      </button>

      {expandable && open && (
        <div className="mt-1 mb-1 animate-slide-up">
          {seg.subAgent && (
            <div className="mb-2">
              <SubAgentPanel activity={seg.subAgent} />
            </div>
          )}
          <ToolBody seg={seg} />
          {seg.errorMessage && (
            <div className="mt-1.5 text-xs text-danger">{seg.errorMessage}</div>
          )}
        </div>
      )}
    </div>
  );
}

/** 是否有可展开的 body（有结果 / 有 diff / 有子代理 / 有错误信息）。 */
function hasBody(seg: ToolSegment): boolean {
  return Boolean(
    seg.result ||
      seg.diff ||
      seg.subAgent ||
      seg.errorMessage ||
      ((seg.name === "write_file" || seg.name === "write_to_file") &&
        writeContentFromInput(seg.input)) ||
      (seg.name === "edit_file" && editNewContentFromInput(seg.input)),
  );
}

/** body 分发 —— 复用既有 per-tool 渲染器，仅承载在竖线缩进内。 */
function ToolBody({ seg }: { seg: ToolSegment }) {
  const { name, input, result, diff } = seg;

  if (diff) {
    return <DiffView path={diff.path} before={diff.before} after={diff.after} />;
  }

  switch (name) {
    case "run_command":
      return <CommandBody command={extractCommand(input)} result={result} />;
    case "edit_file":
      if (result) {
        return <EditDiffBody path={String(input.path ?? "")} result={result} />;
      }
      {
        const content = editNewContentFromInput(input);
        return content ? <ResultPre text={content} collapsible /> : null;
      }
    case "write_file": {
      const content = writeContentFromInput(input);
      return content ? <ResultPre text={content} collapsible /> : null;
    }
    case "read_file":
      return result ? <ResultPre text={result} collapsible /> : null;
    case "list_directory":
      return result ? <ListBody result={result} /> : null;
    case "grep":
    case "glob":
      return result ? <SearchBody result={result} /> : null;
    case "web_search":
      return result ? <WebSearchBody result={result} /> : null;
    case "web_fetch":
      return result ? <ResultPre text={result} collapsible /> : null;
    default:
      return result ? <ResultPre text={result} collapsible /> : null;
  }
}
