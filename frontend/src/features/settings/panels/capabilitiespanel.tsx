import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";

export function CapabilitiesPanel() {
  return (
    <div>
      <PanelTitle
        title="能力"
        description="控制 Agent 的工具与高级特性开关"
      />

      <Section title="推理">
        <Row
          label="启用思维链 (reasoning)"
          description="V4-Pro 模型生成 reasoning_content,流式期间展示"
          control={<Pill tone="success">已接入</Pill>}
        />
        <Row
          label="自动升级到 Pro"
          description="检测到 <<<NEEDS_PRO>>> 信号时自动切换 (实验性)"
          control={<Pill tone="warning">可用</Pill>}
        />
      </Section>

      <Section title="自动压缩">
        <Row
          label="超过阈值时自动 compact"
          description="当 context 占用 ≥ 70% 时合并历史 turn"
          control={<Pill tone="warning">可用</Pill>}
        />
        <Row
          label="保留最近 turn 数"
          description="自动压缩时不动最近 N 个完整 turn"
          control={
            <select disabled defaultValue="3" className="h-8 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none opacity-50 cursor-not-allowed">
              <option>2</option>
              <option>3</option>
              <option>5</option>
            </select>
          }
        />
      </Section>

      <Section title="代码执行">
        <Row
          label="允许执行 Shell 命令"
          description="关闭后 run_command 工具完全不可用"
          control={<Pill tone="success">已接入</Pill>}
        />
        <Row
          label="允许网络请求"
          description="web_search / web_fetch / npm install 等"
          control={<Pill tone="success">已接入</Pill>}
        />
      </Section>
    </div>
  );
}
