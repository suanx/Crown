import { Icon } from "@/shared/icons/Icon";
import { CodeIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import type { SlashCommand } from "./slashCommands";

/**
 * 斜杠命令补全弹层。显示在 ComposeBar 上方。
 * 受控：父组件传 commands（已过滤）+ activeIndex + 选择回调。
 */
export function SlashCommandMenu({
  commands,
  activeIndex,
  onSelect,
}: {
  commands: SlashCommand[];
  activeIndex: number;
  onSelect: (cmd: SlashCommand) => void;
}) {
  if (commands.length === 0) return null;
  return (
    <div className="mb-2 rounded-xl border border-border-subtle bg-overlay shadow-lg overflow-hidden">
      {commands.map((c, i) => (
        <button
          key={c.name}
          onMouseDown={(e) => {
            e.preventDefault(); // 防 textarea 失焦
            onSelect(c);
          }}
          className={cn(
            "w-full flex items-start gap-2 px-3 py-2 text-left transition-colors",
            i === activeIndex ? "bg-hover" : "hover:bg-hover",
          )}
        >
          <Icon
            icon={CodeIcon}
            size={14}
            className="text-text-tertiary mt-0.5 shrink-0"
          />
          <div className="min-w-0">
            <div className="text-sm font-medium text-text-primary">/{c.name}</div>
            <div className="text-xs text-text-secondary truncate">
              {c.description}
            </div>
          </div>
        </button>
      ))}
    </div>
  );
}
