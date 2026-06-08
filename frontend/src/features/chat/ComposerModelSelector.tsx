import { useEffect, useRef, useState } from "react";
import { agentClient, type ModelInfo, type ThinkingEffort } from "@/api";
import { useActiveThread, useChatStore } from "@/stores/chatStore";
import { useSessionStore } from "@/stores/sessionStore";
import { useUiStore } from "@/stores/uiStore";
import { Icon } from "@/shared/icons/Icon";
import { ProviderIcon } from "@/shared/ui/ProviderIcon";
import {
  CaretDownIcon,
  CaretUpIcon,
  FlashIcon,
  BrandIcon,
  CheckIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

/**
 * 嵌入 ComposeBar 底部的模型选择器.
 * 视觉对齐 Claude Code:小字 + 倒三角,无边框,hover 才高亮.
 * 弹层向上展开 (因为底部空间小).
 */

const MODELS = [
  { id: "deepseek-v4-flash", providerId: "deepseek", label: "v4-flash", description: "便宜快速 · 默认", icon: FlashIcon },
  { id: "deepseek-v4-pro", providerId: "deepseek", label: "v4-pro", description: "强推理 · 复杂任务", icon: BrandIcon },
] as const;

const THINKING_OPTIONS: Array<{ id: ThinkingEffort; label: string }> = [
  { id: "low", label: "低" },
  { id: "medium", label: "中" },
  { id: "high", label: "高" },
  { id: "ultra", label: "超高" },
];

function effortIndex(value: ThinkingEffort) {
  return Math.max(
    0,
    THINKING_OPTIONS.findIndex((item) => item.id === value),
  );
}

function normalizeModel(model: ModelInfo) {
  const known = MODELS.find((m) => m.id === model.id);
  return {
    id: model.id,
    providerId: model.providerId,
    label: known?.label ?? model.label.replace(/^DeepSeek\s+/i, "").replace(/^V4\s+/i, "v4-"),
    description: known?.description ?? model.description,
    icon: known?.icon ?? (model.id.includes("pro") ? BrandIcon : FlashIcon),
  };
}

export function ComposerModelSelector() {
  const [open, setOpen] = useState(false);
  const [loadingModels, setLoadingModels] = useState(false);
  const [models, setModels] = useState<Array<ReturnType<typeof normalizeModel>>>(
    MODELS.map((m) => ({ ...m })),
  );
  const ref = useRef<HTMLDivElement>(null);
  const activeThread = useActiveThread();
  const reloadThread = useChatStore((s) => s.reloadThread);
  const currentUiModel = useUiStore((s) => s.currentModel);
  const setCurrent = useUiStore((s) => s.setCurrentModel);
  const currentUiProviderId = useUiStore((s) => s.currentProviderId);
  const setCurrentProviderId = useUiStore((s) => s.setCurrentProviderId);
  const currentUiThinking = useUiStore((s) => s.currentThinkingEffort);
  const setCurrentThinkingEffort = useUiStore((s) => s.setCurrentThinkingEffort);
  const current = activeThread?.model ?? currentUiModel;
  const currentProviderId = activeThread?.providerId ?? currentUiProviderId;
  const thinking = activeThread?.thinkingEffort ?? currentUiThinking;
  const thinkingIndex = effortIndex(thinking);
  const active =
    models.find(
      (m) =>
        m.id === current &&
        (!currentProviderId || m.providerId === currentProviderId),
    ) ??
    models.find((m) => m.id === current) ??
    models[0];

  async function updateThinkingEffort(index: number) {
    const item = THINKING_OPTIONS[index];
    if (!item) return;
    setCurrentThinkingEffort(item.id);
    if (!activeThread) return;
    await agentClient.updateThread({
      threadId: activeThread.id,
      thinkingEffort: item.id,
    });
    await reloadThread(activeThread.id);
  }

  async function loadModels() {
    setLoadingModels(true);
    try {
      const items = await agentClient.listModels();
      if (items.length > 0) setModels(items.map(normalizeModel));
    } catch {
      // 模型列表拉取失败时保留内置兜底项。
    } finally {
      setLoadingModels(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    setLoadingModels(true);
    void agentClient
      .listModels()
      .then((items) => {
        if (!cancelled && items.length > 0) {
          setModels(items.map(normalizeModel));
        }
      })
      .catch(() => {
        // 模型列表拉取失败时保留内置兜底项。
      })
      .finally(() => {
        if (!cancelled) setLoadingModels(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => {
          setOpen((v) => {
            const next = !v;
            if (next) void loadModels();
            return next;
          });
        }}
        className={cn(
          "h-7 px-2 inline-flex items-center gap-1 rounded-md text-xs transition-colors focus-ring",
          "text-text-secondary hover:bg-hover hover:text-text-primary",
          open && "bg-hover text-text-primary",
        )}
      >
        <ProviderIcon providerId={active.providerId} name={active.providerId} size={12} />
        <span className="font-medium">{active.label}</span>
        <Icon
          icon={open ? CaretUpIcon : CaretDownIcon}
          size={10}
          className="opacity-60"
        />
      </button>

      {open && (
        <div
          className="absolute bottom-full right-0 mb-1 w-56 py-1 bg-overlay border border-border-default rounded-md z-30 animate-scale-in"
          style={{ boxShadow: "var(--ds-shadow-md)" }}
        >
          {loadingModels && (
            <div className="px-3 py-2 text-xs text-text-tertiary">
              正在刷新模型...
            </div>
          )}
          {models.map((m) => (
            <button
              key={`${m.providerId}:${m.id}`}
              onClick={async () => {
                setCurrent(m.id);
                setCurrentProviderId(m.providerId);
                if (activeThread) {
                  await agentClient.switchModel(activeThread.id, m.id, m.providerId);
                  await reloadThread(activeThread.id);
                  void useSessionStore.getState().loadThreads();
                }
                setOpen(false);
              }}
              className="w-full px-3 py-2 flex items-start gap-3 hover:bg-hover transition-colors text-left"
            >
              <ProviderIcon
                providerId={m.providerId}
                name={m.providerId}
                size={14}
                className="mt-0.5"
              />
              <div className="flex-1 min-w-0">
                <div className="text-sm text-text-primary font-medium">
                  {m.label}
                </div>
                <div className="text-xs text-text-tertiary truncate">
                  {m.description}
                </div>
              </div>
              {current === m.id &&
                (!currentProviderId || currentProviderId === m.providerId) && (
                <Icon icon={CheckIcon} size={14} className="text-brand mt-0.5" />
              )}
            </button>
          ))}
          <div className="my-1 h-px bg-border-subtle" />
          <div className="px-3 py-2">
            <div className="mb-1 text-xs text-text-tertiary">推理</div>
            <div className="relative h-7 overflow-hidden rounded-md border border-border-subtle bg-input-bg">
              <div
                className="absolute left-0 top-0 h-full w-1/4 rounded-[5px] bg-hover transition-transform"
                style={{ transform: `translateX(${thinkingIndex * 100}%)` }}
              />
              <div className="pointer-events-none absolute inset-0 grid grid-cols-4">
                {THINKING_OPTIONS.map((item, index) => (
                  <div
                    key={item.id}
                    className={cn(
                      "flex items-center justify-center text-xs transition-colors",
                      index === thinkingIndex
                        ? "text-text-primary"
                        : "text-text-tertiary",
                    )}
                  >
                    {item.label}
                  </div>
                ))}
              </div>
              <input
                type="range"
                min={0}
                max={3}
                step={1}
                value={thinkingIndex}
                onChange={(e) => void updateThinkingEffort(Number(e.target.value))}
                className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
              />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
