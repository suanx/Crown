import { Icon } from "@/shared/icons/Icon";
import {
  FolderIcon,
  GlobeIcon,
  DiffIcon,
  TerminalIcon,
  CaretRightIcon,
} from "@/shared/icons/set";
import { useWorkspaceStore, type PanelKind } from "@/stores/workspaceStore";
import type { Icon as PhIcon } from "@phosphor-icons/react";
import { cn } from "@/shared/lib/cn";

/**
 * 空 dock 时的"内容选择器" — 横条紧凑卡片 (方案 A).
 *
 * Codex 用大正方形,占地大;我们用 64px 高横条,信息密度更高.
 * 4 项内容: 文件 / 浏览器 / 审查 / 终端. 不放技能.
 */

interface PanelOption {
  kind: PanelKind;
  label: string;
  description: string;
  icon: PhIcon;
}

const OPTIONS: PanelOption[] = [
  { kind: "files", label: "文件", description: "浏览项目文件树", icon: FolderIcon },
  { kind: "browser", label: "浏览器", description: "打开网站预览", icon: GlobeIcon },
  { kind: "review", label: "审查", description: "查看代码更改 diff", icon: DiffIcon },
  { kind: "terminal", label: "终端", description: "启动交互式 shell", icon: TerminalIcon },
];

export interface PanelEmptyProps {
  slot: "right" | "bottom";
}

export function PanelEmpty({ slot }: PanelEmptyProps) {
  const openInRight = useWorkspaceStore((s) => s.openInRight);
  const openInBottom = useWorkspaceStore((s) => s.openInBottom);
  const open = slot === "right" ? openInRight : openInBottom;

  return (
    <div className="h-full flex flex-col items-center justify-center px-6">
      <div className="w-full max-w-[420px] space-y-2">
        <div className="text-xs text-text-tertiary mb-3 px-1">
          选择内容打开
        </div>
        {OPTIONS.map((opt) => (
          <PanelCard key={opt.kind} option={opt} onClick={() => open(opt.kind)} />
        ))}
      </div>
    </div>
  );
}

function PanelCard({
  option,
  onClick,
}: {
  option: PanelOption;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "group w-full h-16 px-4 flex items-center gap-3 rounded-lg border border-border-subtle bg-elevated",
        "hover:border-border-default hover:bg-hover transition-colors focus-ring text-left",
      )}
    >
      <div className="h-10 w-10 rounded-md bg-canvas border border-border-subtle text-text-secondary flex items-center justify-center shrink-0 group-hover:text-brand group-hover:border-brand-soft transition-colors">
        <Icon icon={option.icon} size={18} weight="duotone" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-text-primary">
          {option.label}
        </div>
        <div className="text-xs text-text-tertiary">{option.description}</div>
      </div>
      <Icon
        icon={CaretRightIcon}
        size={14}
        className="text-text-tertiary group-hover:text-text-secondary shrink-0"
      />
    </button>
  );
}
