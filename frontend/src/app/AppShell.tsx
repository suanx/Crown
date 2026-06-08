import { useEffect, type ReactNode } from "react";
import { useRouterStore } from "@/stores/routerStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import { ChatSidebar } from "@/features/sidebar/ChatSidebar";
import { SettingsSidebar } from "@/features/sidebar/SettingsSidebar";
import { ResizeHandle } from "@/features/workspace/ResizeHandle";
import { RightPanel } from "@/features/workspace/RightPanel";
import { BottomPanel } from "@/features/workspace/BottomPanel";
import { WindowControls } from "@/shared/ui/WindowControls";
import { Icon } from "@/shared/icons/Icon";
import { MenuIcon, SidebarIcon, SearchIcon } from "@/shared/icons/set";
import type { Icon as PhIcon } from "@phosphor-icons/react";

interface AppShellProps {
  children: ReactNode;
}

/**
 * 3 区 CSS Grid 主壳 — 无 TopBar,Claude 桌面端风格一体感.
 *
 *   ┌──────┬──────────────────────────────┬───────────────┐
 *   │      │                              │               │
 *   │ Side │  Main (chat / settings ...)  │  RightPanel   │
 *   │ bar  │                              │               │
 *   │      ├──────────────────────────────┤               │
 *   │      │  BottomPanel                 │               │
 *   └──────┴──────────────────────────────┴───────────────┘
 *
 * 无分隔线,仅靠 bg 色差区分 sidebar (bg-elevated) 与 main (bg-canvas).
 * 窗口拖拽区域通过 Tauri 的 data-tauri-drag-region 或 CSS drag-region 处理.
 */
export function AppShell({ children }: AppShellProps) {
  const route = useRouterStore((s) => s.current);
  const sidebarW = useWorkspaceStore((s) => s.sidebarW);
  const setSidebarWidth = useWorkspaceStore((s) => s.setSidebarWidth);
  const leftSidebar = useWorkspaceStore((s) => s.leftSidebar);
  const toggleLeftSidebar = useWorkspaceStore((s) => s.toggleLeftSidebar);
  const toggleSearch = useRouterStore((s) => s.toggleSearch);

  // 容器查询断点 — 强制隐藏 right/bottom
  useEffect(() => {
    function onResize() {
      const root = document.documentElement;
      if (window.innerWidth < 900) {
        root.setAttribute("data-compact", "true");
      } else {
        root.removeAttribute("data-compact");
      }
    }
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const isSettings = route.page === "settings";

  return (
    <div
      className="h-screen w-screen bg-canvas text-text-primary"
      style={{
        display: "grid",
        gridTemplateAreas: `
          "titlebar titlebar titlebar"
          "sidebar  main     right"
          "sidebar  bottom   right"
        `,
        gridTemplateColumns:
          "var(--ds-sidebar-w) 1fr var(--ds-right-w)",
        gridTemplateRows: "36px 1fr var(--ds-bottom-h)",
      }}
    >
      {/* 自定义标题栏 — 整行 drag-region,按钮 no-drag */}
      <div
        style={{ gridArea: "titlebar" }}
        className="flex items-center drag-region"
      >
        {/* 左侧操作按钮 */}
        <div className="flex items-center gap-0.5 px-2 shrink-0 no-drag">
          <TitleBarBtn icon={MenuIcon} label="菜单" />
          <TitleBarBtn
            icon={SidebarIcon}
            label="折叠侧栏"
            onClick={toggleLeftSidebar}
          />
          <TitleBarBtn
            icon={SearchIcon}
            label="搜索 (Ctrl+K)"
            onClick={() => toggleSearch(true)}
          />
        </div>

        {/* 中间拖拽区域 — 自然继承 drag-region */}
        <div className="flex-1 h-full" />

        {/* 右侧窗口控制 */}
        <div className="no-drag">
          <WindowControls />
        </div>
      </div>
      {/* Sidebar — 浮起卡片:圆角 + 边框 + 毛玻璃 + 阴影 */}
      <div
        style={{ gridArea: "sidebar" }}
        className={
          leftSidebar === "expanded"
            ? "relative overflow-hidden rounded-lg m-1.5 border border-white/[0.12] shadow-lg backdrop-blur-sm"
            : ""
        }
      >
        {leftSidebar === "expanded" && (
          <>
            <div className="absolute inset-0 bg-elevated opacity-[0.97]" />
            {isSettings ? <SettingsSidebar /> : <ChatSidebar />}
            <ResizeHandle
              axis="x"
              side="right"
              current={sidebarW}
              onResize={setSidebarWidth}
            />
          </>
        )}
      </div>

      {/* Main */}
      <div
        style={{ gridArea: "main" }}
        className="overflow-hidden bg-canvas min-w-0 min-h-0"
      >
        {children}
      </div>

      {/* Bottom panel */}
      <div
        style={{ gridArea: "bottom" }}
        className="overflow-hidden min-h-0"
      >
        <BottomPanel />
      </div>

      {/* Right panel — 无 border-l */}
      <div
        style={{ gridArea: "right" }}
        className="overflow-hidden min-h-0"
      >
        <RightPanel />
      </div>
    </div>
  );
}

function TitleBarBtn({
  icon,
  label,
  onClick,
}: {
  icon: PhIcon;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className="h-8 w-8 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors"
    >
      <Icon icon={icon} size={15} />
    </button>
  );
}
