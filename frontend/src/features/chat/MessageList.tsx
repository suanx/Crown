import { useLayoutEffect, useRef, useState, useEffect } from "react";
import {
  useActiveThread,
  useActiveThreadPendingTurn,
} from "@/stores/chatStore";
import { useUiStore } from "@/stores/uiStore";
import { UserMessage } from "./UserMessage";
import { AssistantMessage } from "./AssistantMessage";
import { Spinner } from "@/shared/ui/Spinner";
import { ShimmerText } from "@/shared/ui/ShimmerText";
import { MessageScrubber } from "./MessageScrubber";
import { pickWorkingVerb } from "./workingVerbs";
import { useWorkingTimer } from "./useWorkingTimer";

/**
 * 消息列表 — 滚动严格限定容器内.
 *
 * 自动滚到底策略 (autoScroll toggle 默认 true):
 *
 *  1. **强制滚底**(绕过阈值):
 *     - 切换 thread 时 (threadId 变),从顶部跳回底部 (修 Bug B:打开历史
 *       会话停留在最上面)
 *     - turn 结束(streaming true → false)那一帧,因为 Incremark 把流式
 *       纯文本 reflow 成完整 markdown,DOM 高度突变 ~150-300px → 不补一刀
 *       下次发消息会落入 Bug A
 *
 *  2. **贴底跟随**(< 200px 时滚底):
 *     - 普通流式期间 messageCount / 末尾 message 长度变化时跟随
 *     - 阈值 200 比之前 100 宽容,容纳 Incremark reflow / 长 user message
 *       / 头像 + padding 等高度跳跃,避免"看着贴底实则差几像素就不跟了"
 *
 *  3. 用户主动往上滚 > 200px → 视为浏览历史,不抢
 *
 * 依赖列表:autoScroll / threadId / messageCount / 末尾 content/reasoning
 * 长度 / anyStreaming(任一 message isStreaming).每个变化都重评估.
 *
 * 间距档:
 *   - 横向 padding: 24 (px-6)
 *   - 上下 padding: 24 (py-6)
 *   - 消息间距: 24 (space-y-6) — 与外部 padding 同档,视觉节奏一致
 */
export function MessageList() {
  const thread = useActiveThread();
  const autoScroll = useUiStore((s) => s.autoScroll);
  const scrollRef = useRef<HTMLDivElement>(null);

  const threadId = thread?.id ?? null;
  const messageCount = thread?.messages.length ?? 0;
  const last = thread?.messages.at(-1);
  const lastContentLen = last?.content.length ?? 0;
  const lastReasoningLen = last?.reasoning?.length ?? 0;
  // 工具调用也要触发滚动:用 toolCalls 数组长度作为依赖
  const lastToolCallCount = last?.toolCalls?.length ?? 0;
  // 子代理活动 (P4): task 卡片的嵌套子代理面板出现 / 其工具数 / 产出文本
  // 增长时也要带动主列表跟随。面板自身固定高度内部滚动 (问题4)，这里只
  // 负责面板"出现"或绑定后整体高度变化时让主列表贴底。
  const lastSubAgentSignal =
    last?.toolCalls?.reduce((acc, tc) => {
      const sa = tc.subAgent;
      if (!sa) return acc;
      return acc + 1 + sa.toolCalls.length + (sa.text.length > 0 ? 1 : 0);
    }, 0) ?? 0;
  // 用 pendingTurn 替代 anyStreaming — 不受 rAF 合批延迟影响
  const pendingTurn = useActiveThreadPendingTurn();

  const prevThreadIdRef = useRef<string | null>(null);
  const wasPendingRef = useRef(false);
  const contentRef = useRef<HTMLDivElement>(null);
  // 用户"增长前"是否贴底。由滚动事件持续维护，ResizeObserver 在内容高度
  // 变化时读它来决定是否跟随——关键：必须读**增长发生之前**的状态，否则
  // 一张工具卡突然展开(+300px)会让回调里现算的 distanceFromBottom 超过阈值
  // 而误判"用户不在底部"，导致不跟随（之前的 bug）。
  const atBottomRef = useRef(true);

  // 维护 atBottomRef：用户每次滚动后记录当前是否在底部附近。
  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    const onScroll = () => {
      const dist =
        scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight;
      atBottomRef.current = dist < 120;
    };
    onScroll(); // 初始判定
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => scroller.removeEventListener("scroll", onScroll);
  }, [threadId]);

  // 内容高度变化自动贴底 —— ResizeObserver 捕获**任何**高度增长（工具卡
  // 展开/收起、result 填充、QuestionPanel、子代理面板、markdown reflow）。
  // 读"增长前"的 atBottomRef：增长前贴底 → 跟随到底；否则不打扰（用户在
  // 翻历史）。这修正了"大跳变被现算阈值挡掉"的问题。
  useEffect(() => {
    if (!autoScroll) return;
    const scroller = scrollRef.current;
    const content = contentRef.current;
    if (!scroller || !content) return;
    const ro = new ResizeObserver(() => {
      if (atBottomRef.current) {
        scroller.scrollTop = scroller.scrollHeight;
      }
    });
    ro.observe(content);
    return () => ro.disconnect();
  }, [autoScroll, threadId]);

  useLayoutEffect(() => {
    if (!autoScroll) return;
    const el = scrollRef.current;
    if (!el) return;

    const threadChanged = prevThreadIdRef.current !== threadId;
    // turn 结束边缘:pendingTurn true → false
    const turnEndEdge = wasPendingRef.current && !pendingTurn;

    prevThreadIdRef.current = threadId;
    wasPendingRef.current = pendingTurn;

    if (threadChanged || turnEndEdge) {
      el.scrollTo({ top: el.scrollHeight, behavior: threadChanged ? "auto" : "smooth" });
      return;
    }

    const distanceFromBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distanceFromBottom < 200) {
      el.scrollTop = el.scrollHeight;
    }
  }, [
    autoScroll,
    threadId,
    messageCount,
    lastContentLen,
    lastReasoningLen,
    lastToolCallCount,
    lastSubAgentSignal,
    pendingTurn,
  ]);

  if (!thread) return null;

  return (
    <div className="relative h-full">
      <div ref={scrollRef} className="h-full scrollable">
        <div ref={contentRef} className="max-w-[760px] mx-auto px-6 py-6 space-y-6">
          {renderMessages(thread)}

          {/* 工作中指示器 — 整个 turn 进行期间常驻底部（无论在吐字 / 调工具 /
              停顿），让用户始终知道 agent 在动还是卡住了。turn 结束
              (pendingTurn=false) 才消失。 */}
          {pendingTurn && (
            <ThinkingIndicator
              progressKey={
                lastContentLen +
                lastReasoningLen +
                lastToolCallCount +
                lastSubAgentSignal
              }
            />
          )}
        </div>
      </div>

      {/* 对话时间轴 Scrubber — 贴右边缘，悬停展开预览，磁吸放大 + 平滑跳转 */}
      <MessageScrubber messages={thread.messages} scrollRef={scrollRef} />
    </div>
  );
}

function renderMessages(
  thread: NonNullable<ReturnType<typeof useActiveThread>>,
) {
  const lastMainMessageId = [...thread.messages]
    .reverse()
    .find((m) => !m.brainstorm?.runId)?.id;

  return thread.messages.map((m) => {
    if (m.brainstorm?.runId) return null;

    if (m.role === "user") {
      return (
        <div key={m.id} data-msg-id={m.id}>
          <UserMessage message={m} />
        </div>
      );
    }
    if (m.role === "assistant") {
      return (
        <div key={m.id} data-msg-id={m.id}>
          <AssistantMessage
            message={m}
            providerId={thread.providerId}
            isActive={m.id === lastMainMessageId}
          />
        </div>
      );
    }
    return null;
  });
}

/**
 * 思考中指示器 —— 绿色转圈 + 扫光随机动词 + 跑秒计时。
 *
 * 让用户明确"没卡死、已经花了 Ns"。超过卡顿阈值（无新进展）文案转琥珀色
 * 并提示"仍在处理"。动词每隔几秒随机切换（含低概率"摸鱼中"彩蛋）。
 */
function ThinkingIndicator({ progressKey }: { progressKey: number }) {
  const { elapsedSec, stalled } = useWorkingTimer(true, progressKey);
  const [verb, setVerb] = useState(() => pickWorkingVerb());

  // 每 6 秒换一个动词，增加"活气"。
  useEffect(() => {
    const id = window.setInterval(() => setVerb(pickWorkingVerb()), 6000);
    return () => window.clearInterval(id);
  }, []);

  // 琥珀色（卡顿）保留转圈变色提示；正常进行时转圈用成功绿。
  const label = stalled ? "仍在处理" : verb;

  return (
    // 与 AssistantMessage 同结构对齐：头像位留空 (w-7) + gap-3，让文案左缘
    // 与上方助手消息正文严格对齐。
    <div className="flex gap-3 py-1" aria-live="polite">
      <div className="shrink-0 w-7" />
      <div className="flex items-center gap-2 min-w-0">
        <Spinner size={14} className={stalled ? "text-warning" : undefined} />
        {stalled ? (
          <span className="text-xs text-warning">{label}</span>
        ) : (
          // 白色系扫光：基色稍暗的白、高光纯白，一道光从左扫到右。
          <ShimmerText
            baseColor="rgba(255,255,255,0.55)"
            highlightColor="#ffffff"
            className="text-xs"
          >
            {label}
          </ShimmerText>
        )}
        {elapsedSec >= 1 && (
          <span className="text-xs text-text-tertiary tabular-nums">
            {elapsedSec}s
          </span>
        )}
      </div>
    </div>
  );
}
