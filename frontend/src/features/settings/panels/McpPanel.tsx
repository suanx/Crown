import { useCallback, useEffect, useState } from "react";
import { agentClient, type McpServer, type McpToolInfo } from "@/api";
import { Toggle } from "@/shared/ui/Toggle";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { Dialog } from "@/shared/ui/Dialog";
import { Icon } from "@/shared/icons/Icon";
import {
  PlusIcon,
  RefreshIcon,
  McpIcon,
  TrashIcon,
  EditIcon,
  CloseIcon,
  SpinnerIcon,
  CaretDownIcon,
  CaretRightIcon,
  CodeIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

/**
 * MCP 服务器面板 — 连真后端 (deepseek-mcp / rmcp).
 *
 * - 列表 + 状态徽章来自 `listMcpServers`
 * - 添加/编辑: 结构化表单 → `mcpAddServer` (写 mcp.json + 热重载)
 * - 启用/禁用: `toggleMcpServer`; 删除: `mcpRemoveServer`; 重连: `restartMcpServer`
 * - 全部重载: `mcpReload`
 * - 实时状态: 订阅 `onMcpServerStatusChanged` / `onMcpToolsChanged`,无需轮询
 */
export function McpPanel() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<McpServer | null>(null);

  const refresh = useCallback(async () => {
    try {
      setServers(await agentClient.listMcpServers());
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    // 状态变更 / 工具集变更时静默刷新列表 (含 toolCount)。
    const unsubStatus = agentClient.onMcpServerStatusChanged(() => {
      void refresh();
    });
    const unsubTools = agentClient.onMcpToolsChanged(() => {
      void refresh();
    });
    return () => {
      unsubStatus();
      unsubTools();
    };
  }, [refresh]);

  const runAction = useCallback(
    async (name: string, action: () => Promise<void>) => {
      setBusy(name);
      try {
        await action();
        await refresh();
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  return (
    <div>
      <div className="flex items-start justify-between mb-6">
        <div>
          <h1 className="text-xl font-semibold text-text-primary">
            MCP 服务器
          </h1>
          <p className="mt-1 text-sm text-text-secondary">
            连接外部工具与数据源,扩展 Agent 能力。配置写入全局 mcp.json,热重载生效。
          </p>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <Button
            variant="ghost"
            icon={RefreshIcon}
            onClick={() => void agentClient.mcpReload().then(refresh)}
          >
            全部重载
          </Button>
          <Button
            variant="primary"
            icon={PlusIcon}
            onClick={() => setAddOpen(true)}
          >
            添加服务器
          </Button>
        </div>
      </div>

      <div className="space-y-2">
        {servers.map((s) => (
          <ServerCard
            key={s.name}
            server={s}
            busy={busy === s.name}
            onToggle={(enabled) =>
              void runAction(s.name, () =>
                agentClient.toggleMcpServer(s.name, enabled),
              )
            }
            onRestart={() =>
              void runAction(s.name, () =>
                agentClient.restartMcpServer(s.name),
              )
            }
            onRemove={() =>
              void runAction(s.name, () => agentClient.mcpRemoveServer(s.name))
            }
            onEdit={() => setEditingServer(s)}
          />
        ))}
      </div>

      {!loading && servers.length === 0 && (
        <div className="rounded-lg border border-dashed border-border-default p-8 text-center">
          <Icon
            icon={McpIcon}
            size={28}
            weight="duotone"
            className="text-text-tertiary mx-auto mb-3"
          />
          <div className="text-sm text-text-secondary">尚未配置 MCP 服务器</div>
          <div className="mt-1 text-xs text-text-tertiary">
            点击「添加服务器」,或在对话里直接说「帮我装一个 xxx MCP」让 Agent 自己装。
          </div>
        </div>
      )}

      {/* 添加对话框 */}
      <AddEditServerDialog
        open={addOpen}
        editingServer={null}
        onClose={() => setAddOpen(false)}
        onSave={async (name, config) => {
          await agentClient.mcpAddServer(name, config);
          setAddOpen(false);
          await refresh();
        }}
      />

      {/* 编辑对话框 */}
      <AddEditServerDialog
        open={editingServer !== null}
        editingServer={editingServer}
        onClose={() => setEditingServer(null)}
        onSave={async (name, config) => {
          if (editingServer && name !== editingServer.name) {
            // 改名时先删旧服务器再加新配置
            await agentClient.mcpRemoveServer(editingServer.name);
          }
          // 同名时 mcpAddServer 直接覆盖写 mcp.json
          await agentClient.mcpAddServer(name, config);
          setEditingServer(null);
          await refresh();
        }}
      />
    </div>
  );
}

function ServerCard({
  server,
  busy,
  onToggle,
  onRestart,
  onRemove,
  onEdit,
}: {
  server: McpServer;
  busy: boolean;
  onToggle: (enabled: boolean) => void;
  onRestart: () => void;
  onRemove: () => void;
  onEdit: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [tools, setTools] = useState<McpToolInfo[] | null>(null);
  const [loadingTools, setLoadingTools] = useState(false);
  const [schemaTool, setSchemaTool] = useState<McpToolInfo | null>(null);

  useEffect(() => {
    if (expanded && tools === null && !loadingTools && server.status === "connected" && server.toolCount > 0) {
      setLoadingTools(true);
      void agentClient.listMcpTools(server.name).then((t) => {
        setTools(t);
        setLoadingTools(false);
      }).catch(() => setLoadingTools(false));
    }
  }, [expanded, tools, loadingTools, server.name, server.status, server.toolCount]);

  const isHttp = server.command?.startsWith("http://") || server.command?.startsWith("https://");

  return (
    <div
      className={cn(
        "rounded-lg border bg-elevated",
        server.status === "failed"
          ? "border-danger/30"
          : "border-border-subtle",
      )}
    >
      <div className="p-4">
        <div className="flex items-start gap-3">
          {/* Tool count badge — clickable to expand */}
          <button
            onClick={() => {
              if (server.toolCount > 0) setExpanded(!expanded);
            }}
            className={cn(
              "h-9 w-9 rounded-lg flex items-center justify-center shrink-0 transition-colors focus-ring",
              server.status === "connected" && "bg-success-soft text-success",
              server.status === "failed" && "bg-danger-soft text-danger",
              (server.status === "disabled" ||
                server.status === "pending" ||
                server.status === "needs_auth") &&
                "bg-elevated text-text-tertiary border border-border-subtle",
              server.toolCount > 0 && "cursor-pointer hover:opacity-80",
            )}
            title={server.toolCount > 0 ? (expanded ? "收起工具列表" : "展开工具列表") : undefined}
          >
            <Icon icon={McpIcon} size={16} weight="duotone" />
          </button>

          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-sm font-semibold text-text-primary">
                {server.name}
              </span>
              {(() => {
                const map: Record<string, { tone: "success" | "danger" | "neutral" | "warning" | "info"; label: string }> = {
                  connected: { tone: "success", label: "已连接" },
                  failed: { tone: "danger", label: "连接失败" },
                  disabled: { tone: "neutral", label: "已禁用" },
                  pending: { tone: "warning", label: "连接中" },
                  needs_auth: { tone: "info", label: "需要认证" },
                };
                const s = map[server.status] ?? { tone: "neutral", label: server.status };
                return <Pill tone={s.tone}>{s.label}</Pill>;
              })()}
              {server.toolCount > 0 && (
                <button
                  onClick={() => setExpanded(!expanded)}
                  className="flex items-center gap-1 text-xs text-text-tertiary hover:text-text-secondary transition-colors focus-ring rounded px-1"
                >
                  <Icon icon={expanded ? CaretDownIcon : CaretRightIcon} size={10} />
                  {server.toolCount} 个工具
                </button>
              )}
            </div>
            <div className="text-xs font-mono text-text-tertiary mt-1 truncate">
              {isHttp ? (
                <span>{server.command}</span>
              ) : (
                <span>{server.command} {server.args.join(" ")}</span>
              )}
            </div>
            {server.errorMessage && (
              <div className="mt-2 text-xs text-danger break-all">
                {server.errorMessage}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <Button
              variant="ghost"
              size="sm"
              icon={EditIcon}
              disabled={busy}
              onClick={onEdit}
            >
              编辑
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={TrashIcon}
              disabled={busy}
              onClick={onRemove}
            >
              删除
            </Button>
            <Button
              variant="ghost"
              size="sm"
              icon={busy ? SpinnerIcon : RefreshIcon}
              disabled={busy}
              onClick={onRestart}
            >
              重连
            </Button>
            <Toggle
              checked={server.enabled}
              disabled={busy}
              onChange={onToggle}
              label={`启用 ${server.name}`}
            />
          </div>
        </div>
      </div>

      {/* Expandable tool list */}
      {expanded && (
        <div className="border-t border-border-subtle px-4 py-3 space-y-2">
          {loadingTools ? (
            <div className="flex items-center gap-2 text-xs text-text-tertiary py-2">
              <Icon icon={SpinnerIcon} size={12} className="animate-spin" />
              加载工具列表...
            </div>
          ) : tools && tools.length > 0 ? (
            tools.map((tool) => (
              <div
                key={tool.name}
                className="rounded-md border border-border-subtle bg-surface p-3"
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <Icon icon={CodeIcon} size={12} className="text-brand shrink-0" />
                      <span className="text-xs font-semibold text-text-primary font-mono">
                        {tool.name}
                      </span>
                    </div>
                    {tool.description && (
                      <p className="mt-1 text-xs text-text-secondary leading-relaxed">
                        {tool.description}
                      </p>
                    )}
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={CodeIcon}
                    onClick={() => setSchemaTool(schemaTool?.name === tool.name ? null : tool)}
                  >
                    Schema
                  </Button>
                </div>
                {/* Inline schema preview */}
                {schemaTool?.name === tool.name && (
                  <div className="mt-2 rounded bg-elevated border border-border-subtle p-2 overflow-x-auto">
                    <pre className="text-[10px] font-mono text-text-secondary leading-relaxed whitespace-pre-wrap">
                      {JSON.stringify(tool.inputSchema, null, 2)}
                    </pre>
                  </div>
                )}
              </div>
            ))
          ) : (
            <div className="text-xs text-text-tertiary py-1">
              {server.status === "connected"
                ? "该服务器未暴露任何工具。"
                : "服务器未连接，无法获取工具列表。"}
            </div>
          )}
        </div>
      )}
    </div>
  );
}


/** 添加/编辑服务器对话框 — 结构化表单。 */
function AddEditServerDialog({
  open,
  editingServer,
  onClose,
  onSave,
}: {
  open: boolean;
  editingServer: McpServer | null;
  onClose: () => void;
  onSave: (name: string, config: unknown) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [type, setType] = useState<"stdio" | "http">("stdio");
  const [command, setCommand] = useState("");
  const [argsText, setArgsText] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // 打开时初始化表单
  useEffect(() => {
    if (open) {
      if (editingServer) {
        setName(editingServer.name);
        const isHttp =
          editingServer.command?.startsWith("http://") ||
          editingServer.command?.startsWith("https://");
        if (isHttp) {
          setType("http");
          setUrl(editingServer.command);
          setCommand("");
          setArgsText("");
        } else {
          setType("stdio");
          setCommand(editingServer.command || "");
          setArgsText(editingServer.args.join("\n"));
          setUrl("");
        }
      } else {
        setName("");
        setType("stdio");
        setCommand("");
        setArgsText("");
        setUrl("");
      }
      setError(null);
      setSubmitting(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editingServer]);

  async function submit() {
    setError(null);
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("请填写服务器名称");
      return;
    }

    let config: unknown;
    if (type === "http") {
      const trimmedUrl = url.trim();
      if (!trimmedUrl) {
        setError("请填写 URL");
        return;
      }
      config = { type: "http", url: trimmedUrl };
    } else {
      const trimmedCmd = command.trim();
      if (!trimmedCmd) {
        setError("请填写命令 (command)");
        return;
      }
      // 按行分割 args，过滤空行
      const args = argsText
        .split("\n")
        .map((a) => a.trim())
        .filter((a) => a.length > 0);
      config = { command: trimmedCmd, args };
    }

    setSubmitting(true);
    try {
      await onSave(trimmedName, config);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  }

  const isEditing = editingServer !== null;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      className="max-w-lg"
      modal={submitting}
    >
      <div className="flex items-center justify-between px-5 h-12 border-b border-border-subtle">
        <span className="text-sm font-semibold text-text-primary">
          {isEditing ? "编辑 MCP 服务器" : "添加 MCP 服务器"}
        </span>
        <button
          onClick={onClose}
          className="text-text-tertiary hover:text-text-primary focus-ring rounded"
          aria-label="关闭"
          disabled={submitting}
        >
          <Icon icon={CloseIcon} size={16} />
        </button>
      </div>

      <div className="p-5 space-y-4">
        {/* 服务器名称 */}
        <div>
          <label className="text-xs text-text-secondary mb-1.5 block">
            服务器名称
          </label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如 everything"
            disabled={submitting}
            className={cn(
              "h-9 w-full px-3 rounded-md text-sm bg-input-bg text-text-primary",
              "border border-border-default placeholder:text-text-tertiary",
              "outline-none focus:border-border-focus transition-colors",
              submitting && "opacity-50 cursor-not-allowed",
            )}
          />
        </div>

        {/* 类型选择 */}
        <div>
          <label className="text-xs text-text-secondary mb-1.5 block">
            连接类型
          </label>
          <div className="flex gap-3">
            <label
              className={cn(
                "flex items-center gap-2 px-3 h-9 rounded-md text-sm cursor-pointer transition-colors border",
                type === "stdio"
                  ? "border-brand bg-brand/10 text-text-primary"
                  : "border-border-default bg-input-bg text-text-secondary hover:border-border-focus",
                submitting && "opacity-50 cursor-not-allowed",
              )}
            >
              <input
                type="radio"
                name="mcp-type"
                value="stdio"
                checked={type === "stdio"}
                onChange={() => setType("stdio")}
                disabled={submitting}
                className="sr-only"
              />
              <span
                className={cn(
                  "w-3.5 h-3.5 rounded-full border flex items-center justify-center shrink-0",
                  type === "stdio"
                    ? "border-brand"
                    : "border-border-default",
                )}
              >
                {type === "stdio" && (
                  <span className="w-2 h-2 rounded-full bg-brand" />
                )}
              </span>
              stdio (本地命令)
            </label>
            <label
              className={cn(
                "flex items-center gap-2 px-3 h-9 rounded-md text-sm cursor-pointer transition-colors border",
                type === "http"
                  ? "border-brand bg-brand/10 text-text-primary"
                  : "border-border-default bg-input-bg text-text-secondary hover:border-border-focus",
                submitting && "opacity-50 cursor-not-allowed",
              )}
            >
              <input
                type="radio"
                name="mcp-type"
                value="http"
                checked={type === "http"}
                onChange={() => setType("http")}
                disabled={submitting}
                className="sr-only"
              />
              <span
                className={cn(
                  "w-3.5 h-3.5 rounded-full border flex items-center justify-center shrink-0",
                  type === "http"
                    ? "border-brand"
                    : "border-border-default",
                )}
              >
                {type === "http" && (
                  <span className="w-2 h-2 rounded-full bg-brand" />
                )}
              </span>
              HTTP (远程)
            </label>
          </div>
        </div>

        {/* stdio 字段: command + args */}
        {type === "stdio" && (
          <>
            <div>
              <label className="text-xs text-text-secondary mb-1.5 block">
                命令 (command)
              </label>
              <input
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="例如 npx"
                disabled={submitting}
                className={cn(
                  "h-9 w-full px-3 rounded-md text-sm bg-input-bg text-text-primary",
                  "border border-border-default placeholder:text-text-tertiary",
                  "outline-none focus:border-border-focus transition-colors",
                  submitting && "opacity-50 cursor-not-allowed",
                )}
              />
            </div>
            <div>
              <label className="text-xs text-text-secondary mb-1.5 block">
                参数 (args) — 每行一个
              </label>
              <textarea
                value={argsText}
                onChange={(e) => setArgsText(e.target.value)}
                placeholder={`-y\n@modelcontextprotocol/server-everything`}
                rows={4}
                spellCheck={false}
                disabled={submitting}
                className={cn(
                  "w-full px-3 py-2 rounded-md text-xs font-mono bg-input-bg text-text-primary",
                  "border border-border-default placeholder:text-text-tertiary",
                  "outline-none focus:border-border-focus transition-colors resize-y",
                  submitting && "opacity-50 cursor-not-allowed",
                )}
              />
            </div>
          </>
        )}

        {/* http 字段: url */}
        {type === "http" && (
          <div>
            <label className="text-xs text-text-secondary mb-1.5 block">
              URL
            </label>
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/mcp"
              disabled={submitting}
              className={cn(
                "h-9 w-full px-3 rounded-md text-sm bg-input-bg text-text-primary",
                "border border-border-default placeholder:text-text-tertiary",
                "outline-none focus:border-border-focus transition-colors",
                submitting && "opacity-50 cursor-not-allowed",
              )}
            />
          </div>
        )}

        {error && <div className="text-xs text-danger break-all">{error}</div>}
      </div>

      <div className="flex items-center justify-end gap-2 px-5 h-14 border-t border-border-subtle">
        <Button variant="ghost" onClick={onClose} disabled={submitting}>
          取消
        </Button>
        <Button
          variant="primary"
          onClick={() => void submit()}
          disabled={submitting}
          icon={submitting ? SpinnerIcon : PlusIcon}
        >
          {isEditing ? "保存修改" : "添加并连接"}
        </Button>
      </div>
    </Dialog>
  );
}
