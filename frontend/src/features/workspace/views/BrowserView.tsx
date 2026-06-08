import { PanelHeader } from "../PanelHeader";
import { Icon } from "@/shared/icons/Icon";
import { GlobeIcon, RefreshIcon, ArrowRightIcon } from "@/shared/icons/set";

export interface BrowserViewProps {
  slot: "right" | "bottom";
}

export function BrowserView({ slot }: BrowserViewProps) {
  return (
    <div className="h-full flex flex-col">
      <PanelHeader slot={slot} kind="browser" />
      <div className="px-3 py-2 border-b border-border-subtle flex items-center gap-2 shrink-0">
        <button className="h-7 w-7 rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors flex items-center justify-center focus-ring" title="刷新">
          <Icon icon={RefreshIcon} size={13} />
        </button>
        <input
          defaultValue="http://localhost:5173"
          className="flex-1 h-7 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none focus:border-border-focus font-mono"
        />
        <button className="h-7 w-7 rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors flex items-center justify-center focus-ring" title="打开">
          <Icon icon={ArrowRightIcon} size={13} />
        </button>
      </div>
      <div className="flex-1 min-h-0 flex items-center justify-center text-text-tertiary text-sm">
        <div className="flex flex-col items-center gap-2">
          <Icon icon={GlobeIcon} size={32} weight="duotone" className="opacity-40" />
          <span>预览将在此处加载</span>
          <span className="text-xs">原型阶段不渲染实际网页</span>
        </div>
      </div>
    </div>
  );
}
