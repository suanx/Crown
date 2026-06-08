import { useSyncExternalStore } from "react";
import { useUiStore } from "@/stores/uiStore";
import {
  CONTRACT_STATUS,
  computeContractStats,
  devtools,
  apiMode,
  COMMAND_KEYS,
  EVENT_KEYS,
  type ContractStatus,
  type EndpointKey,
} from "@/api";
import { Icon } from "@/shared/icons/Icon";
import {
  CloseIcon,
  CopyIcon,
  DownloadIcon,
  RefreshIcon,
  WarningIcon,
} from "@/shared/icons/set";
import { Pill } from "@/shared/ui/Pill";
import { cn } from "@/shared/lib/cn";

/**
 * IPC 对接看板.
 * 快捷键 Ctrl+Shift+D 切换显隐.
 *
 * 三段:
 *   - 概览 (统计 + 图条 + 操作按钮)
 *   - Commands 列表 (状态 / 调用次数 / 最近调用)
 *   - Events 列表
 *   - 字段不匹配警告
 */
export function DevtoolsPanel() {
  const open = useUiStore((s) => s.devtoolsOpen);
  const close = useUiStore((s) => s.toggleDevtools);

  // 订阅 devtools 状态
  useSyncExternalStore(devtools.subscribe, devtools.getSnapshot);
  const snapshot = devtools.getSnapshot();

  if (!open) return null;

  const stats = computeContractStats();
  const ready = stats.byStatus.connected + stats.byStatus.verified;
  const pct = stats.pctConnected * 100;

  return (
    <div className="fixed inset-y-0 right-0 z-40 w-[460px] flex flex-col bg-elevated border-l border-border-default animate-fade-in"
      style={{ boxShadow: "var(--ds-shadow-lg)" }}
    >
      {/* —— 头部 —— */}
      <div className="px-4 h-12 flex items-center justify-between border-b border-border-subtle shrink-0">
        <div className="flex items-center gap-2">
          <span className="h-2 w-2 rounded-full bg-brand animate-pulse-soft" />
          <span className="text-sm font-semibold text-text-primary">
            IPC 对接看板
          </span>
          <Pill tone="neutral" size="sm">
            mode: {apiMode}
          </Pill>
        </div>
        <button
          onClick={() => close(false)}
          aria-label="关闭"
          className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary focus-ring"
        >
          <Icon icon={CloseIcon} size={14} />
        </button>
      </div>

      {/* —— 概览 —— */}
      <div className="px-4 py-3 border-b border-border-subtle shrink-0">
        <div className="flex items-baseline justify-between mb-2">
          <div className="flex items-baseline gap-2">
            <span className="text-2xl font-semibold text-text-primary font-mono">
              {ready}/{stats.total}
            </span>
            <span className="text-xs text-text-tertiary">已对接 ({pct.toFixed(0)}%)</span>
          </div>
        </div>
        {/* 进度条 */}
        <ProgressBar stats={stats.byStatus} />
        {/* 图例 */}
        <div className="mt-2 flex flex-wrap gap-3 text-xs">
          <Legend color="bg-success" label="verified" count={stats.byStatus.verified} />
          <Legend color="bg-brand" label="connected" count={stats.byStatus.connected} />
          <Legend color="bg-text-tertiary" label="mock" count={stats.byStatus.mock} />

        </div>

        <div className="mt-3 flex items-center gap-2">
          <button
            onClick={() => downloadReport()}
            className="h-7 px-2 inline-flex items-center gap-1.5 rounded-md text-xs bg-canvas border border-border-subtle hover:bg-hover text-text-secondary focus-ring"
          >
            <Icon icon={DownloadIcon} size={12} />
            导出报告
          </button>
          <button
            onClick={() => copyReport()}
            className="h-7 px-2 inline-flex items-center gap-1.5 rounded-md text-xs bg-canvas border border-border-subtle hover:bg-hover text-text-secondary focus-ring"
          >
            <Icon icon={CopyIcon} size={12} />
            复制 markdown
          </button>
          <button
            onClick={() => devtools.clear()}
            className="h-7 px-2 inline-flex items-center gap-1.5 rounded-md text-xs bg-canvas border border-border-subtle hover:bg-hover text-text-secondary focus-ring"
          >
            <Icon icon={RefreshIcon} size={12} />
            清空记录
          </button>
        </div>
      </div>

      {/* —— 内容区 —— */}
      <div className="flex-1 min-h-0 scrollable">
        <SectionHeader>Commands ({COMMAND_KEYS.length})</SectionHeader>
        {COMMAND_KEYS.map((k) => (
          <EndpointRow
            key={k}
            endpoint={k}
            status={CONTRACT_STATUS[k]}
            count={snapshot.callCountByEndpoint[k] ?? 0}
            last={snapshot.lastCallByEndpoint[k]}
          />
        ))}

        <SectionHeader>Events ({EVENT_KEYS.length})</SectionHeader>
        {EVENT_KEYS.map((k) => (
          <EndpointRow
            key={k}
            endpoint={k}
            status={CONTRACT_STATUS[k]}
            count={snapshot.callCountByEndpoint[k] ?? 0}
            last={snapshot.lastCallByEndpoint[k]}
          />
        ))}

        {snapshot.shapeMismatches.length > 0 && (
          <>
            <SectionHeader tone="danger">
              字段形状不匹配 ({snapshot.shapeMismatches.length})
            </SectionHeader>
            {snapshot.shapeMismatches.slice(-20).map((m, i) => (
              <div
                key={i}
                className="px-4 py-2 border-b border-border-subtle text-xs font-mono"
              >
                <div className="text-text-secondary">
                  <span className="text-danger">●</span> {m.endpoint}
                </div>
                <div className="text-text-tertiary mt-0.5">
                  字段 <span className="text-text-primary">{m.field}</span>{" "}
                  期望 <span className="text-success">{m.expected}</span>{" "}
                  实际 <span className="text-danger">{m.actual}</span>
                </div>
              </div>
            ))}
          </>
        )}

        <div className="px-4 py-3 text-xs text-text-tertiary leading-relaxed">
          <Icon icon={WarningIcon} size={12} className="inline mr-1" />
          这是开发期看板. 对接进度由{" "}
          <code className="px-1 bg-canvas rounded">src/api/status.ts</code>{" "}
          的 CONTRACT_STATUS 决定. 改一个端点的状态需要在 Rust 端实现对应 IPC handler.
        </div>
      </div>
    </div>
  );
}

// ── sub-components ────────────────────────────────────────────────────────

function ProgressBar({
  stats,
}: {
  stats: Record<ContractStatus, number>;
}) {
  const total =
    stats.verified + stats.connected + stats.mock || 1;
  const segs: Array<{ color: string; w: number }> = [
    { color: "bg-success", w: stats.verified / total },
    { color: "bg-brand", w: stats.connected / total },
    { color: "bg-text-tertiary", w: stats.mock / total },
  ];

  return (
    <div className="h-1.5 w-full bg-canvas rounded-full overflow-hidden flex">
      {segs.map((s, i) => (
        <div
          key={i}
          className={cn("h-full", s.color)}
          style={{ width: `${s.w * 100}%` }}
        />
      ))}
    </div>
  );
}

function Legend({
  color,
  label,
  count,
}: {
  color: string;
  label: string;
  count: number;
}) {
  return (
    <div className="inline-flex items-center gap-1.5 text-text-secondary">
      <span className={cn("h-2 w-2 rounded-full", color)} />
      <span>{label}</span>
      <span className="text-text-tertiary font-mono">{count}</span>
    </div>
  );
}

function SectionHeader({
  children,
  tone,
}: {
  children: React.ReactNode;
  tone?: "danger";
}) {
  return (
    <div
      className={cn(
        "sticky top-0 z-10 px-4 py-2 text-xs font-medium uppercase tracking-wide bg-elevated border-b border-border-subtle",
        tone === "danger" ? "text-danger" : "text-text-tertiary",
      )}
    >
      {children}
    </div>
  );
}

function EndpointRow({
  endpoint,
  status,
  count,
  last,
}: {
  endpoint: EndpointKey;
  status: ContractStatus;
  count: number;
  last?: { source: string; timestamp: number; errorMessage: string | null };
}) {
  const ago = last ? `${((Date.now() - last.timestamp) / 1000).toFixed(1)}s 前` : "—";
  return (
    <div className="px-4 py-2 border-b border-border-subtle flex items-center gap-3 hover:bg-hover transition-colors">
      <StatusDot status={status} />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-mono text-text-primary truncate">
          {endpoint}
        </div>
        {last?.errorMessage && (
          <div className="text-xs text-danger truncate mt-0.5 font-mono">
            {last.errorMessage}
          </div>
        )}
      </div>
      <div className="text-right shrink-0">
        <div className="text-xs font-mono text-text-secondary">{count} 次</div>
        <div className="text-xs text-text-tertiary">{ago}</div>
      </div>
    </div>
  );
}

function StatusDot({ status }: { status: ContractStatus }) {
  const map: Record<ContractStatus, string> = {
    verified: "bg-success",
    connected: "bg-brand",
    mock: "bg-text-tertiary",
  };

  return (
    <span
      title={status}
      className={cn("h-2 w-2 rounded-full shrink-0", map[status])}
    />
  );
}

// ── helpers ───────────────────────────────────────────────────────────────

function downloadReport() {
  const md = devtools.exportReportMarkdown();
  const blob = new Blob([md], { type: "text/markdown" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `ipc-report-${Date.now()}.md`;
  a.click();
  URL.revokeObjectURL(url);
}

function copyReport() {
  const md = devtools.exportReportMarkdown();
  void navigator.clipboard.writeText(md);
}
