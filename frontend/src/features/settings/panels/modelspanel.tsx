import { useEffect, useMemo, useState } from "react";
import {
  agentClient,
  type AppConfig,
  type ProviderConfig,
  type ProviderKind,
  type ProviderModel,
  type ProviderTestResult,
} from "@/api";
import { PanelTitle, Section } from "./_shared";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { Pill } from "@/shared/ui/Pill";
import { Icon } from "@/shared/icons/Icon";
import type { ReactNode } from "react";
import { ProviderIcon } from "@/shared/ui/ProviderIcon";
import {
  CheckIcon,
  CloseIcon,
  PlusIcon,
  RefreshIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

const KIND_OPTIONS: Array<{ id: ProviderKind; label: string }> = [
  { id: "deepseek", label: "DeepSeek" },
  { id: "openai", label: "OpenAI" },
  { id: "openai-compatible", label: "OpenAI 兼容" },
  { id: "anthropic", label: "Anthropic" },
  { id: "ollama", label: "Ollama" },
];

function cloneProviders(items: ProviderConfig[]) {
  return items.map((p) => ({ ...p, models: p.models.map((m) => ({ ...m })) }));
}

function shortKey(key: string) {
  if (!key) return "未填写";
  if (key.length <= 10) return "已填写";
  return `${key.slice(0, 5)}...${key.slice(-4)}`;
}

function providerKey(provider: ProviderConfig) {
  return provider.apiKey ?? "";
}

function CompactRow({
  label,
  description,
  control,
}: {
  label: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div className="px-3 py-2 flex items-center gap-3">
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium text-text-primary">{label}</div>
        {description && (
          <div className="mt-0.5 text-[11px] leading-snug text-text-tertiary">
            {description}
          </div>
        )}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

export function ModelsPanel() {
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [defaultProviderId, setDefaultProviderId] = useState("deepseek");
  const [defaultModel, setDefaultModel] = useState("deepseek-v4-flash");
  const [selectedId, setSelectedId] = useState("deepseek");
  const [saving, setSaving] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [testing, setTesting] = useState(false);
  const [saved, setSaved] = useState(false);
  const [testResult, setTestResult] = useState<ProviderTestResult | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [newModelId, setNewModelId] = useState("");

  useEffect(() => {
    let cancelled = false;
    void agentClient.getConfig().then((cfg: AppConfig) => {
      if (cancelled) return;
      setProviders(cloneProviders(cfg.providers ?? []));
      setDefaultProviderId(cfg.defaultProviderId || "deepseek");
      setDefaultModel(cfg.defaultModel || "deepseek-v4-flash");
      setSelectedId(cfg.defaultProviderId || cfg.providers?.[0]?.id || "deepseek");
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const selected = useMemo(
    () => providers.find((p) => p.id === selectedId) ?? providers[0],
    [providers, selectedId],
  );

  const enabledModels = selected?.models.filter((m) => m.enabled) ?? [];

  function updateProvider(patch: Partial<ProviderConfig>) {
    if (!selected) return;
    setProviders((items) =>
      items.map((p) => (p.id === selected.id ? { ...p, ...patch } : p)),
    );
  }

  function updateModel(modelId: string, patch: Partial<ProviderModel>) {
    if (!selected) return;
    updateProvider({
      models: selected.models.map((m) =>
        m.id === modelId ? { ...m, ...patch } : m,
      ),
    });
  }

  function addManualModel() {
    const id = newModelId.trim();
    if (!selected || !id || selected.models.some((m) => m.id === id)) return;
    updateProvider({
      models: [...selected.models, { id, label: id, enabled: true }],
    });
    setNewModelId("");
  }

  async function fetchModels() {
    if (!selected) return;
    setFetching(true);
    setSyncError(null);
    try {
      const models = await agentClient.fetchProviderModels({ provider: selected });
      updateProvider({ models });
      if (selected.id === defaultProviderId && models.length > 0) {
        setDefaultModel(models[0].id);
      }
    } catch (error) {
      setSyncError(error instanceof Error ? error.message : String(error));
    } finally {
      setFetching(false);
    }
  }

  async function testConnection() {
    if (!selected) return;
    setTesting(true);
    setTestResult(null);
    setSyncError(null);
    try {
      const result = await agentClient.testProviderConnection({ provider: selected });
      setTestResult(result);
    } catch (error) {
      setTestResult({
        ok: false,
        latencyMs: 0,
        modelCount: 0,
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setTesting(false);
    }
  }

  async function save() {
    setSaving(true);
    setSyncError(null);
    const normalizedProviders = providers.map((provider) =>
      provider.id !== defaultProviderId
        ? provider
        : {
            ...provider,
            enabled: true,
            models: provider.models.map((model) =>
              model.id === defaultModel ? { ...model, enabled: true } : model,
            ),
          },
    );
    try {
      const cfg = await agentClient.saveProviders({
        providers: normalizedProviders,
        defaultProviderId,
        defaultModel,
      });
      const returned = cloneProviders(cfg.providers ?? providers);
      setProviders(
        returned.map((p) => {
          const local = normalizedProviders.find((item) => item.id === p.id);
          return {
            ...p,
            apiKey: p.apiKey ?? local?.apiKey ?? null,
          };
        }),
      );
      setDefaultProviderId(cfg.defaultProviderId);
      setDefaultModel(cfg.defaultModel);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1400);
    } catch (error) {
      setSyncError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  if (loading || !selected) {
    return (
      <div>
        <PanelTitle title="模型服务" description="正在读取供应商配置" />
      </div>
    );
  }

  return (
    <div>
      <PanelTitle
        title="模型服务"
        description="供应商、模型列表、默认模型在这里统一配置，保存后写入本地配置文件"
      />

      <div className="relative grid grid-cols-[180px_minmax(0,1fr)] items-start gap-4">
        <div>
          <Section title="供应商" className="mb-0 [&>h2]:mb-2">
            {providers.map((p) => {
              const active = p.id === selected.id;
              return (
                <button
                  key={p.id}
                  onClick={() => {
                    setSelectedId(p.id);
                    setTestResult(null);
                  }}
                  className={cn(
                    "w-full px-3 py-2 flex items-center gap-2 text-left hover:bg-hover transition-colors",
                    active && "bg-brand-soft",
                  )}
                >
                  <ProviderIcon
                    providerId={p.id}
                    name={p.name}
                    size={16}
                    className={p.enabled ? "opacity-100" : "opacity-40"}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="text-sm text-text-primary truncate">
                      {p.name}
                    </div>
                    <div className="text-[11px] text-text-tertiary truncate">
                      {p.models.filter((m) => m.enabled).length} 个模型
                    </div>
                  </div>
                  {p.id === defaultProviderId && (
                    <Icon icon={CheckIcon} size={13} className="text-brand" />
                  )}
                </button>
              );
            })}
          </Section>
        </div>

        <div>
          <Section title={selected.name} className="mb-2 [&>h2]:mb-1">
            <CompactRow
              label="启用"
              description="关闭后不会出现在输入区模型选择里"
              control={
                <input
                  type="checkbox"
                  checked={selected.enabled}
                  onChange={(e) => updateProvider({ enabled: e.target.checked })}
                  className="mt-1"
                />
              }
            />
            <CompactRow
              label="名称"
              description={`配置 ID: ${selected.id}`}
              control={
                <Input
                  value={selected.name}
                  onChange={(e) => updateProvider({ name: e.target.value })}
                  className="w-[220px]"
                />
              }
            />
            <CompactRow
              label="类型"
              description="决定拉取模型和认证方式"
              control={
                <select
                  value={selected.providerType}
                  onChange={(e) =>
                    updateProvider({ providerType: e.target.value as ProviderKind })
                  }
                  className="h-9 w-[220px] rounded-md border border-border-default bg-input-bg px-3 text-sm text-text-primary outline-none focus:border-border-focus"
                >
                  {KIND_OPTIONS.map((k) => (
                    <option key={k.id} value={k.id}>
                      {k.label}
                    </option>
                  ))}
                </select>
              }
            />
            <CompactRow
              label="Base URL"
              description="填供应商官方或 OpenAI 兼容端点"
              control={
                <Input
                  value={selected.baseUrl}
                  onChange={(e) => updateProvider({ baseUrl: e.target.value })}
                  className="w-[220px]"
                />
              }
            />
            <CompactRow
              label="API Key"
              description={`当前: ${selected.apiKeyPresent ? shortKey(providerKey(selected)) : "未填写"}`}
              control={
                <Input
                  type="password"
                  value={providerKey(selected)}
                  onChange={(e) =>
                    updateProvider({
                      apiKey: e.target.value,
                      apiKeyPresent: e.target.value.trim().length > 0,
                    })
                  }
                  placeholder="sk-..."
                  className="w-[220px]"
                />
              }
            />
            <CompactRow
              label="默认模型"
              description="新建对话默认使用这个供应商和模型"
              control={
                <select
                  value={selected.id === defaultProviderId ? defaultModel : ""}
                  onChange={(e) => {
                    setDefaultProviderId(selected.id);
                    setDefaultModel(e.target.value);
                  }}
                  className="h-9 w-[220px] rounded-md border border-border-default bg-input-bg px-3 text-sm text-text-primary outline-none focus:border-border-focus"
                >
                  <option value="" disabled>
                    选择模型
                  </option>
                  {enabledModels.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label || m.id}
                    </option>
                  ))}
                </select>
              }
            />
            <CompactRow
              label="连接"
              description="测试配置，或从供应商模型接口同步最新模型名"
              control={
                <div className="flex max-w-[300px] flex-wrap items-center justify-end gap-2">
                  {testResult &&
                    (testResult.ok ? (
                      <Pill tone="success" icon={CheckIcon}>
                        {testResult.latencyMs}ms · {testResult.modelCount} 个模型
                      </Pill>
                    ) : (
                      <Pill tone="danger" icon={CloseIcon}>
                        {testResult.error}
                      </Pill>
                    ))}
                  {syncError && (
                    <span
                      title={syncError}
                      className="inline-flex h-7 max-w-[160px] items-center gap-1 rounded-md border border-danger/30 bg-danger/10 px-2 text-xs text-danger"
                    >
                      <Icon icon={CloseIcon} size={12} />
                      <span className="min-w-0 truncate">同步失败</span>
                    </span>
                  )}
                  <Button
                    size="sm"
                    variant="secondary"
                    icon={RefreshIcon}
                    disabled={testing}
                    onClick={() => void testConnection()}
                  >
                    {testing ? "测试中" : "测试"}
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    icon={RefreshIcon}
                    disabled={fetching}
                    onClick={() => void fetchModels()}
                  >
                    {fetching ? "同步中" : "同步模型"}
                  </Button>
                </div>
              }
            />
          </Section>

          <Section title="模型列表" className="mb-0 [&>h2]:mb-1">
            {selected.models.map((m) => (
              <div
                key={m.id}
                className="px-3 py-2 flex items-center gap-2"
              >
                <input
                  type="checkbox"
                  checked={m.enabled}
                  onChange={(e) => updateModel(m.id, { enabled: e.target.checked })}
                />
                <Input
                  value={m.label}
                  onChange={(e) => updateModel(m.id, { label: e.target.value })}
                  className="w-[160px]"
                />
                <div className="min-w-0 flex-1 truncate text-xs font-mono text-text-tertiary">
                  {m.id}
                </div>
                {selected.id === defaultProviderId && m.id === defaultModel && (
                  <Pill tone="brand" icon={CheckIcon} size="sm">
                    默认
                  </Pill>
                )}
              </div>
            ))}
            <div className="px-3 py-2 flex items-center gap-2">
              <Input
                value={newModelId}
                onChange={(e) => setNewModelId(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") addManualModel();
                }}
                placeholder="手动添加模型 ID"
                className="w-[220px]"
              />
              <Button size="sm" variant="secondary" icon={PlusIcon} onClick={addManualModel}>
                添加
              </Button>
            </div>
          </Section>
        </div>
        <div className="absolute bottom-0 left-0 w-[180px]">
          <Button
            size="sm"
            variant="primary"
            fullWidth
            onClick={() => void save()}
            disabled={saving}
          >
            {saving ? "保存中" : saved ? "已保存" : "保存配置"}
          </Button>
        </div>
      </div>
    </div>
  );
}
