import { useRouterStore } from "@/stores/routerStore";
import { Icon } from "@/shared/icons/Icon";
import {
  PlusIcon,
  SearchIcon,
  SkillIcon,
  SettingsIcon,
} from "@/shared/icons/set";
import { ProjectSessionList } from "./ProjectSessionList";
import { cn } from "@/shared/lib/cn";

/**
 * 聊天/项目模式下的 Sidebar.
 *
 *   ┌────────────────────────────┐
 *   │ + 新建对话                  │  brand h-9
 *   │ 🔍 搜索  ⌘K                  │  h-8
 *   │ ✨ 技能                      │
 *   │ ─────────                    │
 *   │ 项目分组列表 (flex-1)         │
 *   │ 📁 deepseek-agent ▾          │
 *   │   · Rust HTTP 搭建            │
 *   │   · 前端 hook 闭包            │
 *   │ 📁 blog-2026     ▸           │
 *   │ ─ 无项目                       │
 *   │   · 计算机大赛 PPT             │
 *   │ ─────────                    │
 *   │ [ME] 本地         [⚙]        │  bottom h-12
 *   └────────────────────────────┘
 */
export function ChatSidebar() {
  const navigate = useRouterStore((s) => s.navigate);
  const toggleSearch = useRouterStore((s) => s.toggleSearch);
  const route = useRouterStore((s) => s.current);

  const isSkills = route.page === "skills";

  function handleNewThread() {
    navigate({ page: "welcome" });
  }

  return (
    <div className="h-full flex flex-col relative">
      {/* 顶部间距 — 跟 Claude 对齐,给标题栏预留视觉呼吸空间 */}
      <div className="h-4 shrink-0" />

      {/* 顶部主操作 + 导航 */}
      <div className="space-y-1 shrink-0 px-2">
        <button
          onClick={() => void handleNewThread()}
          className={cn(
            "w-full flex items-center h-9 rounded-md text-base font-medium transition-colors focus-ring",
            "bg-[#3D3D3D] text-white border border-white/[0.12] hover:bg-[#4A4A4A]",
            "gap-2 px-3",
          )}
        >
          <Icon icon={PlusIcon} size={16} weight="bold" />
          <span>新建对话</span>
        </button>

        <NavBtn
          icon={SearchIcon}
          label="搜索"
          shortcut="⌘K"
          onClick={() => toggleSearch(true)}
        />
        <NavBtn
          icon={SkillIcon}
          label="技能"
          active={isSkills}
          onClick={() => navigate({ page: "skills" })}
        />
      </div>

      {/* 项目分组的对话列表 */}
      <div className="flex-1 min-h-0 flex flex-col mt-3">
        <ProjectSessionList />
      </div>
      {/* 底部设置 */}
      <div className="h-12 flex items-center shrink-0 justify-end px-2">
        <button
          onClick={() => navigate({ page: "settings", tab: "general" })}
          aria-label="设置"
          className="h-8 w-8 rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring flex items-center justify-center"
        >
          <Icon icon={SettingsIcon} size={14} />
        </button>
      </div>
    </div>
  );
}


function NavBtn({
  icon,
  label,
  shortcut,
  active,
  onClick,
}: {
  icon: typeof PlusIcon;
  label: string;
  shortcut?: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-full flex items-center h-8 px-3 gap-2 rounded-md text-sm transition-colors focus-ring",
        active
          ? "bg-hover text-text-primary font-medium"
          : "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
    >
      <Icon icon={icon} size={14} />
      <span className="truncate flex-1 text-left">{label}</span>
      {shortcut && (
        <span className="text-xs font-mono text-text-tertiary">{shortcut}</span>
      )}
    </button>
  );
}