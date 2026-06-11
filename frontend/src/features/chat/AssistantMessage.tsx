import { memo } from "react";
import type { Message } from "@/api";
import { cn } from "@/shared/lib/cn";
import { ReasoningBlock } from "./ReasoningBlock";
import { ToolGroup } from "./ToolGroup";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { MessageMeta } from "./MessageMeta";
import { groupSegments } from "./groupSegments";
import { Icon } from "@/shared/icons/Icon";
import { CopyIcon, ReasoningIcon } from "@/shared/icons/set";

interface AssistantMessageProps {
  message: Message;
  /** 是否是当前 turn 正在输出的最后一条 assistant message */
  isActive?: boolean;
}

/**
 * 助手消息 — 全宽,左对齐,无气泡背景.
 * 头像 28 (h-7 w-7),内部图标 14.
 * 内部 vertical gap 12 (space-y-3),hover actions 与正文之间 8.
 *
 * 渲染走 segments[],按交错顺序渲染 text/reasoning/tool.
 *
 * memo 通过引用比较短路:chatStore 的 upsertMessage / .map 保证未变化
 * sibling message 在 reducer 里引用稳定,memo 后这些消息整棵子树跳过
 * 重渲染.只有"当前正在流式 push delta"那一条 message 引用变.
 */
function AssistantMessageImpl({
  message,
  isActive,
}: AssistantMessageProps) {
  // 头像呼吸灯:只在当前正在工作的那条(最后一条 streaming assistant)
  const shouldPulse = isActive && message.isStreaming;
  const brainstorm = message.brainstorm;

  return (
    <div className="group flex gap-3">
      <div className="shrink-0 mt-0.5">
        {brainstorm ? (
          <div
            className={cn(
              "h-7 w-7 rounded-full border border-border-subtle flex items-center justify-center text-[11px] font-semibold text-white",
              shouldPulse && "animate-pulse-soft transition-transform",
            )}
            style={{ backgroundColor: brainstorm.participant.color }}
            title={brainstorm.participant.name}
          >
            {brainstorm.participant.name.slice(0, 1)}
          </div>
        ) : (
          <div
            className={cn(
              "h-7 w-7 rounded-full bg-elevated border border-border-subtle flex items-center justify-center",
              shouldPulse && "animate-pulse-soft transition-transform",
            )}
          >
            <Icon icon={ReasoningIcon} size={15} weight="duotone" className="text-brand" />
          </div>
        )}
      </div>

      <div className="flex-1 min-w-0 space-y-3">
        {brainstorm && (
          <div className="flex items-baseline gap-2 -mb-1">
            <span className="text-sm font-medium text-text-primary">
              {brainstorm.participant.name}
            </span>
            <span className="text-xs text-text-tertiary">
              {brainstorm.participant.role}
            </span>
          </div>
        )}
        {groupSegments(message.segments).map((unit) => {
          if (unit.kind === "reasoning") {
            return (
              <ReasoningBlock
                key={`r-${unit.index}`}
                content={unit.text}
                // 思维链的"流式态"绑定整条消息，而非"是否最后一段"。否则
                // reasoning 后面跟了正文时，reasoning 会被误判为已完成而提前
                // 折叠（用户感觉"内容没了"）。
                streaming={message.isStreaming}
              />
            );
          }
          if (unit.kind === "text") {
            return (
              <div key={`t-${unit.index}`} className="text-text-primary text-msg">
                <MarkdownRenderer
                  content={unit.text}
                  streaming={message.isStreaming && isLastUnitText(message, unit.index)}
                />
              </div>
            );
          }
          // toolGroup：连续工具段收成一组
          return <ToolGroup key={`g-${unit.index}`} tools={unit.tools} />;
        })}

        {message.usage && !message.isStreaming && (
          <MessageMeta usage={message.usage} messageId={message.id} />
        )}

        {!message.isStreaming && (
          <div className="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1">
            <ActionBtn
              icon={CopyIcon}
              label="复制"
              onClick={() => {
                const text = message.segments
                  .filter((s) => s.kind === "text")
                  .map((s) => s.text)
                  .join("\n\n")
                  .trim();
                if (text) void navigator.clipboard.writeText(text).catch(() => {});
              }}
            />
          </div>
        )}
      </div>
    </div>
  );
}

function ActionBtn({
  icon,
  label,
  onClick,
}: {
  icon: typeof CopyIcon;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      title={label}
      onClick={onClick}
      className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-secondary transition-colors focus-ring"
    >
      <Icon icon={icon} size={12} />
    </button>
  );
}

/**
 * 默认 memo 比较:浅比较 props.message.shouldnt 引用稳定即跳过.
 * chatStore reducer 用不可变 upsertMessage 保证未改 message 在每次
 * setState 后引用不变,因此 memo 真的能短路掉非当前流式消息的重渲染.
 */
export const AssistantMessage = memo(AssistantMessageImpl);

/**
 * 该 text 单元是否是消息里最后一个文本段 —— 只有最后一段在流式中才走
 * MarkdownRenderer 的 streaming 增量路径（前面的文本段已定稿）。
 */
function isLastUnitText(message: Message, index: number): boolean {
  for (let i = message.segments.length - 1; i >= 0; i--) {
    const seg = message.segments[i];
    if (seg.kind === "text" && seg.text.trim().length > 0) {
      return i === index;
    }
  }
  return false;
}
