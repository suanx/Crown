import { useEffect, useState } from "react";
import { agentClient, type PermissionMode, type PermissionRule } from "@/api";
import { useActiveThread } from "@/stores/chatStore";
import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";
import { Icon } from "@/shared/icons/Icon";
import {
  ShieldIcon,
  AgentIcon,
  WarningIcon,
  TrashIcon,
} from "@/shared/icons/set";
import {
  PERMISSION_MODE_DESCRIPTIONS,
  PERMISSION_MODE_LABELS,
} from "@/shared/lib/permissionMode";

const MODE_OPTIONS: PermissionMode[] = [
  "default",
  "plan",
  "acceptEdits",
  "bypassPermissions",
  "dontAsk",
];

export function PermissionsPanel() {
  const thread = useActiveThread();
  const [rules, setRules] = useState<PermissionRule[]>([]);

  useEffect(() => {
    if (!thread) return;
    void agentClient.listPermissionRules(thread.id).then(setRules);
  }, [thread]);

  async function removeRule(rule: PermissionRule) {
    if (!thread) return;
    setRules((rs) => rs.filter((r) => r !== rule));
    try {
      await agentClient.removePermissionRule(thread.id, rule);
    } catch {
      // fallback 已处理
    }
  }

  return (
    <div>
      <PanelTitle title="权限" description="控制 Agent 在你机器上能做什么" />

      <Section title="新建会话默认模式">
        <Row
          label="默认模式"
          description="影响新建 thread 的初始 permissionMode (当前 thread 切换在 ComposeBar 右下角)"
          control={
            <select
              defaultValue="default"
              disabled
              className="h-8 px-2 rounded-md text-sm bg-input-bg border border-border-default text-text-primary outline-none opacity-50 cursor-not-allowed"
            >
              {MODE_OPTIONS.map((m) => (
                <option key={m} value={m}>
                  {PERMISSION_MODE_LABELS[m]}
                </option>
              ))}
            </select>
          }
        />
      </Section>

      <Section title="模式说明">
        <Row
          label="Agent (default)"
          description={PERMISSION_MODE_DESCRIPTIONS.default}
          control={
            <Pill tone="brand" icon={AgentIcon}>
              推荐
            </Pill>
          }
        />
        <Row
          label="Plan"
          description={PERMISSION_MODE_DESCRIPTIONS.plan}
          control={
            <Pill tone="neutral" icon={ShieldIcon}>
              安全
            </Pill>
          }
        />
        <Row
          label="YOLO (bypassPermissions)"
          description={PERMISSION_MODE_DESCRIPTIONS.bypassPermissions}
          control={
            <Pill tone="danger" icon={WarningIcon}>
              高风险
            </Pill>
          }
        />
      </Section>

      <Section title="本会话已记住的规则">
        {rules.length === 0 ? (
          <Row
            label="暂无规则"
            description='当你点 "始终允许" 时,规则会出现在这里. 撤销后下次同名工具会重新询问.'
            control={null}
          />
        ) : (
          <div className="divide-y divide-border-subtle">
            {rules.map((rule, i) => (
              <div
                key={`${rule.ruleValue.toolName}-${i}`}
                className="px-4 py-3 flex items-center justify-between gap-4"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-text-primary font-mono">
                    {rule.ruleValue.toolName}
                    {rule.ruleValue.ruleContent && (
                      <span className="text-text-tertiary">
                        {" "}({rule.ruleValue.ruleContent})
                      </span>
                    )}
                  </div>
                  <div className="text-xs text-text-tertiary">
                    {rule.source} · {rule.ruleBehavior}
                  </div>
                </div>
                <button
                  onClick={() => removeRule(rule)}
                  aria-label="撤销规则"
                  title="撤销"
                  className="h-8 w-8 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-danger transition-colors focus-ring"
                >
                  <Icon icon={TrashIcon} size={14} />
                </button>
              </div>
            ))}
          </div>
        )}
      </Section>

      <Section title="审批行为">
        <Row
          label="审批超时自动拒绝"
          description="60 秒未响应视为拒绝,避免阻塞"
          control={<Pill tone="info">可用</Pill>}
        />
      </Section>
    </div>
  );
}
