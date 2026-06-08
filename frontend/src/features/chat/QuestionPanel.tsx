import { useMemo, useState } from "react";
import { useChatStore } from "@/stores/chatStore";
import type { QuestionRequestEvent } from "@/api/contracts";
import { QuestionCard, type QuestionCardValue } from "./QuestionCard";
import { cn } from "@/shared/lib/cn";

interface QuestionPanelProps {
  threadId: string;
}

/**
 * 结构化问答上浮面板入口 —— 取当前 thread 的首项 pending 问答。
 *
 * 主体抽到 [`QuestionPanelInner`]，用 `toolUseId` 做 React key：不同问答到来时
 * 自动重挂载、内部翻页/答案 state 干净重置（避免上一轮残留串到下一轮）。
 * 无 pending 时返回 null，不占位。
 */
export function QuestionPanel({ threadId }: QuestionPanelProps) {
  const pending = useChatStore((s) => s.pendingQuestions);
  const req = useMemo(
    () => pending.find((q) => q.threadId === threadId),
    [pending, threadId],
  );
  if (!req) return null;
  return <QuestionPanelInner key={req.toolUseId} threadId={threadId} req={req} />;
}

interface QuestionPanelInnerProps {
  threadId: string;
  req: QuestionRequestEvent;
}

/**
 * 问答面板主体 —— 紧贴输入框上沿浮出的固定尺寸卡片，底部向下投影。
 *
 * 装多题，一次显示一题，← → 翻页；单选选中自动前进，全部答完点"提交"。
 * 绝不撑大对话区/输入框（max-height 固定 + 内部滚动）。取消 =
 * `submitAnswers(..., cancelled=true)`（拒绝工具调用）。
 */
function QuestionPanelInner({ threadId, req }: QuestionPanelInnerProps) {
  const submitAnswers = useChatStore((s) => s.submitAnswers);
  const [idx, setIdx] = useState(0);
  const [values, setValues] = useState<Record<string, QuestionCardValue>>({});

  const total = req.questions.length;
  const safeIdx = Math.min(idx, total - 1);
  const q = req.questions[safeIdx];
  const key = q.question;
  const value: QuestionCardValue = values[key] ?? { selected: [], other: "" };
  const isLast = safeIdx >= total - 1;

  function setValue(v: QuestionCardValue) {
    setValues((prev) => ({ ...prev, [key]: v }));
  }

  function next() {
    if (!isLast) setIdx((i) => i + 1);
  }
  function prev() {
    if (safeIdx > 0) setIdx((i) => i - 1);
  }

  function buildAnswers() {
    return req.questions.map((qq) => {
      const v = values[qq.question] ?? { selected: [], other: "" };
      return {
        question: qq.question,
        selected: v.selected,
        other: v.other.trim().length > 0 ? v.other.trim() : null,
      };
    });
  }

  function handleSubmit() {
    void submitAnswers(threadId, req.toolUseId, buildAnswers(), false);
  }
  function handleCancel() {
    void submitAnswers(threadId, req.toolUseId, [], true);
  }
  function handlePick() {
    // 单选选中：非最后一题自动前进；最后一题不自动提交（避免 React state
    // 异步更新读不到刚选的值），由用户点"提交"统一交。
    if (!isLast) next();
  }

  return (
    <div
      className={cn(
        "rounded-2xl border border-border-subtle bg-elevated",
        "shadow-[0_8px_24px_-4px_rgba(0,0,0,0.35)]",
        "flex flex-col gap-3 p-4 mb-2",
        "max-h-[340px] overflow-hidden",
      )}
      data-testid="question-panel"
    >
      <div className="flex items-center justify-between">
        <span className="text-xs text-text-tertiary">
          问题 {safeIdx + 1} / {total}
        </span>
        <button
          onClick={handleCancel}
          className="text-xs text-text-tertiary hover:text-danger transition-colors"
        >
          取消
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">
        <QuestionCard
          key={key}
          question={q}
          value={value}
          onChange={setValue}
          onPick={handlePick}
        />
      </div>

      <div className="flex items-center justify-between pt-1">
        <button
          onClick={prev}
          disabled={safeIdx === 0}
          className={cn(
            "h-7 w-7 rounded-md flex items-center justify-center transition-colors",
            safeIdx === 0
              ? "text-text-tertiary/40 cursor-not-allowed"
              : "text-text-secondary hover:bg-hover",
          )}
          aria-label="上一题"
        >
          ←
        </button>

        {isLast ? (
          <button
            onClick={handleSubmit}
            className="rounded-md bg-brand text-white px-4 py-1.5 text-sm hover:bg-brand-hover transition-colors"
          >
            提交
          </button>
        ) : (
          <button
            onClick={next}
            className="rounded-md bg-elevated border border-border-subtle text-text-secondary px-4 py-1.5 text-sm hover:bg-hover transition-colors"
          >
            下一题
          </button>
        )}

        <button
          onClick={next}
          disabled={isLast}
          className={cn(
            "h-7 w-7 rounded-md flex items-center justify-center transition-colors",
            isLast
              ? "text-text-tertiary/40 cursor-not-allowed"
              : "text-text-secondary hover:bg-hover",
          )}
          aria-label="下一题"
        >
          →
        </button>
      </div>
    </div>
  );
}
