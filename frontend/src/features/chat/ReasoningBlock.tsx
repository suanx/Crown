import { useEffect, useRef, useState } from "react";
import { Icon } from "@/shared/icons/Icon";
import { CaretRightIcon, ReasoningIcon } from "@/shared/icons/set";
import { ShimmerText } from "@/shared/ui/ShimmerText";
import { useSettingsStore } from "@/stores/settingsStore";

export interface ReasoningBlockProps {
  content: string;
  /** 整条消息是否仍在流式输出中. */
  streaming?: boolean;
}

/**
 * 思维链块 —— 去卡片化（对齐 Claude 桌面端）：折叠头一行（脑图标 + 文案 +
 * chevron），展开后左竖线 + 缩进文本，无背板。
 *
 * 展开逻辑：
 *   - 流式进行中：始终展开，实时显示思考内容（扫光"思考中…"）。
 *   - 流式刚结束：若设置开启"完成后折叠"，延迟 600ms 折叠（给用户看完）。
 *   - 用户手动开/关后：尊重用户选择，不再自动改。
 *   - 折叠态：头部显示内容预览（不是空白）。
 */
export function ReasoningBlock({
  content,
  streaming = false,
}: ReasoningBlockProps) {
  const collapseOnComplete = useSettingsStore(
    (s) => s.ui.collapseReasoningOnComplete,
  );

  const [open, setOpen] = useState(true);
  const userTouched = useRef(false);
  const wasStreaming = useRef(streaming);

  useEffect(() => {
    if (streaming && !userTouched.current) setOpen(true);
  }, [streaming]);

  useEffect(() => {
    const justFinished = wasStreaming.current && !streaming;
    wasStreaming.current = streaming;
    if (justFinished && collapseOnComplete && !userTouched.current) {
      const t = setTimeout(() => {
        if (!userTouched.current) setOpen(false);
      }, 600);
      return () => clearTimeout(t);
    }
  }, [streaming, collapseOnComplete]);

  function toggle() {
    userTouched.current = true;
    setOpen((v) => !v);
  }

  const hasContent = content.trim().length > 0;

  return (
    <div className="min-w-0">
      <button
        onClick={toggle}
        className="group/think w-full flex items-center gap-2 py-1 -mx-1.5 px-1.5 text-left text-text-secondary hover:text-text-primary hover:bg-hover active:scale-[0.99] transition-all focus-ring rounded-md"
      >
        <Icon icon={ReasoningIcon} size={14} weight="duotone" className="shrink-0 text-brand" />
        {streaming ? (
          <ShimmerText
            baseColor="rgba(255,255,255,0.55)"
            highlightColor="#ffffff"
            className="text-sm"
          >
            思考中…
          </ShimmerText>
        ) : (
          <span className="text-sm font-medium shrink-0">思维过程</span>
        )}
        {!open && !streaming && (
          <span className="text-text-tertiary text-xs truncate min-w-0">
            {preview(content)}
          </span>
        )}
        <Icon
          icon={CaretRightIcon}
          size={12}
          className={`ml-auto shrink-0 opacity-50 transition-transform duration-200 ${open ? "rotate-90" : ""}`}
        />
      </button>
      {open && hasContent && (
        <div className="pl-[6px] mt-1 mb-2">
          <div className="tool-rail pl-5 animate-slide-up">
            <pre className="whitespace-pre-wrap break-words font-sans text-sm leading-relaxed text-text-secondary">
              {content}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}

function preview(s: string): string {
  const single = s.replace(/\s+/g, " ").trim();
  return single.length > 60 ? single.slice(0, 60) + "..." : single;
}
