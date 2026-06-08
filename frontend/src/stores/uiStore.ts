import { create } from "zustand";
import type { PermissionMode, ThemeMode, ThinkingEffort } from "@/api";

/**
 * UI 状态 — 仅会话内的 ephemeral 状态.
 * Workspace dock (sidebar / right panel / bottom panel) 在 workspaceStore.
 * Approval 决策从这里删除 — 改为内嵌在 ToolCallCard 内部 state.
 */
interface UiState {
  theme: ThemeMode;
  setTheme: (m: ThemeMode) => void;

  currentModel: string;
  setCurrentModel: (m: string) => void;
  currentProviderId: string;
  setCurrentProviderId: (providerId: string) => void;
  currentThinkingEffort: ThinkingEffort;
  setCurrentThinkingEffort: (effort: ThinkingEffort) => void;
  permissionMode: PermissionMode;
  setPermissionMode: (m: PermissionMode) => void;

  /**
   * 流式回复时 MessageList 自动滚到底.
   * 阶段三 #8 GeneralPanel "自动滚动" toggle 接通后由 setConfig 双向同步;
   * 当前先内置默认 true,UI 通过 useUiStore 读取.
   */
  autoScroll: boolean;
  setAutoScroll: (v: boolean) => void;

  /**
   * 主页 (sidebar 底部账户行) 显示余额.
   * 默认 true.关闭后 sidebar 用户卡显示 v0.1.0 副标题,余额仍可在
   * Settings → 用量与计费 页查看.
   */
  showBalanceInSidebar: boolean;
  setShowBalanceInSidebar: (v: boolean) => void;

  /**
   * 单条消息底部显示成本徽章.
   * 默认 false.关闭后 MessageMeta 仅显 token 数 + 缓存命中率.
   * 开启时单条 cost 用 CNY 显示 (后端 USD * 静态汇率).
   */
  showMessageCost: boolean;
  setShowMessageCost: (v: boolean) => void;

  /** Devtools 面板 (Ctrl+Shift+D). */
  devtoolsOpen: boolean;
  toggleDevtools: (v?: boolean) => void;
}

const UI_STORAGE_KEY = "ds.ui.v1";

type PersistedUi = Pick<
  UiState,
  "theme" | "showBalanceInSidebar" | "showMessageCost"
>;

function loadPersistedUi(): PersistedUi {
  if (typeof window === "undefined") {
    return {
      theme: "dark",
      showBalanceInSidebar: true,
      showMessageCost: false,
    };
  }
  try {
    return {
      theme: "dark",
      showBalanceInSidebar: true,
      showMessageCost: false,
      ...JSON.parse(localStorage.getItem(UI_STORAGE_KEY) || "{}"),
    };
  } catch {
    return {
      theme: "dark",
      showBalanceInSidebar: true,
      showMessageCost: false,
    };
  }
}

function savePersistedUi(patch: Partial<PersistedUi>) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(
      UI_STORAGE_KEY,
      JSON.stringify({ ...loadPersistedUi(), ...patch }),
    );
  } catch {
    /* ignore storage errors */
  }
}

const persistedUi = loadPersistedUi();

export const useUiStore = create<UiState>((set) => ({
  theme: persistedUi.theme,
  setTheme: (theme) => {
    set({ theme });
    savePersistedUi({ theme });
    applyTheme(theme);
  },

  currentModel: "deepseek-v4-flash",
  setCurrentModel: (currentModel) => set({ currentModel }),
  currentProviderId: "deepseek",
  setCurrentProviderId: (currentProviderId) => set({ currentProviderId }),
  currentThinkingEffort: "medium",
  setCurrentThinkingEffort: (currentThinkingEffort) =>
    set({ currentThinkingEffort }),

  permissionMode: "default",
  setPermissionMode: (permissionMode) => set({ permissionMode }),

  autoScroll: true,
  setAutoScroll: (autoScroll) => set({ autoScroll }),

  showBalanceInSidebar: persistedUi.showBalanceInSidebar,
  setShowBalanceInSidebar: (showBalanceInSidebar) => {
    set({ showBalanceInSidebar });
    savePersistedUi({ showBalanceInSidebar });
  },

  showMessageCost: persistedUi.showMessageCost,
  setShowMessageCost: (showMessageCost) => {
    set({ showMessageCost });
    savePersistedUi({ showMessageCost });
  },

  devtoolsOpen: false,
  toggleDevtools: (v) =>
    set((s) => ({
      devtoolsOpen: typeof v === "boolean" ? v : !s.devtoolsOpen,
    })),
}));

function applyTheme(theme: ThemeMode) {
  const root = document.documentElement;
  const isDark =
    theme === "dark" ||
    (theme === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  root.classList.toggle("dark", isDark);
}

applyTheme(useUiStore.getState().theme);
