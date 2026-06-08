import { useState } from "react";
import type { QuestionDto } from "@/api/contracts";
import { cn } from "@/shared/lib/cn";

export interface QuestionCardValue {
  /** 选中的 label 集合（单选 1 个，多选 N 个）。 */
  selected: string[];
  /** "其他"填空文本（未展开/未填为 ""）。 */
  other: string;
}

interface QuestionCardProps {
  question: QuestionDto;
  value: QuestionCardValue;
  onChange: (v: QuestionCardValue) => void;
  /** 单选选中即触发（用于自动前进）。 */
  onPick?: () => void;
}

/**
 * 单题渲染卡 —— 单选 radio / 多选 checkbox + 折叠"其他"填空。
 *
 * 容器固定尺寸由父级 QuestionPanel 约束，这里只负责内容 + 自身滚动。
 * "其他"为方案丙：默认折叠成一个小链接，点开才出现输入框，不抢视觉。
 */
export function QuestionCard({ question, value, onChange, onPick }: QuestionCardProps) {
  const [showOther, setShowOther] = useState(value.other.length > 0);

  function toggle(label: string) {
    if (question.multiSelect) {
      const has = value.selected.includes(label);
      const next = has
        ? value.selected.filter((l) => l !== label)
        : [...value.selected, label];
      onChange({ ...value, selected: next });
    } else {
      onChange({ ...value, selected: [label] });
      onPick?.();
    }
  }

  return (
    <div className="flex flex-col gap-3 h-full">
      <div className="flex flex-col gap-1">
        <span className="text-xs font-medium text-text-tertiary uppercase tracking-wide">
          {question.header}
        </span>
        <span className="text-msg text-text-primary">{question.question}</span>
      </div>

      <div className="flex flex-col gap-1.5 overflow-y-auto scrollable min-h-0">
        {question.options.map((opt) => {
          const active = value.selected.includes(opt.label);
          return (
            <button
              key={opt.label}
              onClick={() => toggle(opt.label)}
              className={cn(
                "text-left rounded-lg border px-3 py-2 transition-all active:scale-[0.98]",
                active
                  ? "border-brand bg-brand/10"
                  : "border-border-subtle hover:bg-hover",
              )}
            >
              <div className="flex items-center gap-2">
                <span
                  className={cn(
                    "shrink-0 flex items-center justify-center text-[10px] leading-none",
                    question.multiSelect ? "h-4 w-4 rounded" : "h-4 w-4 rounded-full",
                    "border",
                    active
                      ? "border-brand bg-brand text-white"
                      : "border-border-subtle",
                  )}
                >
                  {active ? "✓" : ""}
                </span>
                <div className="flex flex-col">
                  <span className="text-sm text-text-primary">{opt.label}</span>
                  {opt.description && (
                    <span className="text-xs text-text-tertiary">
                      {opt.description}
                    </span>
                  )}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {showOther ? (
        <input
          autoFocus
          value={value.other}
          onChange={(e) => onChange({ ...value, other: e.target.value })}
          placeholder="其他（自由填写）…"
          className="rounded-lg border border-border-subtle bg-input-bg px-3 py-2 text-sm text-text-primary outline-none focus:border-border-focus"
        />
      ) : (
        <button
          onClick={() => setShowOther(true)}
          className="self-start text-xs text-text-tertiary hover:text-text-secondary"
        >
          其他…
        </button>
      )}
    </div>
  );
}
