import { useState } from "react";
import { agentClient } from "@/api";
import { PanelTitle, Section, Row } from "./_shared";
import { useSettingsStore } from "@/stores/settingsStore";

export function WorkspacePanel() {
  const storeDir = useSettingsStore((s) => s.workspaceDir);
  const [dir, setDir] = useState(storeDir);

  const handlePick = async () => {
    const picked = await agentClient.pickProjectDirectory().catch(() => null);
    if (picked) setDir(picked);
  };

  const handleSave = async () => {
    await agentClient.setConfig({ workspaceDir: dir });
    useSettingsStore.getState().update({ workspaceDir: dir });
  };

  return (
    <div>
      <PanelTitle title="工作目录" />

      <Section>
        <Row
          label="当前工作目录"
          control={
            <input
              type="text"
              className="h-8 w-[280px] rounded-md border border-border-default bg-input-bg px-3 text-sm text-text-primary outline-none focus:border-border-focus"
              value={dir}
              onInput={(e) => setDir((e.target as HTMLInputElement).value)}
              placeholder="C:/Projects 或 /home/user/projects"
            />
          }
        />
        <Row
          label="操作"
          control={
            <div className="flex items-center gap-2">
              <button
                className="h-8 rounded-md px-3 text-xs border border-border-default bg-surface text-text-primary hover:bg-hover focus-ring"
                onClick={handlePick}
              >
                选择目录…
              </button>
              <button
                className="h-8 rounded-md px-3 text-xs bg-brand text-white hover:opacity-90 focus-ring"
                onClick={handleSave}
              >
                保存
              </button>
            </div>
          }
        />
      </Section>
    </div>
  );
}
