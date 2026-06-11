/**
 * ============================================================================
 * balanceStore — 用户余额查询 (P3a task 7)
 * ============================================================================
 *
 * 设计:
 *
 * - 不放进 chatStore 是因为余额跟 turn 流式状态生命周期不同:turn 结束后想刷
 *   余额,但余额本身跟 messages / pendingApprovals 完全独立,职责分离.
 *
 * - 失败 (网络 / 认证 / 不支持 provider) 时 client 返 null,store 把 balance
 *   存为 null + lastError 留诊断字符串.UI 应根据 balance == null 隐藏 cell,
 *   不显错.
 *
 * - 刷新策略 (handoff 推荐):App mount 一次 + 每 turn_complete 后一次.不轮询.
 *   chatStore.applyTurnComplete reducer 里调用 useBalanceStore.getState().reload()
 *   触发刷新.
 * ----------------------------------------------------------------------------
 */

import { create } from "zustand";
import { agentClient, type UserBalance } from "@/api";

interface BalanceState {
  balance: UserBalance | null;
  loading: boolean;
  /** 仅 dev 诊断用,UI 不显示. */
  lastError: string | null;
  reload: () => Promise<void>;
}

export const useBalanceStore = create<BalanceState>((set) => ({
  balance: null,
  loading: false,
  lastError: null,

  reload: async () => {
    set({ loading: true });
    try {
      const next = await agentClient.getUserBalance();
      // 后端真返 null (provider 调用失败) 时 next 为 null,UI 收到 null 自行隐藏
      set({ balance: next, loading: false, lastError: null });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      set({ balance: null, loading: false, lastError: msg });
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[balanceStore] reload failed:", msg);
      }
    }
  },
}));
