import { useState } from "react";
import { Icon } from "@/shared/icons/Icon";
import { CopyIcon, CheckIcon } from "@/shared/icons/set";

export interface CodeBlockProps {
  lang: string;
  code: string;
  /** 行号显示;diff 时不需要. */
  showLineNumbers?: boolean;
}

/**
 * 代码块 — 深色背景固定,即使在亮色主题也用深色
 * (规范文档明确要求,提升对比度).
 */
export function CodeBlock({
  lang,
  code,
  showLineNumbers = true,
}: CodeBlockProps) {
  const [copied, setCopied] = useState(false);
  const [failed, setFailed] = useState(false);
  const lines = code.split("\n");

  function handleCopy() {
    navigator.clipboard.writeText(code).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      },
      () => {
        // Clipboard denied (insecure context / no permission). Don't claim
        // success — surface a brief failure hint instead.
        setFailed(true);
        setTimeout(() => setFailed(false), 1200);
      },
    );
  }

  return (
    <div
      className="rounded-lg overflow-hidden border border-border-subtle"
      style={{ backgroundColor: "var(--ds-code-bg)" }}
    >
      <div
        className="flex items-center justify-between px-3 py-1.5 border-b text-xs font-mono"
        style={{
          color: "var(--ds-code-text)",
          borderColor: "rgba(255,255,255,0.08)",
        }}
      >
        <span className="opacity-60 lowercase">{lang || "text"}</span>
        <button
          onClick={handleCopy}
          className="inline-flex items-center gap-1 opacity-60 hover:opacity-100 transition-opacity focus-ring rounded px-1"
          aria-label="复制代码"
        >
          <Icon icon={copied ? CheckIcon : CopyIcon} size={12} />
          {copied ? "已复制" : failed ? "复制失败" : "复制"}
        </button>
      </div>
      <div
        className="overflow-x-auto"
        style={{ color: "var(--ds-code-text)" }}
      >
        <pre className="text-sm leading-[1.6] font-mono py-2.5">
          {lines.map((line, i) => (
            <div key={i} className="px-3 hover:bg-white/[0.02]">
              {showLineNumbers && (
                <span className="select-none inline-block w-7 mr-3 text-right opacity-30">
                  {i + 1}
                </span>
              )}
              <span>{line || "\u00A0"}</span>
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}
