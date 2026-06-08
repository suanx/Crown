import { useCallback, useEffect, useMemo, useState } from "react";
import { agentClient, type Skill, type SkillScope } from "@/api";
import { useActiveThread } from "@/stores/chatStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CodeIcon,
  GlobeIcon,
  FolderIcon,
  RefreshIcon,
  CloseIcon,
  SpinnerIcon,
  SkillIcon,
} from "@/shared/icons/set";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { Dialog } from "@/shared/ui/Dialog";
import { cn } from "@/shared/lib/cn";

/**
 * 技能(Skill)页 — 连真后端 (deepseek-skill).
 *
 * Skill = 一个带 SKILL.md 的目录 (渐进式披露:列表只显示 name+description,
 * 正文按需经 `skillRead` 加载)。来源四目录:全局/项目 × native/claude。
 *
 * - 列表: `skillList(threadId?)` — 传当前 thread 以纳入其项目作用域 skill
 * - 预览: `skillRead(name, threadId)` — 拿到模型调 skill 工具时看到的正文
 * - 刷新: `skillReload(threadId)` — 重新扫描目录
 *
 * 安装走 YOLO:用户在对话里说「装一个 xxx skill」,Agent 用现有工具
 * (git clone / write_file) 自己写到 skills 目录,这里刷新即可见。
 */

type SkillFilter = "all" | SkillScope;

export function SkillsPage() {
  const thread = useActiveThread();
  const threadId = thread?.id;

  const [skills, setSkills] = useState<Skill[]>([]);
  const [loading, setLoading] = useState(true);
  const [reloading, setReloading] = useState(false);
  const [filter, setFilter] = useState<SkillFilter>("all");
  const [preview, setPreview] = useState<Skill | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSkills(await agentClient.skillList(threadId));
    } finally {
      setLoading(false);
    }
  }, [threadId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function reload() {
    setReloading(true);
    try {
      await agentClient.skillReload(threadId);
      await refresh();
    } finally {
      setReloading(false);
    }
  }

  const filtered = useMemo(
    () => skills.filter((s) => filter === "all" || s.scope === filter),
    [skills, filter],
  );

  const globalCount = skills.filter((s) => s.scope === "global").length;
  const projectCount = skills.filter((s) => s.scope === "project").length;

  return (
    <div className="h-full scrollable">
      <div className="max-w-[760px] mx-auto px-8 py-8">
        {/* 头部 */}
        <div className="flex items-start justify-between mb-6">
          <div>
            <h1 className="text-xl font-semibold text-text-primary tracking-tight">
              技能
            </h1>
            <p className="mt-1 text-sm text-text-secondary">
              带 SKILL.md 的可复用指令包。启动时只注入名称+描述,匹配到时由模型按需加载正文。
            </p>
          </div>
          <Button
            variant="ghost"
            icon={reloading ? SpinnerIcon : RefreshIcon}
            disabled={reloading}
            onClick={() => void reload()}
          >
            重新扫描
          </Button>
        </div>

        {/* 过滤栏 */}
        <div className="mb-4 flex items-center gap-1">
          <FilterTab
            label="全部"
            active={filter === "all"}
            onClick={() => setFilter("all")}
            count={skills.length}
          />
          <FilterTab
            label="全局"
            active={filter === "global"}
            onClick={() => setFilter("global")}
            count={globalCount}
          />
          <FilterTab
            label="项目"
            active={filter === "project"}
            onClick={() => setFilter("project")}
            count={projectCount}
          />
        </div>

        {/* 列表 */}
        <div className="space-y-2">
          {filtered.map((s) => (
            <SkillCard
              key={`${s.scope}:${s.name}`}
              skill={s}
              onPreview={() => setPreview(s)}
            />
          ))}
        </div>

        {!loading && skills.length === 0 && (
          <div className="rounded-lg border border-dashed border-border-default p-8 text-center">
            <Icon
              icon={SkillIcon}
              size={28}
              weight="duotone"
              className="text-text-tertiary mx-auto mb-3"
            />
            <div className="text-sm text-text-secondary">尚未发现任何技能</div>
            <div className="mt-1 text-xs text-text-tertiary">
              把 SKILL.md 放进 skills 目录,或在对话里说「帮我装一个 xxx skill」让 Agent 自己装。
            </div>
          </div>
        )}

        {/* 提示 */}
        <div className="mt-6 text-xs text-text-tertiary leading-relaxed">
          技能遵循官方 Agent Skills 规格。全局技能跨项目可用,项目技能随当前对话的工作目录解析。
        </div>
      </div>

      <PreviewDialog
        skill={preview}
        threadId={threadId}
        onClose={() => setPreview(null)}
      />
    </div>
  );
}

function scopeMeta(scope: SkillScope) {
  return scope === "global"
    ? { icon: GlobeIcon, label: "全局" }
    : { icon: FolderIcon, label: "项目" };
}

function SkillCard({
  skill,
  onPreview,
}: {
  skill: Skill;
  onPreview: () => void;
}) {
  const meta = scopeMeta(skill.scope);
  return (
    <button
      onClick={onPreview}
      className="w-full text-left rounded-lg border border-border-subtle bg-elevated p-4 hover:border-border-default transition-colors focus-ring"
    >
      <div className="flex items-start gap-3">
        <div className="h-10 w-10 rounded-lg flex items-center justify-center shrink-0 bg-brand-soft text-brand">
          <Icon icon={CodeIcon} size={18} weight="duotone" />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-base font-semibold text-text-primary">
              {skill.name}
            </span>
            <Pill tone="neutral" icon={meta.icon}>
              {meta.label}
            </Pill>
            {skill.source === "claude" && (
              <span className="text-xs text-text-tertiary">claude 兼容</span>
            )}
          </div>
          <div className="mt-1 text-sm text-text-secondary leading-relaxed line-clamp-2">
            {skill.description}
          </div>
          {skill.allowedTools.length > 0 && (
            <div className="mt-2 flex items-center gap-1 flex-wrap">
              {skill.allowedTools.map((t) => (
                <span
                  key={t}
                  className="text-xs text-text-tertiary px-2 h-5 inline-flex items-center rounded bg-canvas border border-border-subtle font-mono"
                >
                  {t}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </button>
  );
}

/** 技能正文预览 — 展示模型调 skill 工具时看到的 SKILL.md 正文。 */
function PreviewDialog({
  skill,
  threadId,
  onClose,
}: {
  skill: Skill | null;
  threadId?: string;
  onClose: () => void;
}) {
  const [body, setBody] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!skill) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    setBody("");
    agentClient
      .skillRead(skill.name, threadId)
      .then((b) => {
        if (!cancelled) setBody(b);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [skill, threadId]);

  return (
    <Dialog open={!!skill} onClose={onClose} className="max-w-2xl">
      <div className="flex items-center justify-between px-5 h-12 border-b border-border-subtle">
        <span className="text-sm font-semibold text-text-primary">
          {skill?.name}
        </span>
        <button
          onClick={onClose}
          className="text-text-tertiary hover:text-text-primary focus-ring rounded"
          aria-label="关闭"
        >
          <Icon icon={CloseIcon} size={16} />
        </button>
      </div>
      <div className="p-5 max-h-[60vh] overflow-auto">
        {skill && (
          <div className="text-xs font-mono text-text-tertiary mb-3 break-all">
            {skill.path}
          </div>
        )}
        {loading && (
          <div className="flex items-center gap-2 text-sm text-text-secondary">
            <Icon icon={SpinnerIcon} size={14} className="animate-spin" />
            加载正文…
          </div>
        )}
        {error && <div className="text-sm text-danger break-all">{error}</div>}
        {!loading && !error && (
          <pre className="text-sm text-text-primary whitespace-pre-wrap leading-relaxed font-mono">
            {body}
          </pre>
        )}
      </div>
    </Dialog>
  );
}

function FilterTab({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "h-7 px-3 inline-flex items-center gap-1.5 rounded-md text-sm transition-colors focus-ring",
        active
          ? "bg-hover text-text-primary font-medium"
          : "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
    >
      <span>{label}</span>
      <span className="text-xs text-text-tertiary font-mono">{count}</span>
    </button>
  );
}
