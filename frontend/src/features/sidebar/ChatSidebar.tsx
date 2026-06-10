import { useRouterStore } from "@/stores/routerStore";
import { useBalanceStore } from "@/stores/balanceStore";
import { useUiStore } from "@/stores/uiStore";
import { Icon } from "@/shared/icons/Icon";
import {
  PlusIcon,
  SearchIcon,
  SkillIcon,
  SettingsIcon,
} from "@/shared/icons/set";
import { ProjectSessionList } from "./ProjectSessionList";
import { balanceTone, formatBalance } from "@/shared/lib/format";
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

      {/* 底部用户/设置 */}
      <BottomBar />
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

function BottomBar() {
  const navigate = useRouterStore((s) => s.navigate);
  const showBalance = useUiStore((s) => s.showBalanceInSidebar);
  const balance = useBalanceStore((s) => s.balance);

  // 主页可见的余额副标题:开关开启 + 后端真返了 balance + 至少一条币种
  const balanceLine = (() => {
    if (!showBalance) return null;
    if (!balance || !balance.isAvailable) return null;
    if (balance.balanceInfos.length === 0) return null;
    const primary =
      balance.balanceInfos.find((b) => b.currency === balance.primaryCurrency) ??
      balance.balanceInfos[0];
    return {
      text: formatBalance(primary.currency, primary.total),
      tone: balanceTone(primary.total),
    };
  })();

  return (
    <div className="h-12 flex items-center shrink-0 px-2 gap-2">
      <button
        onClick={() => navigate({ page: "settings", tab: "billing" })}
        title={
          balanceLine
            ? `余额 ${balanceLine.text} · 点击查看详情`
            : "设置 · 用量与计费"
        }
        className="flex items-center gap-2 rounded-md hover:bg-hover transition-colors focus-ring flex-1 px-2 h-8"
      >
        <div className="h-7 w-7 rounded-full bg-brand-soft text-brand flex items-center justify-center text-xs font-semibold shrink-0">
          ME
        </div>
        <div className="flex-1 min-w-0 text-left">
          <div className="text-sm text-text-primary truncate leading-tight">
            本地
          </div>
          <div
            className={cn(
              "text-xs truncate leading-tight tabular-nums",
              balanceLine?.tone === "danger" && "text-danger",
              balanceLine?.tone === "warning" && "text-warning",
              (!balanceLine || balanceLine.tone === "success") &&
                "text-text-tertiary",
            )}
          >
            {balanceLine ? balanceLine.text : ""}
          </div>
        </div>
      </button>
      <button
        onClick={() => navigate({ page: "settings", tab: "general" })}
        aria-label="设置"
        className="h-8 w-8 rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring flex items-center justify-center"
      >
        <Icon icon={SettingsIcon} size={14} />
      </button>
    </div>
  );
}
