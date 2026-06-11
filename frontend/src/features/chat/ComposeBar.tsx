import { useEffect, useRef, useState } from "react";
import { Icon } from "@/shared/icons/Icon";
import {
  AttachIcon,
  CloseIcon,
  SendIcon,
  StopIcon,
  CodeIcon,
  GlobeIcon,
  BrandIcon,
} from "@/shared/icons/set";
import { agentClient } from "@/api";
import { ComposerModelSelector } from "./ComposerModelSelector";
import { ComposerModeSelector } from "./ComposerModeSelector";
import { matchSlashCommand, applySlashCommand } from "./slashCommands";
import { SlashCommandMenu } from "./SlashCommandMenu";
import { useActiveThreadContextUsage } from "@/stores/chatStore";
import { cn } from "@/shared/lib/cn";
import { formatTokens } from "@/shared/lib/format";

export interface ComposeBarProps {
  autoFocus?: boolean;
  placeholder?: string;
  streaming?: boolean;
  onSend?: (text: string, attachments?: string[]) => void;
  onStop?: () => void;
}

/**
 * 输入区 — 严格对齐 Claude Code 节奏.
 *
 * 布局:
 *   ┌──────────────────────────────────────────────┐
 *   │  textarea ......                              │  (上半部分,12px 内 padding)
 *   ├──────────────────────────────────────────────┤
 *   │ [📎] [/] [🌐]                Model Mode  [↑]  │  (下半部分,28px 高 toolbar)
 *   └──────────────────────────────────────────────┘
 *
 * 圆角 16px,边框 + focus 高亮.
 * 高度档: input area 自适应 (1-8 行),底栏固定 28px + 8 padding = 44.
 */
export function ComposeBar({
  autoFocus,
  placeholder = "给 Agent 发条消息...",
  streaming = false,
  onSend,
  onStop,
}: ComposeBarProps) {
  const [text, setText] = useState("");
  const [menuIndex, setMenuIndex] = useState(0);
  const [files, setFiles] = useState<File[]>([]);
  const ref = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const menuCommands = matchSlashCommand(text);
  const menuOpen =
    text.startsWith("/") &&
    menuCommands.length > 0 &&
    !text.includes("\n") &&
    !text.trimStart().includes(" ");
  const safeMenuIndex =
    menuCommands.length > 0
      ? Math.min(menuIndex, menuCommands.length - 1)
      : 0;
  const [polishing, setPolishing] = useState(false);

  async function handlePolish() {
    const raw = text.trim();
    if (!raw || polishing) return;
    setPolishing(true);
    try {
      const polished = await agentClient.polishPrompt(raw);
      if (polished) setText(polished);
    } catch {
      // Silent fail — user can still send original text
    } finally {
      setPolishing(false);
      ref.current?.focus();
    }
  }


  useEffect(() => {
    if (autoFocus) ref.current?.focus();
  }, [autoFocus]);

  useEffect(() => {
    const ta = ref.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = Math.min(8 * 24, ta.scrollHeight) + "px";
  }, [text]);

  // Handle image paste
  useEffect(() => {
    const ta = ref.current;
    if (!ta) return;
    const handlePaste = async (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      const imageFiles: File[] = [];
      for (const item of Array.from(items)) {
        if (item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) imageFiles.push(file);
        }
      }
      if (imageFiles.length > 0) {
        e.preventDefault();
        setFiles((prev) => [...prev, ...imageFiles]);
      }
    };
    ta.addEventListener("paste", handlePaste);
    return () => ta.removeEventListener("paste", handlePaste);
  }, []);

  const canSend = (text.trim().length > 0 || files.length > 0) && !streaming;

  async function handleSend() {
    if (!canSend) return;
    const raw = text.trim();
    const transformed = applySlashCommand(raw);
    // Convert image files to data URIs, pass file names for text files
    const attachments: string[] = [];
    for (const f of files) {
      if (f.type.startsWith("image/")) {
        const b64 = await fileToBase64(f);
        attachments.push(`data:${f.type};base64,${b64}`);
      } else {
        attachments.push(f.name);
      }
    }
    onSend?.(transformed ?? raw, attachments.length > 0 ? attachments : undefined);
    setText("");
    setFiles([]);
    setMenuIndex(0);
  }

  function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        resolve(result.split(",")[1]);
      };
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
  }

  function handleAttachClick() {
    fileInputRef.current?.click();
  }

  function handleFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const selected = Array.from(e.target.files ?? []);
    setFiles((prev) => [...prev, ...selected]);
    e.target.value = "";
  }

  function removeFile(index: number) {
    setFiles((prev) => prev.filter((_, i) => i !== index));
  }

  function pickCommand(cmd: { name: string } | undefined) {
    if (!cmd) return;
    setText(`/${cmd.name} `);
    setMenuIndex(0);
    ref.current?.focus();
  }

  function handleKey(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (menuOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setMenuIndex((i) => (i + 1) % menuCommands.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setMenuIndex((i) => (i - 1 + menuCommands.length) % menuCommands.length);
        return;
      }
      if (e.key === "Tab") {
        e.preventDefault();
        pickCommand(menuCommands[safeMenuIndex]);
        return;
      }
      // Allow Ctrl/Cmd+arrow to close menu and navigate text
      if ((e.ctrlKey || e.metaKey) && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        setMenuIndex(0);
        // fall through to move cursor
      }
    }
    // Tab → 插入 2 个空格
    if (e.key === "Tab" && !menuOpen) {
      e.preventDefault();
      const ta = ref.current;
      if (!ta) return;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const newText = text.slice(0, start) + "  " + text.slice(end);
      setText(newText);
      requestAnimationFrame(() => {
        ta.selectionStart = ta.selectionEnd = start + 2;
      });
      return;
    }
    // Ctrl+方向键跳词 (Windows 原生支持,此处确保不被其他逻辑干扰)
    if ((e.ctrlKey || e.metaKey) && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      // 让浏览器的默认光标跳词行为继续
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  return (
    <div
      className={cn(
        "rounded-2xl bg-input-bg border border-border-subtle",
        "transition-colors focus-within:border-border-focus",
      )}
    >
      {menuOpen && (
        <div className="px-2 pt-2">
          <SlashCommandMenu
            commands={menuCommands}
            activeIndex={safeMenuIndex}
            onSelect={pickCommand}
          />
        </div>
      )}

      {/* Hidden file input */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        onChange={handleFileChange}
        className="hidden"
        tabIndex={-1}
      />

      {/* Attached file chips */}
      {files.length > 0 && (
        <div className="px-4 pt-2 flex flex-wrap gap-1.5">
          {files.map((file, index) => (
            <div
              key={index}
              className="flex items-center gap-1 rounded-md bg-elevated border border-border-subtle px-2 py-1 text-xs text-text-secondary max-w-[240px]"
            >
              {file.type.startsWith("image/") ? (
                <img
                  src={URL.createObjectURL(file)}
                  alt={file.name}
                  className="w-6 h-6 rounded object-cover shrink-0"
                />
              ) : (
                <Icon icon={AttachIcon} size={10} className="shrink-0" />
              )}
              <span className="truncate">{file.name}</span>
              <button
                onClick={() => removeFile(index)}
                className="shrink-0 ml-0.5 text-text-tertiary hover:text-text-primary transition-colors focus-ring rounded"
                aria-label={`移除 ${file.name}`}
              >
                <Icon icon={CloseIcon} size={10} />
              </button>
            </div>
          ))}
        </div>
      )}

      {/* 输入区 — 上 12 / 下 8 padding,横向 16 */}
      <div className="px-4 pt-3 pb-2">
        <textarea
          ref={ref}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKey}
          placeholder={placeholder}
          rows={1}
          data-testid="compose-input"
          className="scrollable w-full bg-transparent text-msg text-text-primary placeholder:text-text-tertiary resize-none outline-none"
        />
      </div>

      {/* 提示词模板快捷栏 */}
      {!text && !streaming && (
        <div className="flex items-center gap-1.5 px-4 pb-2 flex-wrap">
          <button
            onClick={() => { setText("/plan "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >📋 规划</button>
          <button
            onClick={() => { setText("/review "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >👁️ 审查</button>
          <button
            onClick={() => { setText("/test "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >🧪 测试</button>
          <button
            onClick={() => { setText("/explain "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >💡 解释</button>
          <button
            onClick={() => { setText("/simple "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >✂️ 极简</button>
          <button
            onClick={() => { setText("/safe "); ref.current?.focus(); }}
            className="text-[11px] h-6 px-2 rounded-md bg-elevated border border-border-subtle text-text-tertiary hover:text-text-secondary hover:border-border-default transition-colors shrink-0"
          >🛡️ 稳健</button>
        </div>
      )}

      {/* 工具栏 — 横向 padding 12,垂直 padding 8 */}
      <div className="flex items-center px-3 pb-2 gap-1">
        <ToolbarBtn icon={AttachIcon} label="附件" onClick={handleAttachClick} />
        <ToolbarBtn
          icon={CodeIcon}
          label="斜杠命令"
          onClick={() => {
            setText("/");
            setMenuIndex(0);
            ref.current?.focus();
          }}
        />
        <ToolbarBtn icon={GlobeIcon} label="联网搜索" />

        <div className="flex-1" />

        {/* 字数统计 — 超过 50 字符时显示 */}
        {text.length > 50 && (
          <span className="text-[10px] text-text-tertiary font-mono tabular-nums mr-1">
            {text.length}
          </span>
        )}

        {/* Context Usage 圆环 */}
        <ContextRing />

        {/* 模型 + 模式 — 替代原顶栏位置 */}
        <ComposerModelSelector />
        <ComposerModeSelector />

        {/* 优化提示词 */}
        <button
          onClick={handlePolish}
          disabled={!text.trim() || polishing}
          title={polishing ? "优化中..." : "优化提示词"}
          aria-label="优化提示词"
          className={cn(
            "h-7 w-7 rounded-md flex items-center justify-center transition-all focus-ring",
            polishing
              ? "animate-pulse-soft text-brand cursor-wait"
              : text.trim()
                ? "text-text-tertiary hover:text-brand hover:bg-brand-soft active:scale-95"
                : "text-text-disabled cursor-not-allowed",
          )}
        >
          <Icon icon={BrandIcon} size={14} weight={polishing ? "fill" : "duotone"} />
        </button>


        {/* 发送 / 停止 */}
        <div className="ml-1">
          {streaming ? (
            <button
              onClick={onStop}
              aria-label="停止"
              data-testid="compose-stop"
              className="h-7 w-7 rounded-md bg-danger text-white flex items-center justify-center hover:opacity-90 active:scale-95 transition-all focus-ring"
            >
              <Icon icon={StopIcon} size={14} weight="fill" />
            </button>
          ) : (
            <button
              onClick={handleSend}
              disabled={!canSend}
              aria-label="发送"
              data-testid="compose-send"
              className={cn(
                "h-7 w-7 rounded-md flex items-center justify-center transition-all focus-ring",
                canSend
                  ? "bg-brand text-white hover:bg-brand-hover active:scale-95"
                  : "bg-elevated text-text-tertiary cursor-not-allowed",
              )}
            >
              <Icon icon={SendIcon} size={14} weight="bold" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function ToolbarBtn({
  icon,
  label,
  onClick,
  disabled,
}: {
  icon: typeof AttachIcon;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "h-7 w-7 rounded-md text-text-tertiary transition-all flex items-center justify-center focus-ring",
        disabled
          ? "opacity-40 cursor-not-allowed"
          : "hover:bg-hover hover:text-text-secondary active:scale-95",
      )}
    >
      <Icon icon={icon} size={14} />
    </button>
  );
}

/**
 * Context Usage 圆环 — 20px SVG arc 显示上下文窗口用量.
 * <60% tertiary / 60-75% warning / >75% danger.
 * 无数据或 ratio=0 时隐藏.
 */
function ContextRing() {
  const usage = useActiveThreadContextUsage();
  if (!usage || usage.ratio <= 0) return null;

  const { ratio, usedTokens, maxTokens } = usage;
  const r = 8;
  const stroke = 2.5;
  const circumference = 2 * Math.PI * r;
  const offset = circumference * (1 - ratio);
  const color =
    ratio > 0.75
      ? "var(--ds-danger)"
      : ratio > 0.6
        ? "var(--ds-warning)"
        : "var(--ds-text-tertiary)";

  return (
    <div
      title={`${formatTokens(usedTokens)} / ${formatTokens(maxTokens)} tokens (${(ratio * 100).toFixed(0)}%)`}
      className="shrink-0 cursor-default"
    >
      <svg width={20} height={20}>
        <circle
          cx={10}
          cy={10}
          r={r}
          fill="none"
          stroke="var(--ds-border-subtle)"
          strokeWidth={stroke}
        />
        <circle
          cx={10}
          cy={10}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={stroke}
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          strokeLinecap="round"
          transform="rotate(-90 10 10)"
          className="transition-all duration-500 ease-out"
        />
      </svg>
    </div>
  );
}
