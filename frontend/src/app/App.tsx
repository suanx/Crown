import { useEffect } from "react";
import { useRouterStore } from "@/stores/routerStore";
import { useUiStore } from "@/stores/uiStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import { useSessionStore } from "@/stores/sessionStore";
import { useProjectStore } from "@/stores/projectStore";
import { useChatStore } from "@/stores/chatStore";

import { useSettingsStore } from "@/stores/settingsStore";
import { useTauriEvents } from "@/hooks/useTauriEvents";
import { AppShell } from "./AppShell";
import { ChatPage } from "@/features/chat/ChatPage";
import { WelcomePage } from "@/features/chat/WelcomePage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { SkillsPage } from "@/features/skills/SkillsPage";
import { SearchPalette } from "@/features/search/SearchPalette";
import { DevtoolsPanel } from "@/features/devtools/DevtoolsPanel";

export function App() {
  const route = useRouterStore((s) => s.current);
  const toggleSearch = useRouterStore((s) => s.toggleSearch);
  const loadThreads = useSessionStore((s) => s.loadThreads);
  const loadProjects = useProjectStore((s) => s.loadProjects);
  const loadThread = useChatStore((s) => s.loadThread);
  const toggleDevtools = useUiStore((s) => s.toggleDevtools);
  const toggleLeftSidebar = useWorkspaceStore((s) => s.toggleLeftSidebar);
  const toggleRightPanel = useWorkspaceStore((s) => s.toggleRightPanel);
  const toggleBottomPanel = useWorkspaceStore((s) => s.toggleBottomPanel);

  // 全局 IPC 事件订阅 — 必须 mount 一次
  useTauriEvents();

  useEffect(() => {
    void loadThreads();
    void loadProjects();

    void useSettingsStore
      .getState()
      .load()
      .then(() => {
        const settings = useSettingsStore.getState();
        useUiStore.getState().setTheme(settings.theme);
      });
  }, [loadProjects, loadThreads]);

  useEffect(() => {
    if (route.page === "chat") void loadThread(route.threadId);
  }, [route, loadThread]);

  // 全局快捷键
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.ctrlKey || e.metaKey;
      const k = e.key.toLowerCase();

      if (meta && e.shiftKey && k === "d") {
        e.preventDefault();
        toggleDevtools();
        return;
      }
      if (meta && !e.shiftKey && !e.altKey && k === "k") {
        e.preventDefault();
        toggleSearch();
        return;
      }
      if (meta && !e.shiftKey && !e.altKey && k === "b") {
        e.preventDefault();
        toggleLeftSidebar();
        return;
      }
      if (meta && e.altKey && k === "b") {
        e.preventDefault();
        toggleRightPanel();
        return;
      }
      if (meta && !e.shiftKey && !e.altKey && k === "j") {
        e.preventDefault();
        toggleBottomPanel();
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    toggleDevtools,
    toggleSearch,
    toggleLeftSidebar,
    toggleRightPanel,
    toggleBottomPanel,
  ]);

  return (
    <>
      <AppShell>
        {route.page === "welcome" && <WelcomePage />}
        {route.page === "chat" && <ChatPage threadId={route.threadId} />}
        {route.page === "skills" && <SkillsPage />}
        {route.page === "settings" && (
          <SettingsPage tab={route.tab ?? "general"} />
        )}
      </AppShell>

      <SearchPalette />
      <DevtoolsPanel />
    </>
  );
}
