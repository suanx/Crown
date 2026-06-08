import { useEffect, useMemo, useState } from "react";
import {
  agentClient,
  type AppConfig,
  type WebSearchProviderConfig,
} from "@/api";
import { PanelTitle, Section } from "./_shared";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { Pill } from "@/shared/ui/Pill";
import { Icon } from "@/shared/icons/Icon";
import { CheckIcon, GlobeIcon, LockIcon } from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

function cloneProviders(items: WebSearchProviderConfig[]) {
  return items.map((p) => ({ ...p }));
}

function providerStatus(provider: WebSearchProviderConfig) {
  if (!provider.implemented) return { label: "未接入", tone: "neutral" as const };
  if (provider.keyRequired && !provider.apiKeyPresent && !provider.apiKey) {
    return { label: "需要 API key", tone: "neutral" as const };
  }
  return { label: "可用", tone: "success" as const };
}

function keyValue(provider: WebSearchProviderConfig) {
  return provider.apiKey ?? "";
}

export function WebSearchPanel() {
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<WebSearchProviderConfig[]>([]);
  const [defaultProviderId, setDefaultProviderId] = useState("jina");
  const [selectedId, setSelectedId] = useState("jina");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void agentClient
      .getConfig()
      .then((cfg: AppConfig) => {
        if (cancelled) return;
        const list = cloneProviders(cfg.webSearch?.providers ?? []);
        setProviders(list);
        setDefaultProviderId(cfg.webSearch?.defaultProviderId || "jina");
        setSelectedId(cfg.webSearch?.defaultProviderId || list[0]?.id || "jina");
      })
      .catch((error) => {
        if (!cancelled) {
          setSyncError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const selected = useMemo(
    () => providers.find((p) => p.id === selectedId) ?? providers[0],
    [providers, selectedId],
  );

  function updateProvider(id: string, patch: Partial<WebSearchProviderConfig>) {
    setProviders((items) =>
      items.map((p) => (p.id === id ? { ...p, ...patch } : p)),
    );
  }

  async function save() {
    setSaving(true);
    setSyncError(null);
    const normalized = providers.map((provider) => ({
      ...provider,
      enabled:
        provider.implemented &&
        (provider.enabled || provider.id === defaultProviderId),
    }));
    try {
      const cfg = await agentClient.saveWebSearchConfig({
        defaultProviderId,
        providers: normalized,
      });
      const returned = cloneProviders(cfg.webSearch?.providers ?? normalized);
      setProviders(
        returned.map((p) => {
          const local = normalized.find((item) => item.id === p.id);
          return { ...p, apiKey: p.apiKey ?? local?.apiKey ?? null };
        }),
      );
      setDefaultProviderId(cfg.webSearch?.defaultProviderId ?? defaultProviderId);
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
        <PanelTitle title="联网搜索" description="正在读取搜索供应商配置" />
      </div>
    );
  }

  return (
    <div>
      <PanelTitle
        title="联网搜索"
        description="管理 web_search 工具使用的搜索供应商和 API key。"
      />

      <Section title="供应商">
        <div className="grid grid-cols-2 gap-2 p-3">
          {providers.map((provider) => {
            const status = providerStatus(provider);
            const active = provider.id === selected.id;
            return (
              <button
                key={provider.id}
                type="button"
                onClick={() => setSelectedId(provider.id)}
                className={cn(
                  "min-h-[76px] rounded-md border p-3 text-left transition-colors focus-ring",
                  active
                    ? "border-border-focus bg-hover"
                    : "border-border-subtle bg-surface hover:bg-hover",
                )}
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <Icon icon={GlobeIcon} size={14} className="text-brand" />
                      <span className="truncate text-sm font-medium text-text-primary">
                        {provider.name}
                      </span>
                    </div>
                    <div className="mt-2 flex flex-wrap gap-1.5">
                      <Pill tone={status.tone}>{status.label}</Pill>
                      {provider.id === defaultProviderId && (
                        <Pill tone="brand">默认</Pill>
                      )}
                    </div>
                  </div>
                  {!provider.implemented && (
                    <Icon icon={LockIcon} size={14} className="text-text-tertiary" />
                  )}
                </div>
              </button>
            );
          })}
        </div>
      </Section>

      <Section title={selected.name}>
        <div className="px-4 py-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium text-text-primary">
                默认搜索供应商
              </div>
              <div className="mt-0.5 text-xs leading-snug text-text-tertiary">
                当前对话调用 web_search 时会使用这个供应商。
              </div>
            </div>
            <Button
              size="sm"
              variant={selected.id === defaultProviderId ? "primary" : "secondary"}
              disabled={!selected.implemented}
              onClick={() => setDefaultProviderId(selected.id)}
              icon={selected.id === defaultProviderId ? CheckIcon : undefined}
            >
              {selected.id === defaultProviderId ? "已设为默认" : "设为默认"}
            </Button>
          </div>
        </div>

        <div className="px-4 py-3">
          <div className="mb-2 flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-medium text-text-primary">API key</div>
              <div className="mt-0.5 text-xs leading-snug text-text-tertiary">
                {selected.keyRequired
                  ? "此供应商需要 API key 才能调用。"
                  : "可留空；有 key 时会优先使用官方 API。"}
              </div>
            </div>
            {selected.apiKeyPresent && !selected.apiKey && (
              <Pill tone="success">已保存</Pill>
            )}
          </div>
          <Input
            type="password"
            value={keyValue(selected)}
            disabled={!selected.implemented || selected.id === "duckduckgo"}
            placeholder={
              selected.id === "duckduckgo"
                ? "无需 API key"
                : selected.apiKeyPresent
                  ? "已保存，输入新 key 可替换，清空后保存可移除"
                  : "粘贴 API key"
            }
            onChange={(event) =>
              updateProvider(selected.id, {
                apiKey: event.target.value,
                apiKeyPresent: event.target.value.trim().length > 0,
              })
            }
          />
        </div>

        <div className="px-4 py-3">
          <div className="text-sm font-medium text-text-primary">状态</div>
          <div className="mt-1 text-xs leading-snug text-text-tertiary">
            {selected.note ?? "已接入。"}
          </div>
        </div>
      </Section>

      {syncError && (
        <div className="mb-4 rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-sm text-danger">
          {syncError}
        </div>
      )}

      <div className="flex items-center justify-end gap-2">
        {saved && <Pill tone="success">已保存</Pill>}
        <Button variant="primary" onClick={save} disabled={saving}>
          {saving ? "保存中" : "保存联网搜索设置"}
        </Button>
      </div>
    </div>
  );
}
