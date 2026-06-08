import type { SettingsTab } from "@/stores/routerStore";
import { GeneralPanel } from "./panels/GeneralPanel";
import { ModelsPanel } from "./panels/ModelsPanel";
import { WebSearchPanel } from "./panels/WebSearchPanel";
import { CapabilitiesPanel } from "./panels/CapabilitiesPanel";
import { OutputStylesPanel } from "./panels/OutputStylesPanel";
import { McpPanel } from "./panels/McpPanel";
import { PermissionsPanel } from "./panels/PermissionsPanel";
import { HooksPanel } from "./panels/HooksPanel";
import { BillingPanel } from "./panels/BillingPanel";
import { ShortcutsPanel } from "./panels/ShortcutsPanel";
import { DeveloperPanel } from "./panels/DeveloperPanel";
import { AboutPanel } from "./panels/AboutPanel";

export interface SettingsPageProps {
  tab: SettingsTab;
}

/**
 * 设置内容区 — 内 nav 已搬到 SettingsSidebar.
 * 这里只渲染 tab 对应的 panel.
 */
export function SettingsPage({ tab }: SettingsPageProps) {
  return (
    <div className="h-full scrollable">
      <div className="max-w-[680px] mx-auto px-8 py-8">
        {renderPanel(tab)}
      </div>
    </div>
  );
}

function renderPanel(tab: SettingsTab) {
  switch (tab) {
    case "general":
      return <GeneralPanel />;
    case "provider":
      return <ModelsPanel />;
    case "models":
      return <WebSearchPanel />;
    case "capabilities":
      return <CapabilitiesPanel />;
    case "outputStyles":
      return <OutputStylesPanel />;
    case "mcp":
      return <McpPanel />;
    case "permissions":
      return <PermissionsPanel />;
    case "hooks":
      return <HooksPanel />;
    case "billing":
      return <BillingPanel />;
    case "shortcuts":
      return <ShortcutsPanel />;
    case "developer":
      return <DeveloperPanel />;
    case "about":
      return <AboutPanel />;
  }
}
