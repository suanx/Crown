import { create } from "zustand";

/**
 * 极简 router.
 *
 * Routes:
 *   - welcome      欢迎页
 *   - chat         对话页
 *   - skills       技能管理
 *   - settings     设置 (内 tab)
 *
 * Projects 不再是独立路由 — 已融合进 ChatSidebar 的项目分组.
 */

export type Route =
  | { page: "welcome" }
  | { page: "chat"; threadId: string }
  | { page: "skills" }
  | { page: "settings"; tab?: SettingsTab };

export type SettingsTab =
  | "general"
  | "provider"
  | "models"
  | "capabilities"
  | "outputStyles"
  | "mcp"
  | "permissions"
  | "hooks"
  | "billing"
  | "shortcuts"
  | "developer"
  | "about";

interface RouterState {
  current: Route;
  navigate: (route: Route) => void;
  searchOpen: boolean;
  toggleSearch: (v?: boolean) => void;
}

export const useRouterStore = create<RouterState>((set, get) => ({
  current: { page: "welcome" },
  navigate: (route) => set({ current: route }),
  searchOpen: false,
  toggleSearch: (v) =>
    set({ searchOpen: typeof v === "boolean" ? v : !get().searchOpen }),
}));
