import { useUiStore } from "@/stores/uiStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { PanelTitle, Section, Row } from "./_shared";
import { Toggle } from "@/shared/ui/Toggle";
import { Pill } from "@/shared/ui/Pill";
import { Icon } from "@/shared/icons/Icon";
import { SunIcon, MoonIcon, SystemIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import type { ThemeMode } from "@/api";

export function GeneralPanel() {
  const theme = useUiStore((s) => s.theme);
  const setTheme = useUiStore((s) => s.setTheme);
  const showBalance = useUiStore((s) => s.showBalanceInSidebar);
  const setShowBalance = useUiStore((s) => s.setShowBalanceInSidebar);
  const showMessageCost = useUiStore((s) => s.showMessageCost);
  const setShowMessageCost = useUiStore((s) => s.setShowMessageCost);

  // 行为设置 — 来自 settingsStore (真实持久化)
  const ui = useSettingsStore((s) => s.ui);
  const updateUi = useSettingsStore((s) => s.updateUi);
  const updateSettings = useSettingsStore((s) => s.update);

  function updateTheme(value: ThemeMode) {
    setTheme(value);
    updateSettings({ theme: value });
  }

  return (
    <div>
      <PanelTitle
        title="通用"
        description="界面主题、语言、字体等基础配置"
      />

      <Section title="外观">
        <Row
          label="主题"
          description="跟随系统时根据 OS 偏好切换"
          control={<ThemeSwitcher value={theme} onChange={updateTheme} />}
        />
        <Row
          label="界面语言"
          description="侧栏、菜单、按钮文案"
          control={<Pill tone="info">可用</Pill>}
        />
        <Row
          label="对话字号"
          description="影响消息正文阅读舒适度"
          control={<Pill tone="info">可用</Pill>}
        />
      </Section>

      <Section title="行为">
        <Row
          label="Enter 直接发送"
          description="关闭后改为 Ctrl+Enter 发送, Enter 换行"
          control={
            <Toggle
              checked={ui.enterToSend}
              onChange={(v) => updateUi({ enterToSend: v })}
              label="Enter 发送"
            />
          }
        />
        <Row
          label="自动滚动到底部"
          description="新消息出现时自动滚到最新, 手动上滚后暂停"
          control={
            <Toggle
              checked={ui.autoScroll}
              onChange={(v) => updateUi({ autoScroll: v })}
              label="自动滚动"
            />
          }
        />
        <Row
          label="完成后折叠思维链"
          description="助手回答结束后自动折叠 reasoning 区域"
          control={
            <Toggle
              checked={ui.collapseReasoningOnComplete}
              onChange={(v) => updateUi({ collapseReasoningOnComplete: v })}
              label="折叠思维链"
            />
          }
        />
      </Section>

      <Section title="账户与余额">
        <Row
          label="在主页显示余额"
          description="开启时侧栏底部账户卡显示当前余额, 关闭后仅在 Settings → 用量与计费 查看"
          control={
            <Toggle
              checked={showBalance}
              onChange={setShowBalance}
              label="侧栏显示余额"
            />
          }
        />
        <Row
          label="在每条消息底部显示成本"
          description="默认关闭. 开启后每条 assistant 消息底部显示该 turn 的 CNY 成本 (后端 USD × 静态汇率)"
          control={
            <Toggle
              checked={showMessageCost}
              onChange={setShowMessageCost}
              label="消息成本徽章"
            />
          }
        />
      </Section>
    </div>
  );
}

function ThemeSwitcher({
  value,
  onChange,
}: {
  value: ThemeMode;
  onChange: (m: ThemeMode) => void;
}) {
  const opts: Array<{ id: ThemeMode; label: string; icon: typeof SunIcon }> = [
    { id: "light", label: "亮色", icon: SunIcon },
    { id: "system", label: "系统", icon: SystemIcon },
    { id: "dark", label: "暗色", icon: MoonIcon },
  ];
  return (
    <div className="inline-flex p-0.5 bg-canvas border border-border-subtle rounded-md">
      {opts.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={cn(
            "h-7 px-2.5 text-xs rounded-[6px] inline-flex items-center gap-1 transition-colors focus-ring",
            value === o.id
              ? "bg-elevated text-text-primary"
              : "text-text-tertiary hover:text-text-secondary",
          )}
        >
          <Icon icon={o.icon} size={12} />
          {o.label}
        </button>
      ))}
    </div>
  );
}
