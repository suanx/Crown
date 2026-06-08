import { useRouterStore } from "@/stores/routerStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import { IconButton } from "@/shared/ui/IconButton";
import {
  MenuIcon,
  SidebarIcon,
  SearchIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

/**
 * 顶栏 — 极简,贴齐 Codex / Claude Code 桌面端.
 *
 *   ┌───────────────────────────────────────────────────────┐
 *   │ ☰ ⊟ 🔍  drag region  [底部▢] [右侧⊟]   [Tauri windowctl]│
 *   └───────────────────────────────────────────────────────┘
 *
 * 不画 Win 风窗口控件 — Tauri 后端接管,prototype 留 90px 占位.
 */
export function TopBar() {
  const toggleLeftSidebar = useWorkspaceStore((s) => s.toggleLeftSidebar);
  const toggleSearch = useRouterStore((s) => s.toggleSearch);

  const rightContent = useWorkspaceStore((s) => s.rightContent);
  const bottomContent = useWorkspaceStore((s) => s.bottomContent);
  const toggleRightPanel = useWorkspaceStore((s) => s.toggleRightPanel);
  const toggleBottomPanel = useWorkspaceStore((s) => s.toggleBottomPanel);

  return (
    <div className="h-full flex items-stretch drag-region">
      {/* 左侧操作 */}
      <div className="flex items-center gap-1 px-2 shrink-0">
        <IconButton icon={MenuIcon} label="菜单" />
        <IconButton
          icon={SidebarIcon}
          label="折叠侧栏 (Ctrl+B)"
          onClick={toggleLeftSidebar}
        />
        <IconButton
          icon={SearchIcon}
          label="搜索 (Ctrl+K)"
          onClick={() => toggleSearch(true)}
        />
      </div>

      {/* 中段 drag region */}
      <div className="flex-1" />

      {/* 面板切换 — 贴右上,Tauri 窗口控件由 OS 在 webview 之外原生绘制 */}
      <div className="flex items-center gap-1 pr-2 pl-1 shrink-0">
        <PanelToggleBtn
          label="切换底部面板"
          shortcut="Ctrl+J"
          active={!!bottomContent}
          onClick={toggleBottomPanel}
          icon="bottom"
        />
        <PanelToggleBtn
          label="切换右侧面板"
          shortcut="Ctrl+Alt+B"
          active={!!rightContent}
          onClick={toggleRightPanel}
          icon="right"
        />
      </div>
    </div>
  );
}

/**
 * 面板切换按钮 — 自绘 SVG icon 模拟"主区右侧/底部高亮"的状态指示.
 * VS Code 同款视觉. 28x28 命中区,内部 14x14 icon.
 */
function PanelToggleBtn({
  label,
  shortcut,
  active,
  onClick,
  icon,
}: {
  label: string;
  shortcut: string;
  active: boolean;
  onClick: () => void;
  icon: "right" | "bottom";
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={`${label} (${shortcut})`}
      className={cn(
        "h-8 w-8 rounded-md flex items-center justify-center transition-colors focus-ring no-drag",
        active
          ? "bg-hover text-text-primary"
          : "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
    >
      <PanelGlyph kind={icon} active={active} />
    </button>
  );
}

function PanelGlyph({
  kind,
  active,
}: {
  kind: "right" | "bottom";
  active: boolean;
}) {
  // 14x14 命中盒,内部 12x12 矩形 + 子分区
  const stroke = "currentColor";
  const fill = active ? "currentColor" : "transparent";
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 14 14"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <rect
        x="1"
        y="1"
        width="12"
        height="12"
        rx="1.5"
        stroke={stroke}
        strokeWidth="1.2"
      />
      {kind === "right" ? (
        <rect x="9" y="1" width="4" height="12" rx="1.5" fill={fill} stroke={stroke} strokeWidth="1.2" />
      ) : (
        <rect x="1" y="9" width="12" height="4" rx="1.5" fill={fill} stroke={stroke} strokeWidth="1.2" />
      )}
    </svg>
  );
}

// (no extra exports)
