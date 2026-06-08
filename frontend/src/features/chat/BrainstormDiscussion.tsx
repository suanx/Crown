import { useEffect, useMemo, useRef, useState } from "react";
import type { BrainstormParticipant, Message } from "@/api";
import { cn } from "@/shared/lib/cn";
import { ReasoningBlock } from "./ReasoningBlock";
import { ToolGroup } from "./ToolGroup";
import { MarkdownRenderer } from "./MarkdownRenderer";
import { groupSegments } from "./groupSegments";

interface BrainstormDiscussionProps {
  runId: string;
  topic?: string;
  participants?: BrainstormParticipant[];
  status?: "running" | "done" | "error";
  messages: Message[];
}

export function BrainstormDiscussion({
  runId,
  topic,
  participants,
  status,
  messages,
}: BrainstormDiscussionProps) {
  const [expanded, setExpanded] = useState(true);
  const [large, setLarge] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const autoCollapsedRunRef = useRef<string | null>(null);
  const people = useMemo(() => {
    const byId = new Map<string, BrainstormParticipant>();
    for (const p of participants ?? []) byId.set(p.id, p);
    for (const message of messages) {
      const p = message.brainstorm?.participant;
      if (p) byId.set(p.id, p);
    }
    return Array.from(byId.values());
  }, [messages, participants]);
  const runningIds = new Set(
    messages
      .filter((message) => message.isStreaming)
      .map((message) => message.brainstorm?.participant.id)
      .filter(Boolean),
  );
  const running = runningIds.size > 0;
  const scrollKey = useMemo(
    () =>
      messages
        .map((message) => {
          const segmentState = message.segments
            .map((segment) =>
              segment.kind === "tool"
                ? `${segment.callId}:${segment.status}:${segment.result?.length ?? 0}`
                : `${segment.kind}:${segment.text.length}`,
            )
            .join(",");
          return `${message.id}:${message.content.length}:${message.isStreaming}:${segmentState}`;
        })
        .join("|"),
    [messages],
  );

  useEffect(() => {
    if (!expanded) return;
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: running ? "smooth" : "auto" });
  }, [expanded, running, scrollKey]);

  useEffect(() => {
    if (status === "running") {
      autoCollapsedRunRef.current = null;
    }
    if (status === "done" && autoCollapsedRunRef.current !== runId) {
      setExpanded(false);
      autoCollapsedRunRef.current = runId;
    }
  }, [runId, status]);

  return (
    <section
      className={cn(
        "mb-2 overflow-hidden rounded-2xl border border-border-subtle bg-input-bg/95",
        "shadow-[0_-16px_34px_rgba(0,0,0,0.2)]",
      )}
      data-brainstorm-run-id={runId}
    >
      <div className="h-10 px-3 flex items-center gap-2 hover:bg-hover/60 transition-colors">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="min-w-0 flex-1 h-full flex items-center gap-2 text-left"
        >
          <div className="flex -space-x-1 shrink-0">
            {people.map((p) => (
              <span
                key={p.id}
                className={cn(
                  "h-5 w-5 rounded-full border border-bg-canvas text-[10px] font-semibold text-white inline-flex items-center justify-center",
                  runningIds.has(p.id) && "animate-pulse-soft",
                )}
                style={{ backgroundColor: p.color }}
                title={`${p.name} · ${p.role}`}
              >
                {p.name.slice(0, 1)}
              </span>
            ))}
          </div>
          <span className="text-xs font-medium text-text-secondary shrink-0">
            多 Agent 讨论
          </span>
          {running && <span className="text-xs text-brand shrink-0">运行中</span>}
          {!running && status === "done" && (
            <span className="text-xs text-success shrink-0">已完成</span>
          )}
          {status === "error" && (
            <span className="text-xs text-danger shrink-0">出错</span>
          )}
          <span className="text-xs text-text-tertiary truncate min-w-0">
            {topic ?? "动态讨论"}
          </span>
        </button>
        <span className="text-xs text-text-tertiary shrink-0">
          {people.length} 人
        </span>
        {expanded && (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setLarge((v) => !v);
            }}
            className="text-xs text-text-tertiary hover:text-text-primary shrink-0"
          >
            {large ? "标准" : "放大"}
          </button>
        )}
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="text-xs text-text-tertiary hover:text-text-primary shrink-0"
        >
          {expanded ? "收起" : "展开"}
        </button>
      </div>

      {expanded && (
        <div
          ref={scrollRef}
          className={cn(
            "overflow-y-auto scrollable border-t border-border-subtle px-4 py-4 space-y-4",
            large ? "h-[66vh]" : "h-[50vh] min-h-[360px]",
          )}
        >
          {messages.length === 0 ? (
            <div className="h-full flex items-center justify-center text-xs text-text-tertiary">
              等待参与者发言
            </div>
          ) : (
            messages.map((message) => (
              <BrainstormMessage key={message.id} message={message} />
            ))
          )}
        </div>
      )}
    </section>
  );
}

function BrainstormMessage({ message }: { message: Message }) {
  const participant = message.brainstorm?.participant;
  if (!participant) return null;

  return (
    <div className="flex gap-3 min-w-0">
      <div
        className={cn(
          "mt-0.5 h-7 w-7 rounded-full shrink-0 text-[11px] font-semibold text-white flex items-center justify-center",
          message.isStreaming && "animate-pulse-soft",
        )}
        style={{ backgroundColor: participant.color }}
        title={participant.name}
      >
        {participant.name.slice(0, 1)}
      </div>
      <div className="min-w-0 flex-1">
        <div className="h-6 flex items-baseline gap-2 min-w-0">
          <span className="text-sm font-medium text-text-primary shrink-0">
            {participant.name}
          </span>
          <span className="text-xs text-text-tertiary truncate">
            {participant.role}
          </span>
        </div>
        <div className="min-w-0 space-y-2 text-sm leading-6">
          {groupSegments(message.segments).map((unit) => {
            if (unit.kind === "reasoning") {
              return (
                <div
                  key={`r-${unit.index}`}
                  className="max-h-[180px] overflow-y-auto scrollable pr-1"
                >
                  <ReasoningBlock
                    content={unit.text}
                    streaming={message.isStreaming}
                  />
                </div>
              );
            }
            if (unit.kind === "text") {
              return (
                <div key={`t-${unit.index}`} className="text-text-secondary">
                  <MarkdownRenderer
                    content={unit.text}
                    streaming={message.isStreaming && isLastText(message, unit.index)}
                  />
                </div>
              );
            }
            return (
              <div
                key={`g-${unit.index}`}
                className="max-h-[180px] overflow-y-auto scrollable pr-1"
              >
                <ToolGroup tools={unit.tools} />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function isLastText(message: Message, index: number): boolean {
  for (let i = message.segments.length - 1; i >= 0; i--) {
    const seg = message.segments[i];
    if (seg.kind === "text" && seg.text.trim().length > 0) {
      return i === index;
    }
  }
  return false;
}
