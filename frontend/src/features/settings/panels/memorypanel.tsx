import { useCallback, useEffect, useState } from "react";
import { agentClient } from "@/api";
import { PanelTitle, Section } from "./_shared";
import { Button } from "@/shared/ui/Button";
import { DownloadIcon } from "@/shared/icons/set";

export function MemoryPanel() {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const mem = await agentClient.readGlobalMemory();
      setContent(mem);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await agentClient.writeGlobalMemory(content);
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleExport() {
    const blob = new Blob([content], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "AGENTS.md";
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div>
      <PanelTitle
        title="长期记忆"
        description="全局 AGENTS.md — 跨对话持久指令，让 Agent 记住你的偏好和项目背景"
      />

      <Section title="编辑记忆">
        <div className="space-y-3">
          <textarea
            value={content}
            onChange={(e) => { setContent(e.target.value); setDirty(true); }}
            disabled={loading}
            placeholder={loading ? "加载中..." : "# 在此编写你的长期记忆指令...\n\n例如：\n- 我使用 TypeScript + React\n- 代码中优先使用函数式组件\n- 测试使用 vitest"}
            className="w-full h-[300px] rounded-lg border border-border-default bg-input-bg p-3 text-sm font-mono text-text-primary placeholder:text-text-tertiary resize-y outline-none focus:border-border-focus transition-colors disabled:opacity-50"
            spellCheck={false}
          />
          <div className="flex items-center gap-2">
            <Button
              variant="primary"
              disabled={!dirty || saving || loading}
              onClick={() => void save()}
            >
              {saving ? "保存中..." : "保存"}
            </Button>
            <Button
              variant="secondary"
              disabled={loading}
              icon={DownloadIcon}
              onClick={handleExport}
            >
              导出为文件
            </Button>
            <Button
              variant="ghost"
              disabled={loading}
              onClick={() => void load()}
            >
              撤销更改
            </Button>
          </div>
          {error && (
            <div className="text-sm text-danger break-all">{error}</div>
          )}
          <div className="text-xs text-text-tertiary leading-relaxed pt-2 border-t border-border-subtle">
            <p><strong>💡 使用技巧：</strong></p>
            <ul className="list-disc pl-4 mt-1 space-y-1">
              <li>告诉 Agent 你的技术栈偏好、编码规范</li>
              <li>列出常用的项目约定和命名规则</li>
              <li>写下你希望 Agent 每次对话都记住的事情</li>
              <li>支持 Markdown 格式</li>
            </ul>
          </div>
        </div>
      </Section>
    </div>
  );
}
