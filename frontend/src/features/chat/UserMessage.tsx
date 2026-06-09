import type { Message } from "@/api";
import { useChatStore } from "@/stores/chatStore";
import { Icon } from "@/shared/icons/Icon";
import { AttachIcon, CopyIcon, EditIcon, RefreshIcon, SwapToRightIcon } from "@/shared/icons/set";

interface UserMessageProps {
  message: Message;
}

/**
 * Strip a leading `<system-reminder>...</system-reminder>` block (injected by
 * slash commands like /plan) so the user sees only their real task text.
 */
function stripReminderPrefix(content: string): string {
  const m = content.match(/^<system-reminder>[\s\S]*?<\/system-reminder>\s*/);
  return m ? content.slice(m[0].length) : content;
}

/**
 * 用户消息 — 右对齐 + DeepSeek 蓝气泡 + 白字.
 * 气泡内 padding: 上下 12, 左右 16.
 * action 按钮高度 28,与其他 IconButton 一致.
 */
export function UserMessage({ message }: UserMessageProps) {
  const rewind = useChatStore((s) => s.rewind);

  async function handleRewind() {
    if (message.seq == null) return;
    const ok = window.confirm(
      "回到这里？将删除此消息之后的所有对话，并还原那之后被工具改动的文件。此操作不可撤销。",
    );
    if (!ok) return;
    await rewind(message.threadId, message.seq);
  }

  return (
    <div className="group flex flex-col items-end gap-2" data-testid="user-message">
      <div
        className="max-w-[80%] rounded-2xl px-4 py-3 text-msg whitespace-pre-wrap break-words bg-brand text-white"
        style={{ borderTopRightRadius: "8px" }}
      >
        {message.attachments && message.attachments.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-1.5">
            {message.attachments.map((att, i) => {
              if (att.startsWith("data:image/")) {
                return (
                  <img
                    key={i}
                    src={att}
                    alt={`attached image ${i + 1}`}
                    className="max-w-[240px] max-h-[180px] rounded-lg object-cover border border-white/20"
                  />
                );
              }
              return (
                <span
                  key={i}
                  className="inline-flex items-center gap-1 rounded-md bg-white/15 px-2 py-0.5 text-[11px] text-white/90"
                >
                  <Icon icon={AttachIcon} size={10} />
                  <span className="truncate max-w-[140px]">{att}</span>
                </span>
              );
            })}
          </div>
        )}
        {stripReminderPrefix(message.content)}
      </div>
      <div className="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1">
        <ActionBtn icon={CopyIcon} label="复制" />
        <ActionBtn icon={EditIcon} label="编辑" />
        <ActionBtn icon={RefreshIcon} label="重发" />
        {message.seq != null && (
          <ActionBtn icon={SwapToRightIcon} label="回到这里" onClick={handleRewind} />
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
      aria-label={label}
      onClick={onClick}
      className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-secondary transition-colors focus-ring"
    >
      <Icon icon={icon} size={12} />
    </button>
  );
}
