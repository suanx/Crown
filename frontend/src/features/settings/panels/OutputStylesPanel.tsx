import { useCallback, useEffect, useState } from "react";
import { agentClient, type OutputStyle } from "@/api";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { Dialog } from "@/shared/ui/Dialog";
import { Input } from "@/shared/ui/Input";
import { Icon } from "@/shared/icons/Icon";
import {
  PlusIcon,
  CheckCircleIcon,
  EditIcon,
  TrashIcon,
  CloseIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import { PanelTitle } from "./_shared";

/**
 * 输出风格面板 (Phase 2) — 连真后端 (output_styles 命令).
 *
 * 输出风格是用户可编辑的 Markdown 片段，存于 `<data_root>/output-styles/<name>.md`。
 * 当前生效的风格会被追加到每个对话的系统提示。这里可以：
 *   - 列出所有风格 + 当前生效标记
 *   - 选中一个编辑正文 → 保存
 *   - 新建（弹自有 Dialog，不用浏览器原生 prompt）
 *   - 删除（带确认）
 *   - 设为当前 / 取消生效 → 立即对后续回合生效
 */
export function OutputStylesPanel() {
  const [styles, setStyles] = useState<OutputStyle[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [body, setBody] = useState("");
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [newOpen, setNewOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const list = await agentClient.listOutputStyles();
    setStyles(list);
    return list;
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const openStyle = useCallback(async (name: string) => {
    setSelected(name);
    setBusy(true);
    try {
      const content = await agentClient.readOutputStyle(name);
      setBody(content);
      setDirty(false);
    } catch {
      setBody("");
      setDirty(false);
    } finally {
      setBusy(false);
    }
  }, []);

  async function handleCreate(name: string, initialBody: string) {
    await agentClient.saveOutputStyle(name, initialBody);
    setNewOpen(false);
    await refresh();
    await openStyle(name);
  }

  async function handleSave() {
    if (!selected) return;
    setBusy(true);
    try {
      await agentClient.saveOutputStyle(selected, body);
      setDirty(false);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleActivate(name: string, makeActive: boolean) {
    setBusy(true);
    try {
      await agentClient.setActiveOutputStyle(makeActive ? name : null);
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(name: string) {
    setBusy(true);
    try {
      await agentClient.deleteOutputStyle(name);
      setDeleteTarget(null);
      if (selected === name) {
        setSelected(null);
        setBody("");
        setDirty(false);
      }
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  const activeName = styles.find((s) => s.active)?.name ?? null;
  const selectedIsActive = selected != null && selected === activeName;

  return (
    <div>
      <PanelTitle
        title="输出风格"
        description="自定义 Agent 的回答风格。当前生效的风格会注入每个对话的系统提示，编辑后立即对后续回合生效。"
      />

      <div className="flex items-center justify-between mb-3">
        <span className="text-sm text-text-secondary">
          {activeName ? (
            <>
              当前生效：
              <span className="text-text-primary font-medium">{activeName}</span>
            </>
          ) : (
            "当前未启用任何输出风格（使用默认）"
          )}
        </span>
        <Button
          variant="primary"
          icon={PlusIcon}
          onClick={() => setNewOpen(true)}
          data-testid="output-style-new"
        >
          新建风格
        </Button>
      </div>

      <div className="flex gap-4" style={{ minHeight: 380 }}>
        {/* 左侧列表 */}
        <div className="w-52 shrink-0 rounded-lg border border-border-subtle bg-elevated overflow-hidden flex flex-col">
          {styles.length === 0 && (
            <div className="p-4 text-xs text-text-tertiary text-center">
              还没有输出风格，点「新建风格」创建一个。
            </div>
          )}
          {styles.map((s) => (
            <div
              key={s.name}
              data-testid="output-style-item"
              className={cn(
                "group flex items-center gap-2 px-3 h-10 text-sm transition-colors border-b border-border-subtle last:border-b-0 cursor-pointer",
                selected === s.name
                  ? "bg-hover text-text-primary font-medium"
                  : "text-text-secondary hover:bg-hover hover:text-text-primary",
              )}
              onClick={() => void openStyle(s.name)}
            >
              <Icon icon={EditIcon} size={13} className="shrink-0 opacity-70" />
              <span className="truncate flex-1">{s.name}</span>
              {s.active && (
                <Icon
                  icon={CheckCircleIcon}
                  size={13}
                  weight="fill"
                  className="text-success shrink-0"
                />
              )}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setDeleteTarget(s.name);
                }}
                aria-label={`删除 ${s.name}`}
                data-testid="output-style-delete"
                className="shrink-0 opacity-0 group-hover:opacity-100 text-text-tertiary hover:text-danger transition-opacity focus-ring rounded p-0.5"
              >
                <Icon icon={TrashIcon} size={13} />
              </button>
            </div>
          ))}
        </div>

        {/* 右侧编辑器 */}
        <div className="flex-1 min-w-0 flex flex-col">
          {selected ? (
            <>
              <div className="flex items-center justify-between mb-2">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-text-primary">
                    {selected}
                  </span>
                  {selectedIsActive && <Pill tone="success">生效中</Pill>}
                </div>
                <div className="flex items-center gap-2">
                  {selectedIsActive ? (
                    <Button
                      variant="ghost"
                      onClick={() => void handleActivate(selected, false)}
                      disabled={busy}
                      data-testid="output-style-activate"
                    >
                      取消生效
                    </Button>
                  ) : (
                    <Button
                      variant="secondary"
                      onClick={() => void handleActivate(selected, true)}
                      disabled={busy}
                      data-testid="output-style-activate"
                    >
                      设为当前
                    </Button>
                  )}
                  <Button
                    variant="primary"
                    onClick={() => void handleSave()}
                    disabled={busy || !dirty}
                    data-testid="output-style-save"
                  >
                    {dirty ? "保存" : "已保存"}
                  </Button>
                </div>
              </div>
              <textarea
                data-testid="output-style-editor"
                value={body}
                onChange={(e) => {
                  setBody(e.target.value);
                  setDirty(true);
                }}
                spellCheck={false}
                placeholder="写下这个风格的指令，例如：用要点列表回答，每点不超过一句话。"
                className={cn(
                  "flex-1 w-full px-3 py-2 rounded-md text-sm font-mono bg-input-bg text-text-primary",
                  "border border-border-default placeholder:text-text-tertiary",
                  "outline-none focus:border-border-focus transition-colors resize-none scrollable",
                )}
              />
            </>
          ) : (
            <div className="flex-1 rounded-lg border border-dashed border-border-default flex items-center justify-center text-sm text-text-tertiary">
              从左侧选择一个风格编辑，或新建一个。
            </div>
          )}
        </div>
      </div>

      <NewStyleDialog
        open={newOpen}
        existing={styles.map((s) => s.name)}
        onClose={() => setNewOpen(false)}
        onCreate={handleCreate}
      />

      <DeleteConfirmDialog
        name={deleteTarget}
        busy={busy}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => deleteTarget && void handleDelete(deleteTarget)}
      />
    </div>
  );
}

/** 新建风格对话框 — 自有 Dialog，不用浏览器原生 prompt。 */
function NewStyleDialog({
  open,
  existing,
  onClose,
  onCreate,
}: {
  open: boolean;
  existing: string[];
  onClose: () => void;
  onCreate: (name: string, body: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (open) {
      setName("");
      setBody("");
      setError(null);
    }
  }, [open]);

  const NAME_RE = /^[A-Za-z0-9_-]+$/;

  async function submit() {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("请填写风格名称");
      return;
    }
    if (!NAME_RE.test(trimmed)) {
      setError("名称只能包含字母、数字、连字符、下划线");
      return;
    }
    if (existing.includes(trimmed)) {
      setError("已存在同名风格");
      return;
    }
    setSubmitting(true);
    try {
      await onCreate(trimmed, body);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onClose={onClose} className="max-w-lg">
      <div className="flex items-center justify-between px-5 h-12 border-b border-border-subtle">
        <span className="text-sm font-semibold text-text-primary">
          新建输出风格
        </span>
        <button
          onClick={onClose}
          className="text-text-tertiary hover:text-text-primary focus-ring rounded"
          aria-label="关闭"
        >
          <Icon icon={CloseIcon} size={16} />
        </button>
      </div>
      <div className="p-5 space-y-4">
        <div>
          <label className="text-xs text-text-secondary mb-1.5 block">
            名称（字母、数字、连字符、下划线）
          </label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如 concise"
            data-testid="output-style-new-name"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>
        <div>
          <label className="text-xs text-text-secondary mb-1.5 block">
            风格指令（可留空，稍后编辑）
          </label>
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            rows={4}
            spellCheck={false}
            placeholder="例如：用简洁的要点回答，不要寒暄。"
            data-testid="output-style-new-body"
            className={cn(
              "w-full px-3 py-2 rounded-md text-sm bg-input-bg text-text-primary",
              "border border-border-default placeholder:text-text-tertiary",
              "outline-none focus:border-border-focus transition-colors resize-y",
            )}
          />
        </div>
        {error && <div className="text-xs text-danger break-all">{error}</div>}
      </div>
      <div className="flex items-center justify-end gap-2 px-5 h-14 border-t border-border-subtle">
        <Button variant="ghost" onClick={onClose} disabled={submitting}>
          取消
        </Button>
        <Button
          variant="primary"
          icon={PlusIcon}
          onClick={() => void submit()}
          disabled={submitting}
          data-testid="output-style-new-confirm"
        >
          创建
        </Button>
      </div>
    </Dialog>
  );
}

/** 删除确认对话框. */
function DeleteConfirmDialog({
  name,
  busy,
  onCancel,
  onConfirm,
}: {
  name: string | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={name != null} onClose={onCancel} className="max-w-sm">
      <div className="p-5">
        <div className="text-sm font-semibold text-text-primary mb-2">
          删除输出风格
        </div>
        <p className="text-sm text-text-secondary">
          确定删除「<span className="text-text-primary font-medium">{name}</span>
          」？此操作不可撤销。若它正在生效，将同时取消生效。
        </p>
      </div>
      <div className="flex items-center justify-end gap-2 px-5 h-14 border-t border-border-subtle">
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          取消
        </Button>
        <Button
          variant="danger"
          icon={TrashIcon}
          onClick={onConfirm}
          disabled={busy}
          data-testid="output-style-delete-confirm"
        >
          删除
        </Button>
      </div>
    </Dialog>
  );
}
