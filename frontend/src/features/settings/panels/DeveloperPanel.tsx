import { useState } from "react";
import { PanelTitle, Section, Row } from "./_shared";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { Dialog } from "@/shared/ui/Dialog";
import { Input } from "@/shared/ui/Input";
import {
  DownloadIcon,
  BugIcon,
  ExternalLinkIcon,
  RefreshIcon,
  TestIcon,
  CloseIcon,
} from "@/shared/icons/set";
import { useUiStore } from "@/stores/uiStore";
import {
  agentClient,
  apiMode,
  computeContractStats,
  devtools,
} from "@/api";
import { cn } from "@/shared/lib/cn";

/**
 * 开发者设置 — 集中放置所有 dev 专用控件.
 *
 * 顶栏不再放 IPC% / Bug 入口,这些 dev 工具都通过这里访问,
 * 普通用户看不到也不会被打扰. 快捷键 Ctrl+Shift+D 仍保留.
 */
export function DeveloperPanel() {
  const stats = computeContractStats();
  const toggleDevtools = useUiStore((s) => s.toggleDevtools);
  const [providerId, setProviderId] = useState("deepseek");
  const [testModel, setTestModel] = useState("deepseek-chat");
  const [testMessage, setTestMessage] = useState("Hello, what model are you?");
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [showResult, setShowResult] = useState(false);

  const [diagnosticsPath, setDiagnosticsPath] = useState<string | null>(null);
  const [diagnosticsError, setDiagnosticsError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const ready = stats.byStatus.connected + stats.byStatus.verified;
  const pct = (stats.pctConnected * 100).toFixed(0);

  const tone =
    stats.pctConnected >= 0.8
      ? "success"
      : stats.pctConnected >= 0.4
        ? "warning"
      : "neutral";

  async function exportDiagnostics() {
    setExporting(true);
    setDiagnosticsPath(null);
    setDiagnosticsError(null);
    try {
      setDiagnosticsPath(await agentClient.exportDiagnostics());
    } catch (error) {
      setDiagnosticsError(error instanceof Error ? error.message : String(error));
    } finally {
      setExporting(false);
    }
  }

  async function testProvider() {
    setTesting(true);
    setTestResult(null);
    setTestError(null);
    try {
      const result = await agentClient.debugTestProvider(providerId, testModel, testMessage);
      setTestResult(result);
      setShowResult(true);
    } catch (error) {
      setTestError(error instanceof Error ? error.message : String(error));
      setShowResult(true);
    } finally {
      setTesting(false);
    }
  }


  return (
    <div>
      <PanelTitle
        title="开发者"
        description="对接进度、日志、诊断 — 仅开发期可见"
      />

      {/* IPC 对接看板入口 — 替代原来顶栏的 IPC% Badge */}
      <Section title="后端对接">
        <Row
          label="API 模式"
          description="VITE_API_MODE 环境变量决定 (mock / hybrid / tauri)"
          control={
            <Pill tone={apiMode === "tauri" ? "success" : "warning"}>
              {apiMode}
            </Pill>
          }
        />
        <Row
          label="IPC 对接进度"
          description={`${ready}/${stats.total} 端点已对接 · 连接 ${stats.byStatus.connected} · 已验证 ${stats.byStatus.verified} · Mock ${stats.byStatus.mock}`}
          control={
            <div className="flex items-center gap-3">
              <span
                className={cn(
                  "text-sm font-mono tabular-nums",
                  tone === "success" && "text-success",
                  tone === "warning" && "text-warning",
                  tone === "neutral" && "text-text-tertiary",
                )}
              >
                {pct}%
              </span>
              <Button
                variant="primary"
                size="sm"
                icon={BugIcon}
                onClick={() => toggleDevtools(true)}
              >
                打开看板
              </Button>
            </div>
          }
        />
        <Row
          label="清空运行时记录"
          description="重置调用次数 / 形状不匹配警告"
          control={
            <Button
              variant="ghost"
              size="sm"
              icon={RefreshIcon}
              onClick={() => devtools.clear()}
            >
              清空
            </Button>
          }
        />
      </Section>

      <Section title="日志">
        <Row
          label="日志级别"
          description="ERROR > WARN > INFO > DEBUG > TRACE"
          control={<Pill tone="info">可用</Pill>}
        />
        <Row
          label="完整 prompt dump"
          description="开启后每次 LLM 调用记录完整 request/response (磁盘占用大)"
          control={<Pill tone="info">可用</Pill>}
        />
      </Section>

      <Section title="诊断">
        <Row
          label="导出诊断包"
          description="最近 100 条日志 + config (脱敏 API key) + 系统信息"
          control={
            <Button
              variant="secondary"
              size="sm"
              icon={DownloadIcon}
              disabled={exporting}
              onClick={exportDiagnostics}
            >
              {exporting ? "导出中" : "导出"}
            </Button>
          }
        />
        {(diagnosticsPath || diagnosticsError) && (
          <Row
            label={diagnosticsPath ? "诊断包已导出" : "诊断导出失败"}
            description={diagnosticsPath ?? diagnosticsError ?? undefined}
            control={null}
          />
        )}
        <Row
          label="数据库路径"
          description="SQLite 文件位置"
          control={
            <Pill tone="info" icon={ExternalLinkIcon}>
              可用
            </Pill>
          }
        />
      </Section>

      <Section title="快捷键">
        <Row
          label="开发者面板"
          description="随时呼出 IPC 对接看板"
          control={
            <kbd className="inline-flex items-center px-2 h-7 rounded-md text-xs font-mono bg-canvas border border-border-default text-text-secondary">
              Ctrl+Shift+D
            </kbd>
          }
        />
      </Section>

      <Section title="测试供应商连接">
        <Row
          label="Provider ID"
          description="供应商标识符，如 deepseek / openai"
          control={
            <Input
              value={providerId}
              onChange={(e) => setProviderId(e.target.value)}
              placeholder="deepseek"
              className="w-40"
            />
          }
        />
        <Row
          label="模型"
          description="模型名称，如 deepseek-chat / gpt-4o"
          control={
            <Input
              value={testModel}
              onChange={(e) => setTestModel(e.target.value)}
              placeholder="deepseek-chat"
              className="w-40"
            />
          }
        />
        <Row
          label="消息"
          description="发送的测试消息内容"
          control={
            <Input
              value={testMessage}
              onChange={(e) => setTestMessage(e.target.value)}
              placeholder="Hello"
              fullWidth
            />
          }
        />
        <Row
          label="执行测试"
          description="发送请求并查看原始响应"
          control={
            <Button
              variant="primary"
              size="sm"
              icon={TestIcon}
              disabled={testing}
              onClick={testProvider}
            >
              {testing ? "测试中…" : "测试连接"}
            </Button>
          }
        />
      </Section>

      {/* 测试结果弹窗 */}
      <Dialog open={showResult} onClose={() => setShowResult(false)}>
        <div className="p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-base font-semibold text-text-primary">
              {testError ? "测试失败" : "测试结果"}
            </h3>
            <button
              onClick={() => setShowResult(false)}
              className="p-1 rounded-md hover:bg-hover text-text-secondary"
            >
              <CloseIcon size={16} />
            </button>
          </div>
          <pre className="text-sm text-text-primary bg-canvas rounded-lg p-4 overflow-auto max-h-96 whitespace-pre-wrap break-all">
            {testError ?? testResult}
          </pre>
          <div className="mt-4 flex justify-end">
            <Button variant="secondary" size="sm" onClick={() => setShowResult(false)}>
              关闭
            </Button>
          </div>
        </div>
      </Dialog>

    </div>
  );
}
