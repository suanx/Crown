import { create } from "zustand";
import type { ColorScheme, PermissionMode, ThemeMode, ThinkingEffort } from "@/api";

/**
 * UI 状态 — 仅会话内的 ephemeral 状态.
 * Workspace dock (sidebar / right panel / bottom panel) 在 workspaceStore.
 * Approval 决策从这里删除 — 改为内嵌在 ToolCallCard 内部 state.
 */
interface UiState {
  theme: ThemeMode;
  setTheme: (m: ThemeMode) => void;

  colorScheme: ColorScheme;
  setColorScheme: (s: ColorScheme) => void;

  currentModel: string;
  setCurrentModel: (m: string) => void;
  currentProviderId: string;
  setCurrentProviderId: (providerId: string) => void;
  currentThinkingEffort: ThinkingEffort;
  setCurrentThinkingEffort: (effort: ThinkingEffort) => void;
  permissionMode: PermissionMode;
  setPermissionMode: (m: PermissionMode) => void;

  autoScroll: boolean;
  setAutoScroll: (v: boolean) => void;

  showBalanceInSidebar: boolean;
  setShowBalanceInSidebar: (v: boolean) => void;

  showMessageCost: boolean;
  setShowMessageCost: (v: boolean) => void;

  /** Devtools 面板 (Ctrl+Shift+D). */
  devtoolsOpen: boolean;
  toggleDevtools: (v?: boolean) => void;
}

const UI_STORAGE_KEY = "ds.ui.v2";

type PersistedUi = Pick<
  UiState,
  "theme" | "colorScheme" | "showBalanceInSidebar" | "showMessageCost"
>;

function loadPersistedUi(): PersistedUi {
  if (typeof window === "undefined") {
    return { theme: "dark", colorScheme: "default", showBalanceInSidebar: true, showMessageCost: false };
  }
  try {
    return {
      theme: "dark",
      colorScheme: "default",
      showBalanceInSidebar: true,
      showMessageCost: false,
      ...JSON.parse(localStorage.getItem(UI_STORAGE_KEY) || "{}"),
    };
  } catch {
    return { theme: "dark", colorScheme: "default", showBalanceInSidebar: true, showMessageCost: false };
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

  colorScheme: persistedUi.colorScheme,
  setColorScheme: (colorScheme) => {
    set({ colorScheme });
    savePersistedUi({ colorScheme });
    applyColorScheme(colorScheme);
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

function applyColorScheme(scheme: ColorScheme) {
  const root = document.documentElement;
  // Remove all scheme classes then add current
  const schemes: ColorScheme[] = ["default", "ocean", "orchid", "flame", "rose", "forest", "midnight"];
  for (const s of schemes) {
    root.classList.toggle(`scheme-${s}`, s === scheme);
  }
}

// Apply persisted settings on init
applyTheme(useUiStore.getState().theme);
applyColorScheme(useUiStore.getState().colorScheme);
