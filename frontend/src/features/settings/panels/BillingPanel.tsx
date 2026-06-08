import { useEffect, useState } from "react";
import {
  agentClient,
  type UsageStats,
  type UsageStatsWindow,
} from "@/api";
import { useBalanceStore } from "@/stores/balanceStore";
import { PanelTitle, Section, Row } from "./_shared";
import { Pill } from "@/shared/ui/Pill";
import { Icon } from "@/shared/icons/Icon";
import { CheckCircleIcon, WarningIcon } from "@/shared/icons/set";
import {
  balanceTone,
  formatBalance,
  formatCostCny,
  formatTokens,
} from "@/shared/lib/format";
import { cn } from "@/shared/lib/cn";

const WINDOW_OPTIONS: Array<{ id: UsageStatsWindow; label: string }> = [
  { id: "session", label: "本会话" },
  { id: "today", label: "今天" },
  { id: "7d", label: "最近 7 天" },
  { id: "30d", label: "最近 30 天" },
  { id: "lifetime", label: "全部" },
];

const WINDOW_LABEL: Record<UsageStatsWindow, string> = Object.fromEntries(
  WINDOW_OPTIONS.map((o) => [o.id, o.label]),
) as Record<UsageStatsWindow, string>;

/**
 * 用量与计费 — P3a task 6 接通真后端 (UsageRepo + 多窗口聚合).
 *
 * 显示:
 *   - 大数 totalCostUsd + cumulativeCacheSavedUsd "省下" (DeepSeek 卖点)
 *   - 时间窗切换下拉 (5 档)
 *   - 4 列 token 详情
 *   - 余额 cell (来自 balanceStore,P3a task 7,失败时整块隐藏)
 *
 * P3a 不在范围:预算上限 / off-peak / NEEDS_PRO 升级 — 这些 UI 之前是装饰
 * 控件,在数据真实化的当下移除避免误导;实做时再回填.
 */
export function BillingPanel() {
  const [window, setWindow] = useState<UsageStatsWindow>("session");
  const [stats, setStats] = useState<UsageStats | null>(null);
  const balance = useBalanceStore((s) => s.balance);
  const reloadBalance = useBalanceStore((s) => s.reload);

  useEffect(() => {
    void agentClient.getUsageStats({ window }).then(setStats);
  }, [window]);

  useEffect(() => {
    // 进入 panel 主动刷新一次余额 (App 启动 + 每 turn_complete 之外的额外触发)
    void reloadBalance();
  }, [reloadBalance]);

  const cost = stats?.totalCostUsd ?? 0;
  const saved = stats?.cumulativeCacheSavedUsd ?? 0;

  return (
    <div>
      <PanelTitle
        title="用量与计费"
        description="按需查看,默认不在主界面常驻显示."
      />

      {/* 余额 cell (失败 / 未配置时整块隐藏) */}
      {balance && balance.isAvailable && balance.balanceInfos.length > 0 && (
        <BalanceBlock balance={balance} />
      )}

      {/* 用量大数 + 时间窗切换 */}
      <div className="rounded-xl border border-border-subtle bg-elevated p-6 mb-6">
        <div className="flex items-center justify-between mb-2">
          <div className="text-xs text-text-tertiary">
            {WINDOW_LABEL[stats?.windowLabel ?? window]}
          </div>
          <select
            value={window}
            onChange={(e) => setWindow(e.target.value as UsageStatsWindow)}
            className="h-7 px-2 rounded-md text-xs bg-input-bg border border-border-default text-text-primary outline-none focus:border-border-focus"
          >
            {WINDOW_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </div>

        <div className="flex items-baseline gap-3 mb-4">
          <span className="text-3xl font-semibold text-text-primary tabular-nums">
            {formatCostCny(cost)}
          </span>
          <span className="text-xs text-success inline-flex items-center gap-1">
            <Icon icon={CheckCircleIcon} size={12} weight="fill" />
            缓存命中 {((stats?.cacheHitRatio ?? 0) * 100).toFixed(0)}%
          </span>
        </div>

        <div className="grid grid-cols-4 gap-4 text-xs">
          <Stat
            label="缓存命中"
            value={formatTokens(stats?.cacheReadTokens ?? 0)}
          />
          <Stat
            label="未缓存输入"
            value={formatTokens(stats?.cacheMissTokens ?? 0)}
          />
          <Stat
            label="缓存写入"
            value={formatTokens(stats?.cacheCreationTokens ?? 0)}
            hint="仅 Anthropic 非 0"
          />
          <Stat
            label="输出 tokens"
            value={formatTokens(stats?.outputTokens ?? 0)}
          />
        </div>

        {saved > 0 && (
          <div className="mt-3 text-xs text-success inline-flex items-center gap-1">
            <Icon icon={CheckCircleIcon} size={12} weight="fill" />
            缓存累计省下 {formatCostCny(saved)}
          </div>
        )}
      </div>

      {/* 计费策略说明 */}
      <Section title="计费策略">
        <Row
          label="缓存命中折扣"
          description="DeepSeek 前缀缓存命中按 1/10 价格计入"
          control={<Pill tone="success">已生效</Pill>}
        />
        <Row
          label="预算上限"
          description="预算 / off-peak / 自动升级 Pro 等高级控制不在 P3a 范围 (P3b+)"
          control={
            <Pill tone="neutral" icon={WarningIcon}>
              规划中
            </Pill>
          }
        />
      </Section>
    </div>
  );
}

function BalanceBlock({
  balance,
}: {
  balance: NonNullable<ReturnType<typeof useBalanceStore.getState>["balance"]>;
}) {
  const primary =
    balance.balanceInfos.find((b) => b.currency === balance.primaryCurrency) ??
    balance.balanceInfos[0];
  const tone = balanceTone(primary.total);

  return (
    <div
      className={cn(
        "rounded-xl border p-5 mb-6",
        tone === "danger" && "border-danger/40 bg-danger-soft",
        tone === "warning" && "border-warning/40 bg-warning-soft",
        tone === "success" && "border-border-subtle bg-elevated",
      )}
    >
      <div className="text-xs text-text-tertiary mb-1">账户余额</div>
      <div className="flex items-baseline gap-3">
        <span
          className={cn(
            "text-2xl font-semibold tabular-nums",
            tone === "danger" && "text-danger",
            tone === "warning" && "text-warning",
            tone === "success" && "text-text-primary",
          )}
        >
          {formatBalance(primary.currency, primary.total)}
        </span>
        {primary.granted !== null && primary.granted > 0 && (
          <span className="text-xs text-text-tertiary">
            含赠送 {formatBalance(primary.currency, primary.granted)}
          </span>
        )}
      </div>
      {balance.balanceInfos.length > 1 && (
        <div className="mt-2 flex flex-wrap gap-3 text-xs text-text-tertiary">
          {balance.balanceInfos
            .filter((b) => b.currency !== primary.currency)
            .map((b) => (
              <span key={b.currency}>
                {formatBalance(b.currency, b.total)}
              </span>
            ))}
        </div>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div>
      <div className="text-text-tertiary mb-1 inline-flex items-center gap-1">
        {label}
        {hint && (
          <span
            className="text-text-tertiary opacity-60"
            title={hint}
            aria-label={hint}
          >
            ⓘ
          </span>
        )}
      </div>
      <div className="text-sm text-text-primary font-mono tabular-nums">
        {value}
      </div>
    </div>
  );
}
