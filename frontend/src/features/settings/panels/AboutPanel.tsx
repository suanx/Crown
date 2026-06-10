import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";
import { BrandLogo } from "@/shared/icons/BrandLogo";

export function AboutPanel() {
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
            v1.3.0
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
    </div>
  );
}
