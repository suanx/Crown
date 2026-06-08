import { create } from "zustand";
import type { ConfigPatch, ThemeMode, PermissionMode } from "@/api/contracts";
import { agentClient, apiMode } from "@/api";

/**
 * Settings Store — 全局配置持久化.
 *
 * 双轨存储:
 *   - mock/hybrid dev 模式: localStorage (前端可独立开发,无需 Rust 后端)
 *   - tauri 生产模式: Tauri IPC → Rust 端写 JSON 到 app_config_dir()
 *
 * 存储路径:
 *   开发: localStorage key "ds.settings.v1"
 *   生产: %APPDATA%/com.deepseek-agent/config.json (由 Rust 管理)
 *
 * API Key 安全:
 *   - localStorage 中的 key 仅用于开发/原型阶段
 *   - 生产版 key 由 Rust 端加密存储 (keyring / encrypted file)
 *   - 前端永远不明文展示完整 key,仅显示 sk-...xxxx 掩码
 */

const STORAGE_KEY = "ds.settings.v1";

// ── 默认值 ─────────────────────────────────────────────────────────────────
const DEFAULT_SETTINGS: SettingsData = {
  provider: {
    apiKey: "",
    baseUrl: "https://api.deepseek.com",
  },
  defaultModel: "deepseek-chat",
  permissionMode: "default",
  theme: "dark",
  language: "zh",
  ui: {
    enterToSend: true,
    autoScroll: true,
    collapseReasoningOnComplete: true,
    showBalanceInSidebar: true,
    showMessageCost: false,
    fontSize: "medium",
  },
  budget: {
    mode: "unlimited",
    limitUsd: null,
  },
  compaction: {
    triggerRatio: 0.85,
    keepRecentTurns: 4,
  },
  shell: {
    timeoutSecs: 120,
    maxOutputBytes: 1048576,
  },
};

// ── 类型 ────────────────────────────────────────────────────────────────────

export interface ProviderConfig {
  apiKey: string;
  baseUrl: string;
}

export interface UiPreferences {
  enterToSend: boolean;
  autoScroll: boolean;
  collapseReasoningOnComplete: boolean;
  showBalanceInSidebar: boolean;
  showMessageCost: boolean;
  fontSize: "small" | "medium" | "large";
}

export interface SettingsData {
  provider: ProviderConfig;
  defaultModel: string;
  permissionMode: PermissionMode;
  theme: ThemeMode;
  language: "zh" | "en";
  ui: UiPreferences;
  budget: {
    mode: "per_session" | "per_day" | "unlimited";
    limitUsd: number | null;
  };
  compaction: {
    triggerRatio: number;
    keepRecentTurns: number;
  };
  shell: {
    timeoutSecs: number;
    maxOutputBytes: number;
  };
}

export type TestConnectionResult =
  | { ok: true; model: string; latencyMs: number }
  | { ok: false; error: string };

interface SettingsState extends SettingsData {
  /** 是否已从持久化加载 */
  loaded: boolean;

  /** 加载配置 (启动时调一次) */
  load: () => Promise<void>;

  /** 更新部分配置 (自动持久化) */
  update: (patch: Partial<SettingsData>) => void;

  /** 更新 provider 配置 */
  updateProvider: (patch: Partial<ProviderConfig>) => void;

  /** 更新 UI 偏好 */
  updateUi: (patch: Partial<UiPreferences>) => void;

  /** 测试 API 连接 */
  testConnection: () => Promise<TestConnectionResult>;

  /** 把当前配置同步给后端 (tauri 模式) */
  syncToBackend: () => Promise<void>;
}

// ── 本地持久化 ──────────────────────────────────────────────────────────────

function loadFromLocal(): SettingsData {
  if (typeof window === "undefined") return DEFAULT_SETTINGS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const parsed = JSON.parse(raw) as Partial<SettingsData>;
    // 深度 merge,防止新字段缺失
    return {
      ...DEFAULT_SETTINGS,
      ...parsed,
      provider: { ...DEFAULT_SETTINGS.provider, ...parsed.provider },
      ui: { ...DEFAULT_SETTINGS.ui, ...parsed.ui },
      budget: { ...DEFAULT_SETTINGS.budget, ...parsed.budget },
      compaction: { ...DEFAULT_SETTINGS.compaction, ...parsed.compaction },
      shell: { ...DEFAULT_SETTINGS.shell, ...parsed.shell },
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function saveToLocal(data: SettingsData) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch {
    /* quota / private mode */
  }
}

// ── Store ────────────────────────────────────────────────────────────────────

export const useSettingsStore = create<SettingsState>((set, get) => {
  function getSettingsData(): SettingsData {
    const s = get();
    return {
      provider: s.provider,
      defaultModel: s.defaultModel,
      permissionMode: s.permissionMode,
      theme: s.theme,
      language: s.language,
      ui: s.ui,
      budget: s.budget,
      compaction: s.compaction,
      shell: s.shell,
    };
  }

  function persist(data: SettingsData) {
    saveToLocal(data);
  }

  return {
    ...DEFAULT_SETTINGS,
    loaded: false,

    load: async () => {
      if (apiMode === "tauri") {
        // 生产模式: 从 Rust 后端拉配置
        try {
          const config = await agentClient.getConfig();
          const data: SettingsData = {
            ...DEFAULT_SETTINGS,
            provider: {
              apiKey: "", // 后端不回传明文 key,仅 apiKeyPresent
              baseUrl: config.baseUrl,
            },
            defaultModel: config.defaultModel,
            permissionMode: config.permissionMode,
            theme: config.theme,
            language: config.language,
            budget: config.budget,
            compaction: config.compaction,
            shell: config.shell,
          };
          set({ ...data, loaded: true });
          // 同时缓存到 local 作为 fallback
          saveToLocal(data);
        } catch {
          // fallback to local
          const local = loadFromLocal();
          set({ ...local, loaded: true });
        }
      } else {
        // dev 模式: 读 localStorage
        const local = loadFromLocal();
        set({ ...local, loaded: true });
      }
    },

    update: (patch) => {
      const current = getSettingsData();
      const next = { ...current, ...patch };
      set(patch);
      persist(next);
      void get().syncToBackend();
    },

    updateProvider: (patch) => {
      const current = get().provider;
      const next = { ...current, ...patch };
      set({ provider: next });
      const data = { ...getSettingsData(), provider: next };
      persist(data);
      void get().syncToBackend();
    },

    updateUi: (patch) => {
      const current = get().ui;
      const next = { ...current, ...patch };
      set({ ui: next });
      const data = { ...getSettingsData(), ui: next };
      persist(data);
      void get().syncToBackend();
    },

    testConnection: async () => {
      const { provider } = get();
      const apiKey = provider.apiKey;
      const baseUrl = provider.baseUrl || "https://api.deepseek.com";

      if (!apiKey) {
        return { ok: false, error: "未填写 API Key" };
      }

      const start = performance.now();
      try {
        // 用 /models 端点测试连接 (轻量,不消耗 tokens)
        const resp = await fetch(`${baseUrl}/models`, {
          method: "GET",
          headers: {
            Authorization: `Bearer ${apiKey}`,
            "Content-Type": "application/json",
          },
        });

        const latencyMs = Math.round(performance.now() - start);

        if (!resp.ok) {
          const body = await resp.text().catch(() => "");
          if (resp.status === 401) {
            return { ok: false, error: "API Key 无效 (401 Unauthorized)" };
          }
          if (resp.status === 403) {
            return { ok: false, error: "权限不足 (403 Forbidden)" };
          }
          return {
            ok: false,
            error: `HTTP ${resp.status}: ${body.slice(0, 200)}`,
          };
        }

        // 解析返回的 models 列表,取第一个作为确认
        const json = await resp.json() as { data?: Array<{ id: string }> };
        const firstModel = json.data?.[0]?.id ?? "unknown";

        return { ok: true, model: firstModel, latencyMs };
      } catch (err) {
        const latencyMs = Math.round(performance.now() - start);
        const msg =
          err instanceof Error ? err.message : "未知网络错误";
        // 区分常见错误
        if (msg.includes("Failed to fetch") || msg.includes("NetworkError")) {
          return {
            ok: false,
            error: `网络不可达 (${latencyMs}ms) — 请检查 Base URL 或代理设置`,
          };
        }
        return { ok: false, error: msg };
      }
    },

    syncToBackend: async () => {
      if (apiMode !== "tauri" && apiMode !== "hybrid") return;
      const data = getSettingsData();
      const patch: ConfigPatch = {
        apiKey: data.provider.apiKey || undefined,
        baseUrl: data.provider.baseUrl,
        defaultModel: data.defaultModel,
        permissionMode: data.permissionMode,
        theme: data.theme,
        language: data.language,
        budget: data.budget,
        compaction: data.compaction,
        shell: data.shell,
      };
      try {
        await agentClient.setConfig(patch);
      } catch (err) {
        if (import.meta.env.DEV) {
          // eslint-disable-next-line no-console
          console.warn("[settingsStore] syncToBackend failed:", err);
        }
      }
    },
  };
});
