import { create } from "zustand";
import type { ColorScheme, PermissionMode, ThemeMode, ThemeVariant, ThinkingEffort } from "@/api";

/**
 * UI 状态 — 仅会话内的 ephemeral 状态.
 */
interface UiState {
  theme: ThemeMode;
  setTheme: (m: ThemeMode) => void;

  themeVariant: ThemeVariant;
  setThemeVariant: (v: ThemeVariant) => void;

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

  devtoolsOpen: boolean;
  toggleDevtools: (v?: boolean) => void;
}

const UI_STORAGE_KEY = "ds.ui.v3";

type PersistedUi = Pick<
  UiState,
  "theme" | "themeVariant" | "colorScheme" | "showBalanceInSidebar" | "showMessageCost"
>;

function loadPersistedUi(): PersistedUi {
  if (typeof window === "undefined") {
    return { theme: "dark", themeVariant: "classic", colorScheme: "default", showBalanceInSidebar: true, showMessageCost: false };
  }
  try {
    return {
      theme: "dark",
      themeVariant: "classic",
      colorScheme: "default",
      showBalanceInSidebar: true,
      showMessageCost: false,
      ...JSON.parse(localStorage.getItem(UI_STORAGE_KEY) || "{}"),
    };
  } catch {
    return { theme: "dark", themeVariant: "classic", colorScheme: "default", showBalanceInSidebar: true, showMessageCost: false };
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

  themeVariant: persistedUi.themeVariant,
  setThemeVariant: (themeVariant) => {
    set({ themeVariant });
    savePersistedUi({ themeVariant });
    applyThemeVariant(themeVariant);
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

const ALL_VARIANTS: ThemeVariant[] = ["classic", "minimal", "vibrant", "sepia", "oled"];

function applyThemeVariant(variant: ThemeVariant) {
  const root = document.documentElement;
  for (const v of ALL_VARIANTS) {
    root.classList.toggle(`theme-${v}`, v === variant);
  }
}

function applyColorScheme(scheme: ColorScheme) {
  const root = document.documentElement;
  const schemes: ColorScheme[] = ["default", "ocean", "orchid", "flame", "rose", "forest", "midnight"];
  for (const s of schemes) {
    root.classList.toggle(`scheme-${s}`, s === scheme);
  }
}

// Apply persisted settings on init
applyTheme(useUiStore.getState().theme);
applyThemeVariant(useUiStore.getState().themeVariant);
applyColorScheme(useUiStore.getState().colorScheme);
