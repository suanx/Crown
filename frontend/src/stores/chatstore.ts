/**
 * ============================================================================
 * chatStore — 流式对话状态机 (per-thread cache)
 * ============================================================================
 *
 * 设计原则:
 *
 *  1. **不持有 activeThreadId**.路由由 routerStore 管,本 store 仅按 threadId
 *     索引数据.避免两个 store 互相依赖 + 切 thread 时不会丢失 A 的流式中间
 *     状态(报告 §16 修复方向).
 *
 *  2. **增量 reducer 是纯逻辑**.所有 apply* action 接收 IPC event payload,
 *     只动 state.不调 client.事件订阅在 useTauriEvents hook 里集中 wire.
 *
 *  3. **command actions 是 thin wrapper**.sendMessage/abortTurn/approveTool
 *     只调 client + 局部 optimistic update,turn 推进完全靠事件回流.
 *
 *  4. **delta 自创建消息**.若 onContentDelta / onToolCallStart 收到的
 *     messageId 不在 messages 里,自动追加一条空 assistant message — 后端
 *     不必先 emit "message_start" 事件.
 *
 * 旧用法迁移:
 *
 *   useChatStore(s => s.thread)        →  useActiveThread()
 *   useChatStore(s => s.loading)       →  useActiveThreadLoading()
 *   useChatStore(s => s.loadThread)    →  保留(cache miss 时拉)
 *   <ModeSelector loadThread 重拉>      →  useChatStore(s => s.reloadThread)
 * ----------------------------------------------------------------------------
 */

import { create } from "zustand";
import { agentClient } from "@/api";
import type {
  ApprovalRequestEvent,
  ApproveToolDecision,
  AnswerItemDto,
  BrainstormAgentStatusEvent,
  BrainstormErrorEvent,
  BrainstormMessageDeltaEvent,
  BrainstormMessageDoneEvent,
  BrainstormMessageStartEvent,
  BrainstormReasoningDeltaEvent,
  BrainstormParticipant,
  BrainstormRunDoneEvent,
  BrainstormRunStartedEvent,
  BrainstormToolCallStartEvent,
  BrainstormToolCallUpdateEvent,
  ContentDeltaEvent,
  Message,
  QuestionRequestEvent,
  ReasoningDeltaEvent,
  Segment,
  StreamAbortedEvent,
  StreamErrorEvent,
  SubAgentActivity,
  Thread,
  ToolCall,
  ToolCallStartEvent,
  ToolCallUpdateEvent,
  ToolSegment,
  TurnCompleteEvent,
  TodoItem,
  TodosUpdatedEvent,
  ContextUsageEvent,
} from "@/api";
import { useRouterStore } from "@/stores/routerStore";
import { useBalanceStore } from "@/stores/balanceStore";
import { useSessionStore } from "@/stores/sessionStore";
import { isDefaultTitle } from "@/shared/lib/threadTitle";

interface ChatState {
  /** Per-thread 缓存.切换 thread 时不丢失流式中间状态. */
  threadsById: Record<string, Thread>;
  /** 单 thread 的 getThread 加载态. */
  loading: Record<string, boolean>;
  /** 单 thread 的 getThread 错误. */
  errors: Record<string, string | null>;
  /**
   * 单 thread 是否处于 "已 sendMessage 但 turn 未完成" 状态.
   * 解决报告 §17 — server snapshot.isStreaming 在第一个 delta 到达前
   * 永远 false 的窗口期问题.
   */
  pendingTurnByThread: Record<string, boolean>;
  /**
   * 待处理审批队列.ApprovalDialog (阶段二) 取首项渲染,处理后调
   * consumeNextApproval shift 出队.
   */
  pendingApprovals: ApprovalRequestEvent[];
  /**
   * 待处理结构化问答队列.QuestionPanel 取首项渲染,提交/取消后出队.
   * 仿 pendingApprovals.
   */
  pendingQuestions: QuestionRequestEvent[];
  /**
   * Per-thread 任务列表 (todo_write 产物).整列表全量替换,内存缓存不落库.
   */
  todosByThread: Record<string, TodoItem[]>;
  /**
   * Per-thread 上下文用量 (stream:context_usage).驱动 ComposeBar 圆环.
   */
  contextUsageByThread: Record<string, { usedTokens: number; maxTokens: number; ratio: number }>;
  /**
   * Per-thread 当前 turn 的起始时间戳 (Date.now()).sendMessage 时记录,
   * turn_complete 时用于算 wall-clock 耗时.仅内存,不落库.
   */
  turnStartByThread: Record<string, number>;
  /**
   * Per-message turn 耗时 (ms).turn_complete 时按 messageId 写入,
   * MessageMeta 显示 `Ns`.独立于 message 对象存放,使 reloadThread 重拉
   * 快照不会冲掉它 (耗时是前端 wall-clock 观测值,后端快照里没有).
   */
  turnDurationByMsg: Record<string, number>;
  /** 当前 thread 展示中的 brainstorm run id，完成后也保留给用户展开查看。 */
  activeBrainstormRunByThread: Record<string, string>;
  /** 多 Agent 讨论元数据，用于过程面板提前显示全员头像。 */
  brainstormRunsById: Record<
    string,
    {
      threadId: string;
      topic: string;
      participants: BrainstormParticipant[];
      status: "running" | "done" | "error";
    }
  >;

  // ── command actions (UI 触发) ─────────────────────────────────────────
  /** Cache miss 时拉,已有缓存直接返回.切 thread 用. */
  loadThread: (id: string) => Promise<void>;
  /** 强制重拉(updateThread / switchModel 等改动后用). */
  reloadThread: (id: string) => Promise<void>;
  /** 发消息:本地插入 user message + 调后端,turn 推进靠事件. */
  sendMessage: (threadId: string, content: string) => Promise<void>;
  /** 中止当前 turn(也用于 ApprovalDialog Esc / ToolCallCard Esc). */
  abortTurn: (threadId: string) => Promise<void>;
  /** 回溯到某条用户消息:截断对话 + 还原文件 (P2). */
  rewind: (threadId: string, messageSeq: number) => Promise<void>;
  /** 审批决策(配合首项 pendingApproval). */
  approveTool: (
    threadId: string,
    toolUseId: string,
    decision: ApproveToolDecision,
  ) => Promise<void>;
  /** 提交结构化问答答案(配合首项 pendingQuestion).取消时 cancelled=true. */
  submitAnswers: (
    threadId: string,
    toolUseId: string,
    answers: AnswerItemDto[],
    cancelled: boolean,
  ) => Promise<void>;
  /**
   * 从所有 per-thread 缓存中清除一个 thread.
   * 删除会话后调用,防止 threadsById / todos / context / pendingApprovals
   * 等无限累积 (内存泄漏).
   */
  dropThread: (threadId: string) => void;

  /** 清除某 thread 的错误（用户关闭错误条 / 重试后）。 */
  clearError: (threadId: string) => void;

  // ── event reducers (useTauriEvents 触发) ────────────────────────────
  applyContentDelta: (e: ContentDeltaEvent) => void;
  applyReasoningDelta: (e: ReasoningDeltaEvent) => void;
  applyToolCallStart: (e: ToolCallStartEvent) => void;
  applyToolCallUpdate: (e: ToolCallUpdateEvent) => void;
  applyTurnComplete: (e: TurnCompleteEvent) => void;
  applyApprovalRequest: (e: ApprovalRequestEvent) => void;
  /** question:request 事件入队. */
  applyQuestionRequest: (e: QuestionRequestEvent) => void;
  applyStreamError: (e: StreamErrorEvent) => void;
  applyStreamAborted: (e: StreamAbortedEvent) => void;
  applyTodosUpdated: (e: TodosUpdatedEvent) => void;
  applyContextUsage: (e: ContextUsageEvent) => void;
  applyBrainstormRunStarted: (e: BrainstormRunStartedEvent) => void;
  applyBrainstormAgentStatus: (e: BrainstormAgentStatusEvent) => void;
  applyBrainstormMessageStart: (e: BrainstormMessageStartEvent) => void;
  applyBrainstormMessageDelta: (e: BrainstormMessageDeltaEvent) => void;
  applyBrainstormReasoningDelta: (e: BrainstormReasoningDeltaEvent) => void;
  applyBrainstormToolCallStart: (e: BrainstormToolCallStartEvent) => void;
  applyBrainstormToolCallUpdate: (e: BrainstormToolCallUpdateEvent) => void;
  applyBrainstormMessageDone: (e: BrainstormMessageDoneEvent) => void;
  applyBrainstormRunDone: (e: BrainstormRunDoneEvent) => void;
  applyBrainstormError: (e: BrainstormErrorEvent) => void;
  /** ApprovalDialog 处理后 shift 出队. */
  consumeApproval: (toolUseId: string) => void;
  /** QuestionPanel 提交/取消后出队. */
  consumeQuestion: (toolUseId: string) => void;
}

// ── reducer 工具 ─────────────────────────────────────────────────────────

/**
 * 把后端 getThread 返回的旧格式消息 (content/reasoning/toolCalls) 转为 segments.
 * 旧对话历史无法还原交错顺序,按 reasoning → content → toolCalls 固定排列.
 */
function legacyToSegments(msg: Message): Segment[] {
  // 已经有 segments 且非空时,跳过转换
  if (msg.segments && msg.segments.length > 0) return msg.segments;
  const segs: Segment[] = [];
  if (msg.reasoning) {
    segs.push({ kind: "reasoning", text: msg.reasoning });
  }
  if (msg.content) {
    segs.push({ kind: "text", text: msg.content });
  }
  if (msg.toolCalls) {
    for (const tc of msg.toolCalls) {
      segs.push({
        kind: "tool",
        callId: tc.id,
        name: tc.name,
        input: tc.input,
        status: tc.status,
        result: tc.result ?? undefined,
        durationMs: tc.durationMs ?? undefined,
        diff: tc.diff,
        errorMessage: tc.errorMessage ?? undefined,
        subAgent: tc.subAgent ?? null,
      });
    }
  }
  return segs;
}

/**
 * 对 Thread 的所有消息填充 segments (兼容后端返回旧格式).
 */
function hydrateThreadSegments(t: Thread): Thread {
  return {
    ...t,
    messages: t.messages.map((m) => ({
      ...m,
      segments: legacyToSegments(m),
    })),
  };
}

/**
 * 从用户首条消息自动生成标题 (Claude 策略).
 *
 * Claude 实际行为: 第一轮 turn_complete 后,用 LLM 生成 ≤60 字符摘要.
 * 我们的原型策略: 截取用户首条消息前 50 字符,在词/句边界断.
 * 后端实装后改为后端生成 (Rust engine turn_complete handler 内部调 LLM
 * 生成 title 然后 emit 一个 thread_title_updated event).
 */
function generateTitleFromFirstMessage(thread: Thread): string | null {
  // 仅在标题为默认值时才生成
  const defaultTitles = ["New Chat", "new chat", "新对话", "新建对话", ""];
  if (!defaultTitles.includes(thread.title.trim())) return null;

  // 找第一条 user 消息
  const firstUser = thread.messages.find((m) => m.role === "user");
  if (!firstUser || !firstUser.content.trim()) return null;

  const raw = firstUser.content.trim();
  // Slice by Unicode code points (not UTF-16 code units) so emoji / astral
  // characters at the boundary aren't split into broken surrogate halves.
  const cps = Array.from(raw);
  if (cps.length <= 50) return raw;
  const cut = cps.slice(0, 50).join("");
  const lastSpace = cut.lastIndexOf(" ");
  const lastPunct = Math.max(
    cut.lastIndexOf("，"),
    cut.lastIndexOf("。"),
    cut.lastIndexOf(","),
    cut.lastIndexOf("."),
    cut.lastIndexOf("、"),
  );
  const breakAt = Math.max(lastSpace, lastPunct);
  if (breakAt > 20) return cut.slice(0, breakAt) + "…";
  return cut + "…";
}

function generateTitleFromPrompt(prompt: string): string {
  const firstLine = prompt.trim().split(/\r?\n/)[0] ?? "";
  const cps = Array.from(firstLine);
  if (cps.length <= 50) return firstLine;
  return cps.slice(0, 50).join("") + "…";
}

function emptyAssistantMessage(threadId: string, messageId: string): Message {
  return {
    id: messageId,
    threadId,
    role: "assistant",
    content: "",
    timestamp: new Date().toISOString(),
    reasoning: null,
    toolCalls: null,
    segments: [],
    usage: null,
    isStreaming: true,
    interrupted: false,
  };
}

function localUserMessage(threadId: string, content: string, attachments?: string[]): Message {
  return {
    id: `local-user-${Date.now()}`,
    threadId,
    role: "user",
    content,
    timestamp: new Date().toISOString(),
    reasoning: null,
    toolCalls: null,
    segments: [{ kind: "text", text: content }],
    usage: null,
    isStreaming: false,
    interrupted: false,
    attachments,
  };
}

function localBrainstormMessage(e: BrainstormMessageStartEvent): Message {
  return {
    id: e.messageId,
    threadId: e.threadId,
    role: "assistant",
    content: "",
    timestamp: new Date().toISOString(),
    reasoning: null,
    toolCalls: null,
    segments: [{ kind: "text", text: "" }],
    usage: null,
    isStreaming: true,
    interrupted: false,
    brainstorm: {
      runId: e.runId,
      messageId: e.messageId,
      participant: e.participant,
    },
  };
}

function parseBrainstorm(input: string): string | null {
  const match = input.match(/^\/brainstorm(?:\s+([\s\S]*))?$/i);
  if (!match) return null;
  return (match[1] ?? "").trim();
}

function mergeToolInput(
  current: Record<string, unknown>,
  incoming: Record<string, unknown> | null,
): Record<string, unknown> {
  return incoming ? { ...current, ...incoming } : current;
}

function reconcileBrainstormSummary(
  messages: Message[],
  e: BrainstormRunDoneEvent,
): Message[] {
  const streamedIdPrefix = `brainstorm-${e.runId}-`;
  let replaced = false;
  const next = messages.map((m) => {
    const isStreamedFinal =
      !m.brainstorm?.runId &&
      m.role === "assistant" &&
      m.id.startsWith(streamedIdPrefix);
    if (!isStreamedFinal) return m;
    replaced = true;
    return {
      ...m,
      content: e.artifact,
      segments: [{ kind: "text" as const, text: e.artifact }],
      isStreaming: false,
    };
  });
  if (replaced) return next;
  if (
    next.some(
      (m) =>
        !m.brainstorm?.runId &&
        m.role === "assistant" &&
        m.content.trim() === e.artifact.trim(),
    )
  ) {
    return next;
  }
  return [
    ...next,
    {
      id: `brainstorm-summary-${e.runId}`,
      threadId: e.threadId,
      role: "assistant",
      content: e.artifact,
      timestamp: new Date().toISOString(),
      reasoning: null,
      toolCalls: null,
      segments: [{ kind: "text", text: e.artifact }],
      usage: null,
      isStreaming: false,
      interrupted: false,
    },
  ];
}

/**
 * 在指定 thread 内 upsert 一条 message.若 messageId 不在,append 一条空
 * assistant 后再 patch.返回新的 thread 引用(immutable).
 */
function upsertMessage(
  thread: Thread,
  messageId: string,
  patch: (m: Message) => Message,
): Thread {
  const idx = thread.messages.findIndex((m) => m.id === messageId);
  let messages: Message[];
  if (idx === -1) {
    const seed = emptyAssistantMessage(thread.id, messageId);
    messages = [...thread.messages, patch(seed)];
  } else {
    // 引用稳定优化：只 patch 目标 message，其他消息保持引用不变，
    // 让 AssistantMessage(memo) 能真正短路非活跃消息的重渲染。
    const patched = patch(thread.messages[idx]);
    if (patched === thread.messages[idx]) return thread;
    messages = thread.messages.map((m, i) => (i === idx ? patched : m));
  }
  return { ...thread, messages };
}

/**
 * 把一个子代理 (agentId) 的活动更新到拥有它的 task 卡片上 (P4)。
 *
 * 由于子代理串行运行 (task 工具 is_parallel_safe=false)，同一时刻父对话里
 * 至多一个 task 卡片处于 running。绑定规则：找到第一个 subAgent 已绑定到此
 * agentId 的 task 卡片；否则找到第一个还没绑定 subAgent 的 running task 卡片，
 * 绑定它。在 messages 与 segments(toolCalls 同步) 两处都更新该卡片。
 */
function updateSubAgent(
  thread: Thread,
  agentId: string,
  patch: (sa: SubAgentActivity) => SubAgentActivity,
): Thread {
  const empty: SubAgentActivity = { agentId, text: "", toolCalls: [] };
  const isOwner = (tc: ToolCall) =>
    tc.name === "task" && tc.subAgent?.agentId === agentId;
  const isFree = (tc: ToolCall) =>
    tc.name === "task" &&
    (tc.subAgent == null) &&
    (tc.status === "running" || tc.status === "pending_approval");

  // Decide which task callId to bind, scanning newest message first.
  let targetCallId: string | null = null;
  for (let i = thread.messages.length - 1; i >= 0 && !targetCallId; i--) {
    const tcs = thread.messages[i].toolCalls ?? [];
    const owner = tcs.find(isOwner);
    if (owner) {
      targetCallId = owner.id;
      break;
    }
    const free = tcs.find(isFree);
    if (free) targetCallId = free.id;
  }
  if (!targetCallId) return thread;

  const messages = thread.messages.map((m) => {
    const tcs = m.toolCalls;
    const hasInToolCalls = tcs?.some((tc) => tc.id === targetCallId);
    const hasInSegments = m.segments.some(
      (seg) => seg.kind === "tool" && seg.callId === targetCallId,
    );
    if (!hasInToolCalls && !hasInSegments) return m;
    return {
      ...m,
      toolCalls:
        tcs?.map((tc) =>
          tc.id === targetCallId
            ? { ...tc, subAgent: patch(tc.subAgent ?? empty) }
            : tc,
        ) ?? null,
      segments: m.segments.map((seg) =>
        seg.kind === "tool" && seg.callId === targetCallId
          ? { ...seg, subAgent: patch(seg.subAgent ?? empty) }
          : seg,
      ),
    };
  });
  return { ...thread, messages };
}

// ── store ────────────────────────────────────────────────────────────────

export const useChatStore = create<ChatState>((set, get) => ({
  threadsById: {},
  loading: {},
  errors: {},
  pendingTurnByThread: {},
  pendingApprovals: [],
  pendingQuestions: [],
  todosByThread: {},
  contextUsageByThread: {},
  turnStartByThread: {},
  turnDurationByMsg: {},
  activeBrainstormRunByThread: {},
  brainstormRunsById: {},

  loadThread: async (id) => {
    if (get().threadsById[id]) return;
    await get().reloadThread(id);
  },

  reloadThread: async (id) => {
    set((s) => ({
      loading: { ...s.loading, [id]: true },
      errors: { ...s.errors, [id]: null },
    }));
    try {
      const t = await agentClient.getThread(id);
      const incoming = hydrateThreadSegments(t);
      set((s) => {
        const cached = s.threadsById[id];
        // Guard against a stale `getThread` snapshot clobbering optimistic /
        // streaming state. This happens on the welcome-page first-send race:
        // App.tsx's route effect fires `loadThread(id)` (→ getThread) the
        // moment we navigate to the new (empty) thread, while WelcomePage
        // concurrently appends the optimistic user message + starts the turn.
        // If the route-effect's getThread resolves *after* the optimistic
        // write, it would wipe the user message (observed: thread left with
        // only the streaming assistant). A snapshot is stale when:
        //   1. the thread is mid-turn (pendingTurn) — streaming events are
        //      the source of truth until turn_complete, or
        //   2. the cached thread already has *more* messages than the
        //      snapshot — optimistic/streaming messages grow the cache ahead
        //      of the last-persisted snapshot.
        // Rewind uses its own path (sets threadsById directly), so legitimate
        // truncations are unaffected.
        const isStale =
          !!cached &&
          (s.pendingTurnByThread[id] === true ||
            cached.messages.length > incoming.messages.length);
        if (isStale) {
          return { loading: { ...s.loading, [id]: false } };
        }
        // Preserve an optimistic non-default title: turn_complete sets the
        // auto-title locally and persists it async via updateThread; a
        // concurrent getThread may read the row *before* that persists and
        // would otherwise revert the title to the default. Keep the local
        // title when the incoming snapshot still has a default one.
        const DEFAULTS = ["New Chat", "new chat", "新对话", "新建对话", ""];

        // ── id 体系不一致：流式消息 id 是后端引擎 ULID，而 getThread 持久化
        // 快照的消息 id 是 SQLite 自增整数 (m.id.to_string() → "42")。两者
        // 永不相等。turn_complete 后 reloadThread (本意只为回填 seq) 若整体
        // 用快照替换缓存，会把 ULID 换成数字、usage 抹成 null、subAgent 丢失
        // → MessageMeta 一闪而过。
        //
        // 修法：缓存已存在且消息条数一致时，**保留缓存的全部消息**(ULID id /
        // usage / subAgent / segments 原样不动)，只按位置把后端的 seq 回填上
        // 去 (回溯按钮唯一需要的字段)。仅在无缓存 (首次加载 / 切到历史会话)
        // 时才整体采用快照。
        if (
          cached &&
          cached.messages.length === incoming.messages.length &&
          cached.messages.length > 0
        ) {
          const keptTitle =
            !DEFAULTS.includes(cached.title.trim()) &&
            DEFAULTS.includes(incoming.title.trim())
              ? cached.title
              : incoming.title;
          const messages = cached.messages.map((m, i) => {
            const seq = incoming.messages[i]?.seq;
            return seq != null && m.seq !== seq ? { ...m, seq } : m;
          });
          return {
            threadsById: {
              ...s.threadsById,
              [id]: { ...incoming, title: keptTitle, messages },
            },
            loading: { ...s.loading, [id]: false },
          };
        }

        const titled =
          cached &&
          !DEFAULTS.includes(cached.title.trim()) &&
          DEFAULTS.includes(incoming.title.trim())
            ? { ...incoming, title: cached.title }
            : incoming;
        return {
          threadsById: { ...s.threadsById, [id]: titled },
          loading: { ...s.loading, [id]: false },
        };
      });
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, [id]: false },
        errors: {
          ...s.errors,
          [id]: err instanceof Error ? err.message : String(err),
        },
      }));
    }
  },

  sendMessage: async (threadId: string, content: string, attachments?: string[]) => {
    const trimmed = content.trim();
    if (!trimmed) return;
    const currentSummary = useSessionStore
      .getState()
      .threads.find((thread) => thread.id === threadId);
    const shouldNameFromPrompt =
      !!currentSummary && isDefaultTitle(currentSummary.title);
    const promptTitle = shouldNameFromPrompt
      ? generateTitleFromPrompt(trimmed)
      : null;
    set((s) => {
      const thread = s.threadsById[threadId];
      if (!thread) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [threadId]: {
            messages: [...thread.messages, localUserMessage(threadId, trimmed, attachments)],
          },
        },
        pendingTurnByThread: { ...s.pendingTurnByThread, [threadId]: true },
        turnStartByThread: { ...s.turnStartByThread, [threadId]: Date.now() },
      };
    });
    if (promptTitle) {
      useSessionStore.setState((s) => ({
        threads: s.threads.map((thread) =>
          thread.id === threadId
            ? { ...thread, title: promptTitle, preview: trimmed }
            : thread,
        ),
      }));
      void agentClient
        .updateThread({ threadId, title: promptTitle })
        .then(() => useSessionStore.getState().loadThreads())
        .catch(() => {
          // 标题只是展示增强，失败时保留本地乐观标题。
        });
    }
    try {
      const brainstormTopic = parseBrainstorm(trimmed);
      if (brainstormTopic !== null) {
        if (!brainstormTopic) {
          throw new Error("/brainstorm 后面需要跟提示词");
        }
        await agentClient.startBrainstorm({
          threadId,
          topic: brainstormTopic,
          rounds: 1,
        });
      } else {
      } else {
        await agentClient.sendMessage({
          threadId,
          content: trimmed,
          attachments: attachments ?? [],
        });
      }
      }
    } catch (err) {
      // sendMessage failure — 关掉 pending,把错误塞到一条本地 system msg
      const msg = err instanceof Error ? err.message : String(err);
      set((s) => {
        const thread = s.threadsById[threadId];
        if (!thread) return s;
        return {
          pendingTurnByThread: { ...s.pendingTurnByThread, [threadId]: false },
          errors: { ...s.errors, [threadId]: msg },
        };
      });
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[chatStore] sendMessage failed:", msg);
      }
    }
  },

  abortTurn: async (threadId) => {
    try {
      const brainstormRunId = get().activeBrainstormRunByThread[threadId];
      const brainstormRun = brainstormRunId
        ? get().brainstormRunsById[brainstormRunId]
        : null;
      if (brainstormRunId && brainstormRun?.status === "running") {
        await agentClient.stopBrainstorm(brainstormRunId);
        set((s) => {
          const activeBrainstormRunByThread = { ...s.activeBrainstormRunByThread };
          delete activeBrainstormRunByThread[threadId];
          return {
            activeBrainstormRunByThread,
            pendingTurnByThread: { ...s.pendingTurnByThread, [threadId]: false },
          };
        });
        return;
      }
      await agentClient.abortTurn(threadId);
    } finally {
      // pendingTurn 等 stream:aborted 事件来正式关,这里不抢
    }
  },

  rewind: async (threadId, messageSeq) => {
    const t = await agentClient.rewindThread(threadId, messageSeq);
    set((s) => ({
      threadsById: { ...s.threadsById, [threadId]: hydrateThreadSegments(t) },
    }));
  },

  approveTool: async (threadId, toolUseId, decision) => {
    try {
      await agentClient.approveTool({ threadId, toolUseId, decision });
      get().consumeApproval(toolUseId);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      set((s) => ({ errors: { ...s.errors, [threadId]: msg } }));
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[chatStore] approveTool failed:", msg);
      }
    }
  },

  submitAnswers: async (_threadId, toolUseId, answers, cancelled) => {
    // 即时出队,UI 立刻收起问答面板;失败由 HybridClient fallback 兜底
    get().consumeQuestion(toolUseId);
    try {
      await agentClient.submitAnswers({ toolUseId, cancelled, answers });
    } catch (err) {
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[chatStore] submitAnswers failed:", err);
      }
    }
  },

  dropThread: (threadId) =>
    set((s) => {
      // 从所有 Record<threadId, _> 缓存里删 key + 过滤 pendingApprovals.
      const omit = <T>(rec: Record<string, T>): Record<string, T> => {
        if (!(threadId in rec)) return rec;
        const next = { ...rec };
        delete next[threadId];
        return next;
      };
      return {
        threadsById: omit(s.threadsById),
        loading: omit(s.loading),
        errors: omit(s.errors),
        pendingTurnByThread: omit(s.pendingTurnByThread),
        todosByThread: omit(s.todosByThread),
        contextUsageByThread: omit(s.contextUsageByThread),
        activeBrainstormRunByThread: omit(s.activeBrainstormRunByThread),
        brainstormRunsById: Object.fromEntries(
          Object.entries(s.brainstormRunsById).filter(
            ([, run]) => run.threadId !== threadId,
          ),
        ),
        turnStartByThread: omit(s.turnStartByThread),
        pendingApprovals: s.pendingApprovals.filter(
          (a) => a.threadId !== threadId,
        ),
        pendingQuestions: s.pendingQuestions.filter(
          (q) => q.threadId !== threadId,
        ),
      };
    }),

  clearError: (threadId) =>
    set((s) => {
      if (!(threadId in s.errors)) return s;
      const errors = { ...s.errors };
      delete errors[threadId];
      return { errors };
    }),

  // ── reducers ──────────────────────────────────────────────────────────
  applyContentDelta: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      if (e.agentId) {
        return {
          threadsById: {
            ...s.threadsById,
            [e.threadId]: updateSubAgent(thread, e.agentId, (sa) => ({
              ...sa,
              text: sa.text + e.delta,
            })),
          },
        };
      }
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => {
            const segs = [...m.segments];
            const last = segs[segs.length - 1];
            if (last && last.kind === "text") {
              segs[segs.length - 1] = { ...last, text: last.text + e.delta };
            } else {
              segs.push({ kind: "text", text: e.delta });
            }
            return { ...m, content: m.content + e.delta, segments: segs, isStreaming: true };
          }),
        },
      };
    }),

  applyReasoningDelta: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => {
            const segs = [...m.segments];
            const last = segs[segs.length - 1];
            if (last && last.kind === "reasoning") {
              segs[segs.length - 1] = { ...last, text: last.text + e.delta };
            } else {
              segs.push({ kind: "reasoning", text: e.delta });
            }
            return { ...m, reasoning: (m.reasoning ?? "") + e.delta, segments: segs, isStreaming: true };
          }),
        },
      };
    }),

  applyToolCallStart: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      if (e.agentId) {
        // Nested sub-agent tool call → attach to the owning task card.
        return {
          threadsById: {
            ...s.threadsById,
            [e.threadId]: updateSubAgent(thread, e.agentId, (sa) => ({
              ...sa,
              toolCalls: [...sa.toolCalls, e.toolCall],
            })),
          },
        };
      }
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => {
            const toolSeg: ToolSegment = {
              kind: "tool",
              callId: e.toolCall.id,
              name: e.toolCall.name,
              input: e.toolCall.input,
              status: e.toolCall.status,
              diff: e.toolCall.diff,
            };
            return {
              ...m,
              toolCalls: [...(m.toolCalls ?? []), e.toolCall],
              segments: [...m.segments, toolSeg],
              isStreaming: true,
            };
          }),
        },
      };
    }),

  applyToolCallUpdate: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      if (e.agentId) {
        // Nested sub-agent tool update → update the matching nested card.
        return {
          threadsById: {
            ...s.threadsById,
            [e.threadId]: updateSubAgent(thread, e.agentId, (sa) => ({
              ...sa,
              toolCalls: sa.toolCalls.map((tc): ToolCall =>
                tc.id !== e.toolUseId
                  ? tc
                  : {
                      ...tc,
                      input: mergeToolInput(tc.input, e.input),
                      status: e.status,
                      result: e.result,
                      diff: e.diff,
                      durationMs: e.durationMs,
                      errorMessage: e.errorMessage,
                    },
              ),
            })),
          },
        };
      }
      // 引用稳定优化：先找目标 message 的索引，只 patch 它，其他保持引用。
      const targetIdx = thread.messages.findIndex((m) =>
        m.segments.some((seg) => seg.kind === "tool" && seg.callId === e.toolUseId) ||
        m.toolCalls?.some((tc) => tc.id === e.toolUseId),
      );
      if (targetIdx === -1) return s;
      const patched = (() => {
        const m = thread.messages[targetIdx];
        return {
          ...m,
          toolCalls: m.toolCalls?.map((tc): ToolCall =>
            tc.id !== e.toolUseId
              ? tc
              : {
                  ...tc,
                  input: mergeToolInput(tc.input, e.input),
                  status: e.status,
                  result: e.result,
                  diff: e.diff,
                  durationMs: e.durationMs,
                  errorMessage: e.errorMessage,
                },
          ) ?? null,
          segments: m.segments.map((seg): Segment =>
            seg.kind === "tool" && seg.callId === e.toolUseId
              ? {
                  ...seg,
                  input: mergeToolInput(seg.input, e.input),
                  status: e.status,
                  result: e.result ?? undefined,
                  diff: e.diff,
                  durationMs: e.durationMs ?? undefined,
                  errorMessage: e.errorMessage ?? undefined,
                }
              : seg,
          ),
        };
      })();
      if (patched === thread.messages[targetIdx]) return s;
      const messages = thread.messages.map((m, i) => (i === targetIdx ? patched : m));
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: { ...thread, messages },
        },
        // 工具离开 pending_approval (变为 running/success/error/aborted) 时,
        // 对应审批条目已无意义 — 出队,防止 pendingApprovals 无限增长.
        pendingApprovals:
          e.status === "pending_approval"
            ? s.pendingApprovals
            : s.pendingApprovals.filter((a) => a.toolUseId !== e.toolUseId),
        // 问答面板:仅在工具进入终态(success/error/aborted)时清理残留.
        // 不在 "running" 清理 —— ask_user_question 是只读工具,会先发
        // running 再 emit question:request(此时面板才入队),在 running
        // 清理会误杀尚未入队的面板.正常提交走 consumeQuestion 出队.
        pendingQuestions:
          e.status === "success" || e.status === "error" || e.status === "aborted"
            ? s.pendingQuestions.filter((q) => q.toolUseId !== e.toolUseId)
            : s.pendingQuestions,
      };
    }),

  applyTurnComplete: (e) => {
    // Compute the new state purely inside `set`; collect side effects to run
    // AFTER so the reducer stays a pure state transform (P2-7).
    let autoTitleToPersist: string | null = null;
    let didApply = false;
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      didApply = true;
      // 精确匹配优先;若 id 对不上 (后端 turn_complete.messageId 偶尔与
      // content_delta.messageId 不一致),fallback 把所有 isStreaming
      // assistant message 都收尾,防止 ChatPage streaming flag 卡住.
      const exactMatch = thread.messages.some((m) => m.id === e.messageId);
      const messages = thread.messages.map((m) => {
        if (m.id === e.messageId) {
          return { ...m, isStreaming: false, usage: e.usage };
        }
        if (!exactMatch && m.role === "assistant" && m.isStreaming) {
          return { ...m, isStreaming: false, usage: e.usage };
        }
        return m;
      });
      // 自动标题: 第一轮 turn 结束后,如果标题仍是默认值,基于首条用户消息生成
      const updatedThread = { ...thread, messages };
      const autoTitle = generateTitleFromFirstMessage(updatedThread);
      if (autoTitle) {
        // 立刻更新本地缓存 (不等后端确认);持久化在 set 之后做.
        updatedThread.title = autoTitle;
        autoTitleToPersist = autoTitle;
      }
      // turn 耗时: wall-clock(turn 起点 → 现在),按收尾的 assistant 消息 id
      // 记录.精确匹配用 e.messageId;fallback 时用最后一条收尾的 streaming
      // 消息 id,与上面 usage 的归属一致.
      const start = s.turnStartByThread[e.threadId];
      const turnDurationByMsg = { ...s.turnDurationByMsg };
      if (start) {
        const elapsed = Date.now() - start;
        if (exactMatch) {
          turnDurationByMsg[e.messageId] = elapsed;
        } else {
          for (const m of thread.messages) {
            if (m.role === "assistant" && m.isStreaming) {
              turnDurationByMsg[m.id] = elapsed;
            }
          }
        }
      }
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: updatedThread,
        },
        pendingTurnByThread: {
          ...s.pendingTurnByThread,
          [e.threadId]: false,
        },
        turnDurationByMsg,
        // turn 结束 → 该 thread 不可能再有 pending 审批,清掉残留条目
        pendingApprovals: s.pendingApprovals.filter(
          (a) => a.threadId !== e.threadId,
        ),
        // turn 结束 → 该 thread 也不可能再有 pending 问答,清掉残留
        pendingQuestions: s.pendingQuestions.filter(
          (q) => q.threadId !== e.threadId,
        ),
      };
    });

    if (!didApply) return;

    // ── side effects (outside the reducer) ──────────────────────────────
    // P3a: turn 完后余额刚被扣完,异步刷新最新余额. balanceStore 自处理失败.
    void useBalanceStore.getState().reload();
    // 自动标题持久化 + 刷新 sidebar (本地缓存已在 set 内更新).
    if (autoTitleToPersist) {
      void agentClient
        .updateThread({ threadId: e.threadId, title: autoTitleToPersist })
        .then(() => useSessionStore.getState().loadThreads())
        .catch(() => { /* 静默失败,标题只是 nice-to-have */ });
    }
    // 回填持久化 seq (P2 回溯): 流式期间产生的消息是乐观本地条目,没有
    // seq,UserMessage 的「回到这里」按钮 (seq != null 才渲染) 因此缺失.
    // turn 结束后台重拉一次,把后端持久化的 seq 同步进缓存. reloadThread
    // 的 stale-guard 此时已放行 (pendingTurn=false 且长度一致), 不会覆盖.
    void get().reloadThread(e.threadId);
  },

  applyApprovalRequest: (e) =>
    set((s) => {
      // 去重:同一 toolUseId 不重复入队 (后端重发 / HybridClient 双触发兜底)
      if (s.pendingApprovals.some((a) => a.toolUseId === e.toolUseId)) {
        return s;
      }
      return { pendingApprovals: [...s.pendingApprovals, e] };
    }),

  applyQuestionRequest: (e) =>
    set((s) => {
      // 去重:同一 toolUseId 不重复入队
      if (s.pendingQuestions.some((q) => q.toolUseId === e.toolUseId)) {
        return s;
      }
      return { pendingQuestions: [...s.pendingQuestions, e] };
    }),

  applyStreamError: (e) =>
    set((s) => {
      const next: Partial<ChatState> = {
        errors: { ...s.errors, [e.threadId]: e.error },
        pendingTurnByThread: { ...s.pendingTurnByThread, [e.threadId]: false },
        // turn 出错终止 → 清掉该 thread 残留审批条目
        pendingApprovals: s.pendingApprovals.filter(
          (a) => a.threadId !== e.threadId,
        ),
        // turn 出错终止 → 清掉该 thread 残留问答面板
        pendingQuestions: s.pendingQuestions.filter(
          (q) => q.threadId !== e.threadId,
        ),
      };
      if (e.messageId) {
        const thread = s.threadsById[e.threadId];
        if (thread) {
          const messages = thread.messages.map((m) =>
            m.id === e.messageId
              ? { ...m, isStreaming: false, interrupted: true }
              : m,
          );
          next.threadsById = {
            ...s.threadsById,
            [e.threadId]: { ...thread, messages },
          };
        }
      }
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[stream:error]", e.threadId, e.error);
      }
      return next;
    }),

  applyStreamAborted: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      const next: Partial<ChatState> = {
        pendingTurnByThread: { ...s.pendingTurnByThread, [e.threadId]: false },
        // turn 被中止 (含 Esc 取消审批走 abortTurn) → 清掉该 thread 残留审批条目
        pendingApprovals: s.pendingApprovals.filter(
          (a) => a.threadId !== e.threadId,
        ),
        // turn 被中止 → 清掉该 thread 残留问答面板
        pendingQuestions: s.pendingQuestions.filter(
          (q) => q.threadId !== e.threadId,
        ),
      };
      if (thread) {
        const exactMatch = thread.messages.some((m) => m.id === e.messageId);
        const messages = thread.messages.map((m) => {
          if (m.id === e.messageId) {
            return { ...m, isStreaming: false, interrupted: true };
          }
          if (!exactMatch && m.role === "assistant" && m.isStreaming) {
            return { ...m, isStreaming: false, interrupted: true };
          }
          return m;
        });
        next.threadsById = {
          ...s.threadsById,
          [e.threadId]: { ...thread, messages },
        };
      }
      return next;
    }),

  consumeApproval: (toolUseId) =>
    set((s) => ({
      pendingApprovals: s.pendingApprovals.filter(
        (a) => a.toolUseId !== toolUseId,
      ),
    })),

  consumeQuestion: (toolUseId) =>
    set((s) => ({
      pendingQuestions: s.pendingQuestions.filter(
        (q) => q.toolUseId !== toolUseId,
      ),
    })),

  applyTodosUpdated: (e) =>
    set((s) => ({
      todosByThread: { ...s.todosByThread, [e.threadId]: e.todos },
    })),

  applyContextUsage: (e) =>
    set((s) => ({
      contextUsageByThread: {
        ...s.contextUsageByThread,
        [e.threadId]: {
          usedTokens: e.usedTokens,
          maxTokens: e.maxTokens,
          ratio: e.ratio,
        },
      },
    })),

  applyBrainstormRunStarted: (e) =>
    set((s) => ({
      activeBrainstormRunByThread: {
        ...s.activeBrainstormRunByThread,
        [e.threadId]: e.runId,
      },
      brainstormRunsById: {
        ...s.brainstormRunsById,
        [e.runId]: {
          threadId: e.threadId,
          topic: e.topic,
          participants: e.participants,
          status: "running",
        },
      },
    })),

  applyBrainstormAgentStatus: (_e) => {
    // v1 不单独渲染状态栏，发言消息自身的 streaming 态就是状态。
  },

  applyBrainstormMessageStart: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      if (thread.messages.some((m) => m.id === e.messageId)) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: {
            ...thread,
            messages: [...thread.messages, localBrainstormMessage(e)],
          },
        },
        pendingTurnByThread: { ...s.pendingTurnByThread, [e.threadId]: true },
      };
    }),

  applyBrainstormMessageDelta: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      const messages = thread.messages.map((m) => {
        if (m.id !== e.messageId) return m;
        const segs = [...m.segments];
        const last = segs[segs.length - 1];
        if (last?.kind === "text") {
          segs[segs.length - 1] = { ...last, text: last.text + e.delta };
        } else {
          segs.push({ kind: "text", text: e.delta });
        }
        return {
          ...m,
          content: m.content + e.delta,
          segments: segs,
          isStreaming: true,
        };
      });
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: { ...thread, messages },
        },
      };
    }),

  applyBrainstormReasoningDelta: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => {
            const segs = [...m.segments];
            const last = segs[segs.length - 1];
            if (last?.kind === "reasoning") {
              segs[segs.length - 1] = { ...last, text: last.text + e.delta };
            } else {
              segs.push({ kind: "reasoning", text: e.delta });
            }
            return {
              ...m,
              reasoning: (m.reasoning ?? "") + e.delta,
              segments: segs,
              isStreaming: true,
            };
          }),
        },
      };
    }),

  applyBrainstormToolCallStart: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => {
            if (m.segments.some((seg) => seg.kind === "tool" && seg.callId === e.toolCall.id)) {
              return m;
            }
            const toolSeg: ToolSegment = {
              kind: "tool",
              callId: e.toolCall.id,
              name: e.toolCall.name,
              input: e.toolCall.input,
              status: e.toolCall.status,
              diff: e.toolCall.diff,
            };
            return {
              ...m,
              toolCalls: [...(m.toolCalls ?? []), e.toolCall],
              segments: [...m.segments, toolSeg],
              isStreaming: true,
            };
          }),
        },
      };
    }),

  applyBrainstormToolCallUpdate: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: upsertMessage(thread, e.messageId, (m) => ({
            ...m,
            toolCalls:
              m.toolCalls?.map((tc): ToolCall =>
                tc.id !== e.toolUseId
                  ? tc
                  : {
                      ...tc,
                      input: mergeToolInput(tc.input, e.input),
                      status: e.status,
                      result: e.result,
                      durationMs: e.durationMs,
                      errorMessage: e.errorMessage,
                    },
              ) ?? null,
            segments: m.segments.map((seg): Segment =>
              seg.kind === "tool" && seg.callId === e.toolUseId
                ? {
                    ...seg,
                    input: mergeToolInput(seg.input, e.input),
                    status: e.status,
                    result: e.result ?? undefined,
                    durationMs: e.durationMs ?? undefined,
                    errorMessage: e.errorMessage ?? undefined,
                  }
                : seg,
            ),
            isStreaming: true,
          })),
        },
      };
    }),

  applyBrainstormMessageDone: (e) =>
    set((s) => {
      const thread = s.threadsById[e.threadId];
      if (!thread) return s;
      const messages = thread.messages.map((m) => {
        if (m.id !== e.messageId) return m;
        const hasVisibleSegment = m.segments.some((seg) =>
          seg.kind === "text"
            ? seg.text.trim().length > 0
            : seg.kind === "reasoning"
              ? seg.text.trim().length > 0
              : true,
        );
        const shouldReplaceSegments =
          !hasVisibleSegment || /^第\s*\d+\s*轮调度中/.test(m.content.trim());
        const segments: Segment[] = shouldReplaceSegments
          ? [{ kind: "text", text: e.content }]
          : m.segments;
        return {
          ...m,
          content: e.content,
          segments,
          isStreaming: false,
        };
      });
      return {
        threadsById: {
          ...s.threadsById,
          [e.threadId]: { ...thread, messages },
        },
      };
    }),

  applyBrainstormRunDone: (e) => {
    set((s) => ({
      threadsById: s.threadsById[e.threadId]
        ? {
            ...s.threadsById,
            [e.threadId]: {
              ...s.threadsById[e.threadId],
              messages: reconcileBrainstormSummary(
                s.threadsById[e.threadId].messages,
                e,
              ),
            },
          }
        : s.threadsById,
      pendingTurnByThread: { ...s.pendingTurnByThread, [e.threadId]: false },
      brainstormRunsById: s.brainstormRunsById[e.runId]
        ? {
            ...s.brainstormRunsById,
            [e.runId]: {
              ...s.brainstormRunsById[e.runId],
              status: "done",
            },
          }
        : s.brainstormRunsById,
    }));
    void get().reloadThread(e.threadId);
    void useSessionStore.getState().loadThreads();
  },

  applyBrainstormError: (e) =>
    set((s) => ({
      pendingTurnByThread: { ...s.pendingTurnByThread, [e.threadId]: false },
      brainstormRunsById: s.brainstormRunsById[e.runId]
        ? {
            ...s.brainstormRunsById,
            [e.runId]: {
              ...s.brainstormRunsById[e.runId],
              status: "error",
            },
          }
        : s.brainstormRunsById,
      errors: { ...s.errors, [e.threadId]: e.error },
    })),
}));

// ── 派生 hook (兼容旧 useChatStore(s => s.thread) 用法) ─────────────────

/** 当前路由对应的 thread,welcome / settings 时返回 null. */
export function useActiveThread(): Thread | null {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) => (id ? s.threadsById[id] ?? null : null));
}

/** 当前路由对应的 getThread 加载态. */
export function useActiveThreadLoading(): boolean {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) => (id ? !!s.loading[id] : false));
}

/** 当前路由 thread 是否处于 sendMessage → turn_complete 之间. */
export function useActiveThreadPendingTurn(): boolean {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) => (id ? !!s.pendingTurnByThread[id] : false));
}

/** 模块级空数组常量. */
const EMPTY_TODOS: TodoItem[] = [];

/** 当前路由 thread 的任务列表(per-thread,互不串). */
export function useActiveThreadTodos(): TodoItem[] {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) =>
    id ? s.todosByThread[id] ?? EMPTY_TODOS : EMPTY_TODOS,
  );
}

/** 当前路由 thread 的上下文用量(驱动 ComposeBar 圆环). */
export function useActiveThreadContextUsage(): {
  usedTokens: number;
  maxTokens: number;
  ratio: number;
} | null {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) =>
    id ? s.contextUsageByThread[id] ?? null : null,
  );
}

/** 当前路由 thread 的错误信息（getThread / sendMessage / stream:error 失败）。 */
export function useActiveThreadError(): string | null {
  const route = useRouterStore((s) => s.current);
  const id = route.page === "chat" ? route.threadId : null;
  return useChatStore((s) => (id ? s.errors[id] ?? null : null));
}

/** 某条消息对应 turn 的 wall-clock 耗时 (ms);无记录返回 null。 */
export function useMessageDuration(messageId: string): number | null {
  return useChatStore((s) => s.turnDurationByMsg[messageId] ?? null);
}
