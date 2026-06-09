import { useMemo } from "react";
import {
  useActiveThread,
  useActiveThreadError,
  useActiveThreadLoading,
  useActiveThreadPendingTurn,
  useChatStore,
} from "@/stores/chatStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { Thread } from "@/api";
import { agentClient } from "@/api";
import { Icon } from "@/shared/icons/Icon";
import { SidebarIcon, TerminalIcon, DownloadIcon } from "@/shared/icons/set";
import { MessageList } from "./MessageList";
import { ComposeBar } from "./ComposeBar";
import { BrainstormDiscussion } from "./BrainstormDiscussion";
import { QuestionPanel } from "./QuestionPanel";
import { ApprovalPanel } from "./ApprovalPanel";
import { displayThreadTitle } from "@/shared/lib/threadTitle";

/** 取首条用户消息正文（剥掉斜杠命令注入的 system-reminder）作标题 fallback。 */
function firstUserPreview(thread: Thread): string | null {
  const first = thread.messages.find((m) => m.role === "user");
  if (!first) return null;
  return first.content
    .replace(/^<system-reminder>[\s\S]*?<\/system-reminder>\s*/i, "")
    .replace(/\s+/g, " ")
    .trim();
}

export interface ChatPageProps {
  threadId: string;
}

/**
 * 对话页 — 主区域,纵向 flex.
 *   ┌──────────────────────┐
 *   │ MessageList (flex-1, │
 *   │   独立滚动)            │
 *   ├──────────────────────┤
 *   │ ComposeBar (定高)     │
 *   └──────────────────────┘
 */
export function ChatPage({ threadId }: ChatPageProps) {
  const thread = useActiveThread();
  const loading = useActiveThreadLoading();
  const pendingTurn = useActiveThreadPendingTurn();
  const error = useActiveThreadError();
  const sendMessage = useChatStore((s) => s.sendMessage);
  const abortTurn = useChatStore((s) => s.abortTurn);
  const clearError = useChatStore((s) => s.clearError);
  const activeBrainstormRunId = useChatStore(
    (s) => s.activeBrainstormRunByThread[threadId],
  );
  const brainstormMeta = useChatStore((s) =>
    activeBrainstormRunId ? s.brainstormRunsById[activeBrainstormRunId] : undefined,
  );
  const toggleRightPanel = useWorkspaceStore((s) => s.toggleRightPanel);
  const toggleBottomPanel = useWorkspaceStore((s) => s.toggleBottomPanel);

  const streaming = pendingTurn;
  const activeBrainstormMessages = useMemo(
    () =>
      thread && activeBrainstormRunId
        ? thread.messages.filter(
            (message) => message.brainstorm?.runId === activeBrainstormRunId,
          )
        : [],
    [thread, activeBrainstormRunId],
  );

  async function handleExport() {
    if (!thread?.id) return;
    const md = await agentClient.exportThread(thread.id);
    const blob = new Blob([md], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `thread-${thread.id.slice(0, 8)}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }


  return (
    <div className="h-full flex flex-col relative">
      {/* 对话顶部栏 — 显示标题 + 右侧面板按钮 */}
      <div className="h-10 px-4 flex items-center shrink-0 relative z-10">
        <span className="text-sm text-text-secondary truncate flex-1">
          {thread ? displayThreadTitle(thread.title, firstUserPreview(thread)) : ""}
        </span>
        <div className="flex items-center gap-1 shrink-0 no-drag">
          <button
            onClick={handleExport}
            title="导出对话为 Markdown"
            className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors"
          >
            <Icon icon={DownloadIcon} size={14} />
          </button>
          <button
            onClick={toggleRightPanel}
            title="切换右侧面板 (Ctrl+Alt+B)"
            className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors"
          >
            <Icon icon={SidebarIcon} size={14} className="rotate-180" />
          </button>
          <button
            onClick={toggleBottomPanel}
            title="切换底部面板 (Ctrl+J)"
            className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors"
          >
            <Icon icon={TerminalIcon} size={14} />
          </button>
        </div>
      </div>

      {/* 顶部渐变遮罩 — 内容滚动时边缘虚化 */}
      <div className="absolute top-10 left-0 right-0 h-6 z-[5] pointer-events-none" style={{ background: "linear-gradient(to bottom, var(--ds-bg-canvas), transparent)" }} />

      {/* 错误条 — getThread/sendMessage/stream 失败时显示,可关闭 (P1-12) */}
      {error && (
        <div className="px-6 pt-1 shrink-0 no-drag">
          <div className="max-w-[760px] mx-auto flex items-start gap-2 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            <span className="flex-1 break-words">{error}</span>
            <button
              onClick={() => {
                if (thread) clearError(thread.id);
              }}
              title="关闭"
              className="shrink-0 rounded px-1.5 text-danger/70 hover:text-danger hover:bg-danger/15 transition-colors"
            >
              ✕
            </button>
          </div>
        </div>
      )}

      <div className="flex-1 min-h-0 relative">
        {loading && !thread ? (
          <div className="h-full flex items-center justify-center text-sm text-text-tertiary">
            加载中...
          </div>
        ) : (
          <MessageList />
        )}
      </div>

      {/* 底部渐变遮罩 — ComposeBar 上方虚化 */}
      <div className="h-6 shrink-0 pointer-events-none" style={{ background: "linear-gradient(to top, var(--ds-bg-canvas), transparent)" }} />

      <div className="px-6 pb-6 pt-0 shrink-0 bg-canvas">
        <div className="max-w-[920px] mx-auto">
          {activeBrainstormRunId && (
            <BrainstormDiscussion
              runId={activeBrainstormRunId}
              topic={brainstormMeta?.topic}
              participants={brainstormMeta?.participants}
              status={brainstormMeta?.status}
              messages={activeBrainstormMessages}
            />
          )}
          {/* 审批面板 — 紧贴输入框上沿浮出，无待审批时不渲染 */}
          <ApprovalPanel threadId={threadId} />
          {/* 结构化问答面板 — 紧贴输入框上沿浮出，无 pending 时不渲染 */}
          <QuestionPanel threadId={threadId} />
          <ComposeBar
            streaming={streaming}
            onSend={(text, attachments) => void sendMessage(threadId, text, attachments)}
            onStop={() => void abortTurn(threadId)}
          />
        </div>
      </div>
    </div>
  );
}
