import { PanelTitle, Section } from "./_shared";

const SHORTCUTS = [
  { label: "新对话", binding: "Ctrl+N" },
  { label: "搜索对话", binding: "Ctrl+K" },
  { label: "折叠侧栏", binding: "Ctrl+B" },
  { label: "停止生成", binding: "Esc" },
  { label: "在新窗口打开当前对话", binding: "Ctrl+Shift+O" },
  { label: "打开开发者面板", binding: "Ctrl+Shift+D" },
];

export function ShortcutsPanel() {
  return (
    <div>
      <PanelTitle title="快捷键" description="常用操作的键盘绑定" />

      <Section>
        {SHORTCUTS.map((s) => (
          <div
            key={s.label}
            className="px-4 py-3 flex items-center justify-between"
          >
            <span className="text-sm text-text-primary">{s.label}</span>
            <Kbd>{s.binding}</Kbd>
          </div>
        ))}
      </Section>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="inline-flex items-center px-2 h-6 rounded-md text-xs font-mono bg-canvas border border-border-default text-text-secondary">
      {children}
    </kbd>
  );
}
