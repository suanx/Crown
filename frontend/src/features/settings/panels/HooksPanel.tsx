import { useEffect, useMemo, useState } from "react";
import { agentClient } from "@/api";
import type {
  HookCommandConfig,
  HookConfigFile,
  HookEventInfo,
  HookMatcherConfig,
  HookScope,
  HookTraceEntry,
} from "@/api";
import { Button } from "@/shared/ui/Button";
import { Icon } from "@/shared/icons/Icon";
import {
  CaretDownIcon,
  CheckIcon,
  PlusIcon,
  RefreshIcon,
  TrashIcon,
} from "@/shared/icons/set";
import { PanelTitle, Section } from "./_shared";

const emptyConfig = (): HookConfigFile => ({
  disableAllHooks: false,
  trustedProjects: [],
  hooks: {},
});

const newHook = (): HookCommandConfig => ({
  id: `hook-${Date.now()}`,
  type: "command",
  command: "",
  shell: null,
  timeout: 10,
  enabled: true,
});

const inputClass =
  "h-8 rounded-md border border-border-subtle bg-input-bg px-2 text-xs text-text-primary placeholder:text-text-disabled outline-none focus:border-brand";
const textareaClass =
  "min-h-[64px] rounded-md border border-border-subtle bg-input-bg px-2 py-2 text-xs text-text-primary placeholder:text-text-disabled outline-none focus:border-brand font-mono";
const hookTypes: HookCommandConfig["type"][] = ["command", "prompt", "agent", "http"];

export function HooksPanel() {
  const [events, setEvents] = useState<HookEventInfo[]>([]);
  const [scope, setScope] = useState<HookScope>("global");
  const [projectPath, setProjectPath] = useState("");
  const [trusted, setTrusted] = useState(false);
  const [selectedEvent, setSelectedEvent] = useState("PreToolUse");
  const [config, setConfig] = useState<HookConfigFile>(emptyConfig);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [testResult, setTestResult] = useState<HookTraceEntry | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventMenuOpen, setEventMenuOpen] = useState(false);
  const [typeMenuKey, setTypeMenuKey] = useState<string | null>(null);

  useEffect(() => {
    void agentClient.listHookEvents().then((list) => {
      setEvents(list);
      if (list[0] && !list.some((event) => event.id === selectedEvent)) {
        setSelectedEvent(list[0].id);
      }
    });
    void agentClient.fsGetWorkspaceRoot().then(setProjectPath).catch(() => {});
  }, []);

  useEffect(() => {
    void loadConfig();
  }, [scope, projectPath]);

  const groups = useMemo(
    () => config.hooks[selectedEvent] ?? [],
    [config.hooks, selectedEvent],
  );

  async function loadConfig() {
    if (scope === "project" && !projectPath.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const next = await agentClient.getHooksConfig({
        scope,
        projectPath: scope === "project" ? projectPath : null,
      });
      setConfig({
        disableAllHooks: next.disableAllHooks ?? false,
        trustedProjects: next.trustedProjects ?? [],
        hooks: next.hooks ?? {},
      });
      if (scope === "project") {
        const trust = await agentClient.getProjectHooksTrust(projectPath);
        setTrusted(trust.trusted);
      }
      setDirty(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  function updateGroups(nextGroups: HookMatcherConfig[]) {
    setConfig((current) => ({
      ...current,
      hooks: {
        ...current.hooks,
        [selectedEvent]: nextGroups,
      },
    }));
    setDirty(true);
  }

  function addGroup() {
    updateGroups([...groups, { matcher: "*", hooks: [newHook()] }]);
  }

  function removeGroup(index: number) {
    updateGroups(groups.filter((_, i) => i !== index));
  }

  function updateGroup(index: number, patch: Partial<HookMatcherConfig>) {
    updateGroups(
      groups.map((group, i) => (i === index ? { ...group, ...patch } : group)),
    );
  }

  function updateHook(
    groupIndex: number,
    hookIndex: number,
    patch: Partial<HookCommandConfig>,
  ) {
    updateGroups(
      groups.map((group, i) => {
        if (i !== groupIndex) return group;
        return {
          ...group,
          hooks: group.hooks.map((hook, j) =>
            j === hookIndex ? { ...hook, ...patch } : hook,
          ),
        };
      }),
    );
  }

  function addHook(groupIndex: number) {
    updateGroups(
      groups.map((group, i) =>
        i === groupIndex ? { ...group, hooks: [...group.hooks, newHook()] } : group,
      ),
    );
  }

  function removeHook(groupIndex: number, hookIndex: number) {
    updateGroups(
      groups.map((group, i) =>
        i === groupIndex
          ? { ...group, hooks: group.hooks.filter((_, j) => j !== hookIndex) }
          : group,
      ),
    );
  }

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const saved = await agentClient.saveHooksConfig({
        scope,
        projectPath: scope === "project" ? projectPath : null,
        config,
      });
      setConfig(saved);
      setDirty(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  async function toggleTrust(next: boolean) {
    if (!projectPath.trim()) return;
    const result = await agentClient.setProjectHooksTrust(projectPath, next);
    setTrusted(result.trusted);
  }

  async function test(group: HookMatcherConfig, hook: HookCommandConfig) {
    setTestResult(null);
    setError(null);
    try {
      const result = await agentClient.testHook({
        event: selectedEvent,
        matcher: group.matcher ?? null,
        cwd: projectPath || null,
        hook,
        input: {
          hook_event_name: selectedEvent,
          tool_name: group.matcher || "run_command",
          tool_input: {},
          prompt: "测试 hook",
        },
      });
      setTestResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  const selectedInfo = events.find((event) => event.id === selectedEvent);

  return (
    <div>
      <PanelTitle
        title="Hooks"
        description="配置工具调用前后、提示词提交、权限请求等事件触发的命令 hook"
      />

      <Section>
        <div className="p-3 flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => setScope("global")}
            className={`h-7 rounded-md px-3 text-xs ${
              scope === "global"
                ? "bg-brand text-white"
                : "bg-input-bg text-text-secondary hover:bg-hover"
            }`}
          >
            全局
          </button>
          <button
            type="button"
            onClick={() => setScope("project")}
            className={`h-7 rounded-md px-3 text-xs ${
              scope === "project"
                ? "bg-brand text-white"
                : "bg-input-bg text-text-secondary hover:bg-hover"
            }`}
          >
            项目
          </button>
          <div className="relative">
            <button
              type="button"
              className="h-7 w-48 rounded-md border border-border-subtle bg-input-bg px-3 text-left text-xs text-text-primary hover:bg-hover focus-ring inline-flex items-center justify-between"
              onClick={() => setEventMenuOpen((open) => !open)}
            >
              <span className="truncate">{selectedEvent}</span>
              <Icon icon={CaretDownIcon} size={13} className="text-text-tertiary" />
            </button>
            {eventMenuOpen && (
              <div className="absolute left-0 top-8 z-50 w-48 max-h-72 overflow-y-auto rounded-md border border-border-subtle bg-elevated shadow-xl py-1">
                {events.map((event) => {
                  const active = event.id === selectedEvent;
                  return (
                    <button
                      key={event.id}
                      type="button"
                      className={`w-full h-8 px-3 text-left text-xs flex items-center justify-between ${
                        active
                          ? "bg-brand/20 text-text-primary"
                          : "text-text-secondary hover:bg-hover hover:text-text-primary"
                      }`}
                      onClick={() => {
                        setSelectedEvent(event.id);
                        setEventMenuOpen(false);
                      }}
                    >
                      <span className="truncate">{event.label}</span>
                      {active && <Icon icon={CheckIcon} size={13} />}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
          <label className="ml-auto flex items-center gap-2 text-xs text-text-secondary">
            <input
              type="checkbox"
              checked={config.disableAllHooks}
              onChange={(event) => {
                setConfig((current) => ({
                  ...current,
                  disableAllHooks: event.target.checked,
                }));
                setDirty(true);
              }}
            />
            禁用所有 hooks
          </label>
          <Button
            size="sm"
            variant="secondary"
            icon={RefreshIcon}
            onClick={() => void loadConfig()}
            disabled={loading}
          >
            刷新
          </Button>
        </div>
        {scope === "project" && (
          <div className="p-3 flex items-center gap-2">
            <input
              className={`${inputClass} flex-1`}
              value={projectPath}
              onChange={(event) => setProjectPath(event.target.value)}
              placeholder="项目路径"
            />
            <Button
              size="sm"
              variant={trusted ? "secondary" : "primary"}
              icon={CheckIcon}
              onClick={() => void toggleTrust(!trusted)}
            >
              {trusted ? "已信任" : "信任项目"}
            </Button>
          </div>
        )}
      </Section>

      <div className="mb-3 flex items-end justify-between gap-4">
        <div>
          <div className="text-sm font-semibold text-text-primary">
            {selectedEvent}
          </div>
          <div className="text-xs text-text-tertiary">
            {selectedInfo?.description ?? "选择事件后配置匹配器和命令"}
          </div>
        </div>
        <Button size="sm" variant="secondary" icon={PlusIcon} onClick={addGroup}>
          添加匹配器
        </Button>
      </div>

      <div className="space-y-3">
        {groups.length === 0 && (
          <div className="rounded-lg border border-border-subtle bg-elevated p-4 text-sm text-text-tertiary">
            当前事件还没有 hook。添加匹配器后可为具体工具或所有工具配置命令。
          </div>
        )}
        {groups.map((group, groupIndex) => (
          <div
            key={`${selectedEvent}-${groupIndex}`}
            className="rounded-lg border border-border-subtle bg-elevated overflow-hidden"
          >
            <div className="flex items-center gap-2 border-b border-border-subtle p-3">
              <span className="text-xs text-text-tertiary">matcher</span>
              <input
                className={`${inputClass} flex-1`}
                value={group.matcher ?? ""}
                onChange={(event) =>
                  updateGroup(groupIndex, { matcher: event.target.value })
                }
                placeholder="* 或工具名，例如 run_command"
              />
              <Button
                size="sm"
                variant="ghost"
                icon={PlusIcon}
                onClick={() => addHook(groupIndex)}
              >
                命令
              </Button>
              <Button
                size="sm"
                variant="ghost"
                icon={TrashIcon}
                onClick={() => removeGroup(groupIndex)}
              />
            </div>
            <div className="divide-y divide-border-subtle">
              {group.hooks.map((hook, hookIndex) => (
                <div key={hook.id ?? hookIndex} className="p-3 space-y-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <label className="flex items-center gap-2 text-xs text-text-secondary">
                      <input
                        className="accent-brand"
                        type="checkbox"
                        checked={hook.enabled}
                        onChange={(event) =>
                          updateHook(groupIndex, hookIndex, {
                            enabled: event.target.checked,
                          })
                        }
                      />
                      启用
                    </label>
                    <input
                      className={`${inputClass} w-36`}
                      value={hook.id ?? ""}
                      onChange={(event) =>
                        updateHook(groupIndex, hookIndex, {
                          id: event.target.value || null,
                        })
                      }
                      placeholder="hook id"
                    />
                    <HookTypeSelect
                      value={hook.type}
                      menuKey={`${groupIndex}:${hookIndex}`}
                      openKey={typeMenuKey}
                      onOpenChange={setTypeMenuKey}
                      onChange={(type) =>
                        updateHook(groupIndex, hookIndex, { type })
                      }
                    />
                    <input
                      className={`${inputClass} w-28`}
                      value={hook.shell ?? ""}
                      onChange={(event) =>
                        updateHook(groupIndex, hookIndex, {
                          shell: event.target.value || null,
                        })
                      }
                      placeholder="shell"
                    />
                    <input
                      className={`${inputClass} w-24`}
                      type="number"
                      min={1}
                      value={hook.timeout ?? 10}
                      onChange={(event) =>
                        updateHook(groupIndex, hookIndex, {
                          timeout: Number(event.target.value || 10),
                        })
                      }
                    />
                    <div className="ml-auto flex items-center gap-2">
                      <Button
                        size="sm"
                        variant="secondary"
                        icon={RefreshIcon}
                        onClick={() => void test(group, hook)}
                        disabled={!hook.command.trim()}
                      >
                        测试
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={TrashIcon}
                        onClick={() => removeHook(groupIndex, hookIndex)}
                      />
                    </div>
                  </div>
                  <textarea
                    className={`${textareaClass} w-full`}
                    value={hook.command}
                    onChange={(event) =>
                      updateHook(groupIndex, hookIndex, {
                        command: event.target.value,
                      })
                    }
                    placeholder="命令会收到 hook JSON stdin，可输出 JSON 控制继续或阻断"
                  />
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>

      {(error || testResult) && (
        <div className="mt-3 rounded-lg border border-border-subtle bg-elevated p-3 text-xs">
          {error ? (
            <div className="text-danger">{error}</div>
          ) : testResult ? (
            <pre className="max-h-44 overflow-auto whitespace-pre-wrap text-text-secondary">
              {JSON.stringify(testResult, null, 2)}
            </pre>
          ) : null}
        </div>
      )}

      <div className="mt-4 flex items-center gap-3">
        <Button
          variant="primary"
          size="sm"
          onClick={() => void save()}
          disabled={saving || loading || !dirty}
        >
          {saving ? "保存中" : "保存配置"}
        </Button>
        <span className="text-xs text-text-tertiary">
          {dirty ? "有未保存更改" : "配置已同步"}
        </span>
      </div>
    </div>
  );
}

function HookTypeSelect({
  value,
  menuKey,
  openKey,
  onOpenChange,
  onChange,
}: {
  value: HookCommandConfig["type"];
  menuKey: string;
  openKey: string | null;
  onOpenChange: (key: string | null) => void;
  onChange: (type: HookCommandConfig["type"]) => void;
}) {
  const open = openKey === menuKey;
  return (
    <div className="relative">
      <button
        type="button"
        className="h-8 w-28 rounded-md border border-border-subtle bg-input-bg px-2 text-xs text-text-primary hover:bg-hover focus-ring inline-flex items-center justify-between"
        onClick={() => onOpenChange(open ? null : menuKey)}
      >
        <span>{value}</span>
        <Icon icon={CaretDownIcon} size={13} className="text-text-tertiary" />
      </button>
      {open && (
        <div className="absolute left-0 top-9 z-50 w-28 rounded-md border border-border-subtle bg-elevated shadow-xl py-1">
          {hookTypes.map((type) => {
            const active = type === value;
            return (
              <button
                key={type}
                type="button"
                className={`h-8 w-full px-2 text-left text-xs flex items-center justify-between ${
                  active
                    ? "bg-brand/20 text-text-primary"
                    : "text-text-secondary hover:bg-hover hover:text-text-primary"
                }`}
                onClick={() => {
                  onChange(type);
                  onOpenChange(null);
                }}
              >
                <span>{type}</span>
                {active && <Icon icon={CheckIcon} size={13} />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
