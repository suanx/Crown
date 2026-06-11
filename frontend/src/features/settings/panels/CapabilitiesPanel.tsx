import { useEffect, useState } from "react";
import { agentClient, type AppConfig } from "@/api";
import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";

const MAX_SUBTASK_OPTIONS = [1, 2, 3, 5, 8, 10, 15, 20];

export function CapabilitiesPanel() {
  const [maxSubtasks, setMaxSubtasks] = useState(5);
  const [model, setModel] = useState("");

  useEffect(() => {
    let cancelled = false;
    void agentClient
      .getConfig()
      .then((cfg: AppConfig) => {
        if (cancelled) return;
        setMaxSubtasks(cfg.subagent?.maxSubtasks ?? 5);
        setModel(cfg.subagent?.model ?? "");
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  async function save() {
    await agentClient.setConfig({
      subagent: { maxSubtasks, model },
    });
  }

  return (
    <div>
      <PanelTitle title="能力" description="控制 Agent 的工具与高级特性开关" />

      <Section title="推理">
        <Row label="启用思维链 (reasoning)" description="模型生成 reasoning_content，流式期间展示" control={<Pill tone="success">已接入</Pill>} />
        <Row label="自动升级到 Pro" description="检测到 <<NEEDS_PRO>> 信号时自动切换 (实验性)" control={<Pill tone="warning">可用</Pill>} />
      </Section>

      <Section title="自动压缩">
        <Row label="超过阈值时自动 compact" description="当 context 占用 ≥ 70% 时合并历史 turn" control={<Pill tone="warning">可用</Pill>} />
        <Row label="保留最近 turn 数" description="自动压缩时不动最近 N 个完整 turn" control={
          <select disabled defaultValue="3" className="h-8 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none opacity-50 cursor-not-allowed"><option>2</option><option>3</option><option>5</option></select>
        } />
      </Section>

      <Section title="子代理 (Sub-agent)">
        <Row label="最大子任务数" description="一次最多允许派发的子代理任务数量" control={
          <select value={maxSubtasks} onChange={(e) => { setMaxSubtasks(Number(e.target.value)); void save(); }} className="h-8 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none focus:border-border-focus">
            {MAX_SUBTASK_OPTIONS.map((n) => (<option key={n} value={n}>{n}</option>))}
          </select>
        } />
        <Row label="子代理模型" description="子代理使用的模型。留空则继承当前对话模型" control={
          <select value={model} onChange={(e) => { setModel(e.target.value); void save(); }} className="h-8 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none focus:border-border-focus max-w-[220px]">
            <option value="">继承对话模型 (默认)</option>
            <option value="deepseek-v4-flash">DeepSeek V4 Flash</option>
            <option value="deepseek-v4-pro">DeepSeek V4 Pro</option>
            <option value="gpt-4o">GPT-4o</option>
            <option value="claude-3-5-sonnet">Claude 3.5 Sonnet</option>
          </select>
        } />
      </Section>

      <Section title="代码执行">
        <Row label="允许执行 Shell 命令" description="关闭后 run_command 工具完全不可用" control={<Pill tone="success">已接入</Pill>} />
        <Row label="允许网络请求" description="web_search / web_fetch / npm install 等" control={<Pill tone="success">已接入</Pill>} />
      </Section>
    </div>
  );
}
