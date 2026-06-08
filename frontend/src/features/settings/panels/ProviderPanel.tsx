import { useState } from "react";
import {
  useSettingsStore,
  type TestConnectionResult,
} from "@/stores/settingsStore";
import { PanelTitle, Section, Row } from "./_shared";
import { Input } from "@/shared/ui/Input";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { CheckIcon, RefreshIcon, CloseIcon } from "@/shared/icons/set";

/**
 * Provider 设置面板 — 真实配置 + 测试连接.
 *
 * 数据流:
 *   1. 用户填写 API Key / Base URL → settingsStore.updateProvider()
 *   2. settingsStore 自动持久化到 localStorage (dev) 或 Tauri IPC (prod)
 *   3. "测试连接" → settingsStore.testConnection() → fetch /models 验证
 *   4. "保存" → syncToBackend() 把配置推到 Rust 端
 */
export function ProviderPanel() {
  const provider = useSettingsStore((s) => s.provider);
  const updateProvider = useSettingsStore((s) => s.updateProvider);
  const testConnection = useSettingsStore((s) => s.testConnection);
  const syncToBackend = useSettingsStore((s) => s.syncToBackend);

  // 本地编辑态 (实时 debounce 太频繁,用 blur 保存)
  const [localKey, setLocalKey] = useState(provider.apiKey);
  const [localUrl, setLocalUrl] = useState(provider.baseUrl);

  // 测试连接状态
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestConnectionResult | null>(
    null,
  );

  // 保存状态
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  function handleKeyBlur() {
    if (localKey !== provider.apiKey) {
      updateProvider({ apiKey: localKey });
    }
  }

  function handleUrlBlur() {
    if (localUrl !== provider.baseUrl) {
      updateProvider({ baseUrl: localUrl });
    }
  }

  async function handleTest() {
    // 先保存当前输入
    updateProvider({ apiKey: localKey, baseUrl: localUrl });
    setTesting(true);
    setTestResult(null);
    const result = await testConnection();
    setTestResult(result);
    setTesting(false);
  }

  async function handleSave() {
    updateProvider({ apiKey: localKey, baseUrl: localUrl });
    setSaving(true);
    await syncToBackend();
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }

  const keyMasked =
    provider.apiKey && provider.apiKey.length > 8
      ? `${provider.apiKey.slice(0, 5)}...${provider.apiKey.slice(-4)}`
      : "";

  return (
    <div>
      <PanelTitle title="Provider" description="API 密钥与服务地址配置" />

      <Section title="DeepSeek">
        <Row
          label="API Key"
          description={
            keyMasked
              ? `当前已保存: ${keyMasked}`
              : "填入 DeepSeek API 密钥,保存后生效"
          }
          control={
            <Input
              type="password"
              placeholder="sk-..."
              value={localKey}
              onChange={(e) => setLocalKey(e.target.value)}
              onBlur={handleKeyBlur}
              className="w-[280px]"
            />
          }
        />
        <Row
          label="Base URL"
          description="OpenAI 兼容端点 (默认 https://api.deepseek.com)"
          control={
            <Input
              placeholder="https://api.deepseek.com"
              value={localUrl}
              onChange={(e) => setLocalUrl(e.target.value)}
              onBlur={handleUrlBlur}
              className="w-[280px]"
            />
          }
        />
        <Row
          label="连接测试"
          description="验证 API Key 与网络可达性 (GET /models)"
          control={
            <div className="flex items-center gap-2">
              {testResult && (
                testResult.ok ? (
                  <Pill tone="success" icon={CheckIcon}>
                    {testResult.latencyMs}ms · {testResult.model}
                  </Pill>
                ) : (
                  <Pill tone="danger" icon={CloseIcon}>
                    {testResult.error}
                  </Pill>
                )
              )}
              <Button
                variant="secondary"
                icon={RefreshIcon}
                onClick={() => void handleTest()}
                disabled={testing}
              >
                {testing ? "测试中..." : "测试连接"}
              </Button>
            </div>
          }
        />
      </Section>

      {/* 保存按钮 */}
      <div className="mt-6 flex items-center gap-3">
        <Button
          variant="primary"
          onClick={() => void handleSave()}
          disabled={saving}
        >
          {saving ? "保存中..." : saved ? "✓ 已保存" : "保存配置"}
        </Button>
        <span className="text-xs text-text-tertiary">
          配置保存到本地,重启后自动加载
        </span>
      </div>

      <Section title="其他 Provider">
        <Row
          label="OpenAI 兼容服务"
          description="如 Ollama / Claude proxy / 自建 LLM 网关 (修改 Base URL 即可)"
          control={
            <span className="text-xs text-text-tertiary">
              修改上方 Base URL 指向你的服务端点
            </span>
          }
        />
      </Section>
    </div>
  );
}
