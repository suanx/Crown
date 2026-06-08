/**
 * ============================================================================
 * useTauriEvents — 全局 IPC 事件订阅入口
 * ============================================================================
 *
 * 在 App 顶层 mount 一次,订阅 AgentClient 的 10 个 stream / approval / status
 * 事件,把 payload 转发到 chatStore 的 reducer actions.
 *
 * 设计要点:
 *   - 单例订阅:整个 app 生命周期只 listen 一次,unmount 时统一 cleanup
 *   - 不依赖 React 树位置:chatStore 通过 selector 给各页面分发,组件无需
 *     重新订阅
 *   - actions 用 useChatStore.getState() 取最新引用,避免订阅函数的闭包陷阱
 *     (zustand 的 setter 永远稳定,直接通过 store API 获取即可)
 *   - HybridClient.subscribe 已经处理 Tauri listen Promise 的异步竞态 + mock
 *     fallback,这里只接它给的 Unsubscribe
 *
 * 报告 §1 修复点:整个仓库以前没人调 agentClient.on*,导致后端 emit 的
 * 流式事件全部进黑洞.
 * ----------------------------------------------------------------------------
 */

import { useEffect } from "react";
import { agentClient } from "@/api";
import { useChatStore } from "@/stores/chatStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";

export function useTauriEvents() {
  useEffect(() => {
    const store = useChatStore.getState();

    const unsubs = [
      // 流式 delta 直接 apply —— 逐 token 即时渲染最丝滑。content_delta 是
      // 一个个独立的异步 IPC 事件（各自微任务），不会攒进同一个同步批次，
      // 因此不会触发 React 的 "Maximum update depth"，无需 rAF 合并（合并反
      // 而把逐字流式变成块状跳变，损失丝滑感）。
      agentClient.onContentDelta(store.applyContentDelta),
      agentClient.onReasoningDelta(store.applyReasoningDelta),
      agentClient.onToolCallStart(store.applyToolCallStart),
      agentClient.onToolCallUpdate(store.applyToolCallUpdate),
      agentClient.onTurnComplete(store.applyTurnComplete),
      agentClient.onApprovalRequest(store.applyApprovalRequest),
      agentClient.onQuestionRequest(store.applyQuestionRequest),
      agentClient.onStreamError(store.applyStreamError),
      agentClient.onStreamAborted(store.applyStreamAborted),
      // P4 永不 emit 但订阅必须挂着,等 P3 cost / escalation 实装时自动生效
      agentClient.onBudgetWarning((e) => {
        if (import.meta.env.DEV) {
          // eslint-disable-next-line no-console
          console.info("[budget:warning]", e);
        }
      }),
      agentClient.onModelEscalated((e) => {
        if (import.meta.env.DEV) {
          // eslint-disable-next-line no-console
          console.info("[model:escalated]", e);
        }
      }),
      agentClient.onTodosUpdated((e) => {
        store.applyTodosUpdated(e);
        if (e.todos.length > 0) {
          const ws = useWorkspaceStore.getState();
          if (!ws.rightContent) {
            ws.openInRight("tasks");
          }
        }
      }),
      agentClient.onContextUsage(store.applyContextUsage),
      agentClient.onBrainstormRunStarted(store.applyBrainstormRunStarted),
      agentClient.onBrainstormAgentStatus(store.applyBrainstormAgentStatus),
      agentClient.onBrainstormMessageStart(store.applyBrainstormMessageStart),
      agentClient.onBrainstormMessageDelta(store.applyBrainstormMessageDelta),
      agentClient.onBrainstormReasoningDelta(store.applyBrainstormReasoningDelta),
      agentClient.onBrainstormToolCallStart(store.applyBrainstormToolCallStart),
      agentClient.onBrainstormToolCallUpdate(store.applyBrainstormToolCallUpdate),
      agentClient.onBrainstormMessageDone(store.applyBrainstormMessageDone),
      agentClient.onBrainstormRunDone(store.applyBrainstormRunDone),
      agentClient.onBrainstormError(store.applyBrainstormError),
    ];

    return () => {
      for (const unsub of unsubs) unsub();
    };
  }, []);
}
