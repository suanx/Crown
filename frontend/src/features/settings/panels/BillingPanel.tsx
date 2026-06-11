import { useEffect, useState } from "react";
import {
  agentClient,
  type UsageStats,
  type UsageStatsWindow,
  type UsageChartPoint,
} from "@/api";
import { PanelTitle } from "./_shared";
import { Icon } from "@/shared/icons/Icon";
import { CheckCircleIcon } from "@/shared/icons/set";
import { formatCostCny, formatTokens } from "@/shared/lib/format";
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

export function BillingPanel() {
  const [window, setWindow] = useState<UsageStatsWindow>("session");
  const [stats, setStats] = useState<UsageStats | null>(null);
  const [chartData, setChartData] = useState<UsageChartPoint[]>([]);

  useEffect(() => {
    void agentClient.getUsageStats({ window }).then(setStats);
  }, [window]);

  useEffect(() => {
    void agentClient.getUsageChart().then(setChartData);
  }, []);

  const cost = stats?.totalCostUsd ?? 0;

  return (
    <div>
      <PanelTitle
        title="用量统计"
        description="Token 消耗与缓存效率统计"
      />

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
          <Stat label="缓存命中" value={formatTokens(stats?.cacheReadTokens ?? 0)} />
          <Stat label="未缓存输入" value={formatTokens(stats?.cacheMissTokens ?? 0)} />
          <Stat
            label="缓存写入"
            value={formatTokens(stats?.cacheCreationTokens ?? 0)}
            hint="仅 Anthropic 非 0"
          />
          <Stat label="输出 tokens" value={formatTokens(stats?.outputTokens ?? 0)} />
        </div>
      </div>

      {/* 每日消耗趋势柱状图 */}
      <div className="rounded-xl border border-border-subtle bg-elevated p-6 mb-6">
        <div className="text-sm font-medium text-text-primary mb-4">
          每日消耗趋势 <span className="text-xs text-text-tertiary ml-2">近 30 天</span>
        </div>
        <CostChart data={chartData} />
      </div>

      {/* 缓存命中率环形图 */}
      <div className="rounded-xl border border-border-subtle bg-elevated p-6 mb-6">
        <div className="text-sm font-medium text-text-primary mb-4">缓存命中率</div>
        <div className="flex items-center gap-8">
          <CacheDonut ratio={stats?.cacheHitRatio ?? 0} />
          <div className="text-xs text-text-tertiary space-y-1">
            <div className="flex items-center gap-2">
              <span className="w-2.5 h-2.5 rounded-full bg-success" />
              命中 {formatTokens(stats?.cacheReadTokens ?? 0)}
            </div>
            <div className="flex items-center gap-2">
              <span className="w-2.5 h-2.5 rounded-full bg-text-tertiary/30" />
              未命中 {formatTokens(stats?.cacheMissTokens ?? 0)}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── SVG 柱状图 ──────────────────────────────────────────────────────────────

function CostChart({ data }: { data: UsageChartPoint[] }) {
  if (data.length === 0) {
    return (
      <div className="h-[120px] flex items-center justify-center text-xs text-text-tertiary">
        暂无数据
      </div>
    );
  }

  const W = 600;
  const H = 120;
  const PAD = { t: 4, r: 4, b: 16, l: 44 };
  const innerW = W - PAD.l - PAD.r;
  const innerH = H - PAD.t - PAD.b;
  const maxCost = Math.max(...data.map((d) => d.totalCostUsd), 0.001);
  const barW = Math.max(3, Math.min(16, innerW / data.length - 2));

  const ticks = 4;
  const yStep = maxCost / ticks;

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full h-full">
      {Array.from({ length: ticks + 1 }, (_, i) => {
        const y = PAD.t + innerH - (i / ticks) * innerH;
        return (
          <g key={i}>
            <line x1={PAD.l} y1={y} x2={W - PAD.r} y2={y} stroke="var(--ds-border-subtle)" strokeWidth={0.5} />
            <text x={PAD.l - 4} y={y + 3} textAnchor="end" className="fill-text-tertiary" fontSize={9}>
              {formatCostCny(yStep * i)}
            </text>
          </g>
        );
      })}

      {data.map((d, i) => {
        const barH = (d.totalCostUsd / maxCost) * innerH;
        const x = PAD.l + (i / data.length) * innerW + (innerW / data.length - barW) / 2;
        const y = PAD.t + innerH - barH;
        const date = new Date(d.dayEpochMs);
        const label = `${date.getMonth() + 1}/${date.getDate()}`;
        const isToday = date.getDate() === new Date().getDate() && date.getMonth() === new Date().getMonth();
        return (
          <g key={i}>
            <rect x={x} y={y} width={barW} height={barH} rx={2} className={isToday ? "fill-brand" : "fill-brand/60"}>
              <title>{label} · {formatCostCny(d.totalCostUsd)}</title>
            </rect>
            {(i % Math.max(1, Math.floor(data.length / 8)) === 0 || isToday) && (
              <text x={x + barW / 2} y={PAD.t + innerH + 12} textAnchor="middle" className={cn("fill-text-tertiary text-[8px]", isToday && "fill-brand font-medium")}>
                {label}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

// ── SVG 缓存命中环形图 ─────────────────────────────────────────────────────

function CacheDonut({ ratio }: { ratio: number }) {
  const size = 80;
  const r = 30;
  const stroke = 8;
  const circ = 2 * Math.PI * r;
  const offset = circ * (1 - ratio);

  return (
    <svg width={size} height={size} className="shrink-0">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--ds-border-subtle)" strokeWidth={stroke} />
      {ratio > 0 && (
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--ds-success)" strokeWidth={stroke}
          strokeDasharray={circ} strokeDashoffset={offset} strokeLinecap="round"
          transform={`rotate(-90 ${size / 2} ${size / 2})`} className="transition-all duration-500" />
      )}
      <text x={size / 2} y={size / 2} textAnchor="middle" dominantBaseline="central" className="fill-text-primary font-semibold" fontSize={14}>
        {(ratio * 100).toFixed(0)}%
      </text>
    </svg>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div>
      <div className="text-text-tertiary mb-1 inline-flex items-center gap-1">
        {label}
        {hint && <span className="text-text-tertiary opacity-60" title={hint} aria-label={hint}>ⓘ</span>}
      </div>
      <div className="text-sm text-text-primary font-mono tabular-nums">{value}</div>
    </div>
  );
}
