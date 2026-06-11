import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";
import { BrandLogo } from "@/shared/icons/BrandLogo";
import { agentClient } from "@/api";
import { useState } from "react";
import { DownloadSimple } from "@phosphor-icons/react";

export function AboutPanel() {
  const [exporting, setExporting] = useState(false);

  async function handleExportMemory() {
    setExporting(true);
    try {
      const content = await agentClient.readGlobalMemory();
      const blob = new Blob([content], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "AGENTS.md";
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } finally {
      setExporting(false);
    }
  }

  return (
    <div>
      <PanelTitle title="关于" />

      <div className="rounded-lg border border-border-subtle bg-elevated p-6 mb-6 flex items-center gap-4">
        <span className="text-brand">
          <BrandLogo size={36} />
        </span>
        <div className="flex-1 min-w-0">
          <div className="text-base font-semibold text-text-primary">
            Crown
          </div>
          <div className="text-xs text-text-tertiary mt-0.5 font-mono">
            v1.3.3
          </div>

        </div>
        <Pill tone="success">已是最新版本</Pill>
      </div>

      <Section title="资源">
        <Row
          label="项目主页"
          description="GitHub repo"
          control={<Pill tone="info">可用</Pill>}
        />
        <Row
          label="许可证"
          description="MIT"
          control={<Pill tone="info">可用</Pill>}
        />
        <Row
          label="第三方依赖"
          description="React · Tailwind · Phosphor · Tauri"
          control={<Pill tone="info">可用</Pill>}
        />
      </Section>

      <Section title="数据">
        <Row
          label="导出长期记忆"
          description="下载 AGENTS.md 文件备份到本地"
          control={
            <button
              onClick={handleExportMemory}
              disabled={exporting}
              className="h-8 px-3 text-xs rounded-md inline-flex items-center gap-1.5
                         bg-elevated text-text-primary border border-border-default
                         hover:bg-hover-bg hover:border-border-strong
                         disabled:opacity-50 disabled:cursor-not-allowed
                         transition-colors focus-ring"
            >
              <DownloadSimple size={14} weight="regular" />
              {exporting ? "导出中…" : "导出记忆"}
            </button>
          }
        />
      </Section>
    </div>
  );
}
