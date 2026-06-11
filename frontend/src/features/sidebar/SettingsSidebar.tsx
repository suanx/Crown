import { useRouterStore, type SettingsTab } from "@/stores/routerStore";
import { useSessionStore } from "@/stores/sessionStore";
import { Icon } from "@/shared/icons/Icon";
import { cn } from "@/shared/lib/cn";

import {
  ArrowRightIcon,
  GlobeIcon,
  FlashIcon,
  ToolIcon,
  McpIcon,
  ShieldIcon,
  BuildIcon,
  SettingsIcon,
  BugIcon,
  DollarIcon,
  EditIcon,
  FileIcon,
  FolderIcon,
} from "@/shared/icons/set";
/**
 * 设置模式下的 Sidebar.
 * 整体替换 ChatSidebar (不是叠层),布局保持稳定.
 *
 * 顶部留 12px,接 "返回对话" 链接,然后两组 nav.
 */

const TABS: Array<{
  id: SettingsTab;
  label: string;
  icon: typeof GlobeIcon;
  group: "app" | "desktop";
}> = [
  { id: "general", label: "通用", icon: SettingsIcon, group: "app" },
  { id: "provider", label: "模型供应商", icon: GlobeIcon, group: "app" },
  { id: "models", label: "联网搜索", icon: FlashIcon, group: "app" },
  { id: "capabilities", label: "能力", icon: ToolIcon, group: "app" },
  { id: "outputStyles", label: "输出风格", icon: EditIcon, group: "app" },
  { id: "permissions", label: "权限", icon: ShieldIcon, group: "app" },
  { id: "mcp", label: "MCP 服务器", icon: McpIcon, group: "app" },
  { id: "hooks", label: "Hooks", icon: BuildIcon, group: "app" },
  { id: "billing", label: "用量统计", icon: DollarIcon, group: "app" },
  { id: "memory", label: "长期记忆", icon: FileIcon, group: "app" },
  { id: "workspace", label: "工作目录", icon: FolderIcon, group: "app" },
  { id: "developer", label: "开发者", icon: BugIcon, group: "desktop" },
];

export function SettingsSidebar() {
  const navigate = useRouterStore((s) => s.navigate);
  const route = useRouterStore((s) => s.current);
  const threads = useSessionStore((s) => s.threads);
  const currentTab = route.page === "settings" ? route.tab ?? "general" : "general";

  // 返回对话:有真 thread 用第一条;没有就回 welcome.不再硬编码 "thread-1"
  const goBack = () => {
    const first = threads[0];
    navigate(
      first ? { page: "chat", threadId: first.id } : { page: "welcome" },
    );
  };

  const appTabs = TABS.filter((t) => t.group === "app");
  const desktopTabs = TABS.filter((t) => t.group === "desktop");

  return (
    <div className="h-full flex flex-col relative">
      <div className="h-3 shrink-0" />

      {/* 返回 */}
      <div className="px-3 pb-2 shrink-0">
        <button
          onClick={goBack}
          className="text-xs text-text-tertiary hover:text-text-secondary inline-flex items-center gap-1 focus-ring rounded h-6"
        >
          <Icon icon={ArrowRightIcon} size={11} className="rotate-180" />
          返回对话
        </button>
      </div>

      <div className="flex-1 min-h-0 scrollable px-2 pb-3">
        <NavGroup title="应用">
          {appTabs.map((t) => (
            <NavItem
              key={t.id}
              icon={t.icon}
              label={t.label}
              active={t.id === currentTab}
              onClick={() => navigate({ page: "settings", tab: t.id })}
            />
          ))}
        </NavGroup>
        <NavGroup title="桌面端">
          {desktopTabs.map((t) => (
            <NavItem
              key={t.id}
              icon={t.icon}
              label={t.label}
              active={t.id === currentTab}
              onClick={() => navigate({ page: "settings", tab: t.id })}
            />
          ))}
        </NavGroup>
      </div>
    </div>
  );
}

function NavGroup({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-3">
      <div className="px-2 pt-3 pb-1 text-xs font-medium text-text-tertiary uppercase tracking-wide">
        {title}
      </div>
      <div className="space-y-0.5">{children}</div>
    </div>
  );
}

function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: typeof GlobeIcon;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full flex items-center gap-2 px-3 h-8 rounded-md text-sm transition-colors focus-ring text-left",
        active
          ? "bg-hover text-text-primary font-medium"
          : "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
    >
      <Icon icon={icon} size={14} />
      <span className="truncate">{label}</span>
    </button>
  );
}
