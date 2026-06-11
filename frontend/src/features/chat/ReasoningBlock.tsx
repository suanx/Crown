import { useEffect, useRef, useState } from "react";
import { Icon } from "@/shared/icons/Icon";
import { CaretRightIcon, ReasoningIcon } from "@/shared/icons/set";

export interface ReasoningBlockProps {
  content: string;
  streaming?: boolean;
}

export function ReasoningBlock({
  content,
  streaming = false,
}: ReasoningBlockProps) {
  const [open, setOpen] = useState(true);
  const userTouched = useRef(false);
  const hasContent = content.trim().length > 0;

  useEffect(() => {
    if (streaming && !userTouched.current) setOpen(true);
  }, [streaming]);

  function toggle() {
    userTouched.current = true;
    setOpen((v) => !v);
  }

  return (
    <div className="min-w-0">
      <button
        onClick={toggle}
        className="group/think w-full flex items-center gap-2 py-1 -mx-1.5 px-1.5 text-left text-text-secondary hover:text-text-primary hover:bg-hover active:scale-[0.99] transition-all focus-ring rounded-md"
      >
        <Icon icon={ReasoningIcon} size={14} weight="duotone" className="shrink-0 text-brand" />
        {streaming ? (
          <span className="text-sm font-medium text-brand shrink-0">
            {hasContent ? "思考中…" : "正在思考..."}
          </span>
        ) : (
          <span className="text-sm font-medium shrink-0">思维过程</span>
        )}
        {!open && hasContent && (
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
      {open && (
        <div className="pl-[6px] mt-1 mb-2">
          <div className="tool-rail pl-5 animate-slide-up">
            {streaming && !hasContent ? (
              <div className="flex items-center gap-2 py-2">
                <div className="flex gap-1">
                  <span className="w-1.5 h-1.5 rounded-full bg-brand animate-bounce" style={{ animationDelay: "0ms" }} />
                  <span className="w-1.5 h-1.5 rounded-full bg-brand animate-bounce" style={{ animationDelay: "150ms" }} />
                  <span className="w-1.5 h-1.5 rounded-full bg-brand animate-bounce" style={{ animationDelay: "300ms" }} />
                </div>
                <span className="text-xs text-text-tertiary">思考中...</span>
              </div>
            ) : (
              <pre className="whitespace-pre-wrap break-words font-sans text-sm leading-relaxed text-text-secondary">
                {content}
              </pre>
            )}
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
