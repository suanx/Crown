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
  TrashIcon,
} from "@/shared/icons/set";
import { Button } from "@/shared/ui/Button";
import { Pill } from "@/shared/ui/Pill";
import { Dialog } from "@/shared/ui/Dialog";
import { cn } from "@/shared/lib/cn";

const BUILTIN_SKILLS = [
  { name: "deep-research", label: "🔬 深度研究", desc: "系统性多维度调研报告，集成信息图可视化" },
  { name: "web-research", label: "🌐 联网搜索", desc: "获取最新资讯、文档、新闻、事实核查" },
  { name: "file-reader", label: "📄 文档读取", desc: "读取 PDF/Word/Excel/PPT 等办公文档" },
  { name: "code-review", label: "👁️ 代码审查", desc: "检查逻辑缺陷、安全漏洞、性能问题" },
  { name: "git-helper", label: "🔀 Git 助手", desc: "Git 工作流、冲突解决、提交信息规范" },
  { name: "db-helper", label: "🗄️ 数据库助手", desc: "SQL 查询编写、优化、表结构设计" },
  { name: "image-analyzer", label: "🖼️ 图片分析", desc: "OCR 文字识别、图片信息提取、格式转换" },
  { name: "api-tester", label: "🔌 API 测试", desc: "HTTP 请求测试、响应检查、Mock 服务" },
  { name: "refactoring", label: "🔨 代码重构", desc: "安全重构代码，消除技术债" },
  { name: "translator", label: "🌐 翻译本地化", desc: "多语种翻译、i18n 文件处理" },
  { name: "terminal-wizard", label: "💻 终端专家", desc: "Shell 脚本编写、批量文件处理" },
];


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

  const [deleting, setDeleting] = useState<string | null>(null);

  async function handleDelete(name: string) {
    setDeleting(name);
    try {
      await agentClient.skillDelete(name);
      await refresh();
    } finally {
      setDeleting(null);
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
              deleting={deleting === s.name}
              onPreview={() => setPreview(s)}
              onDelete={() => void handleDelete(s.name)}
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

        {/* 内置技能市场 */}
        <div className="mt-8 border-t border-border-subtle pt-6">
          <h2 className="text-base font-semibold text-text-primary mb-1">内置技能</h2>
          <p className="text-xs text-text-tertiary mb-4">应用内置的技能包，首次启动自动安装。点击在对话中使用。</p>
          <div className="grid grid-cols-2 gap-2">
            {BUILTIN_SKILLS.filter(s => !skills.find(x => x.name === s.name)).map(s => (
              <div key={s.name} className="rounded-lg border border-border-subtle bg-elevated p-3 hover:border-border-default transition-colors">
                <div className="text-sm font-medium text-text-primary">{s.label}</div>
                <div className="text-xs text-text-tertiary mt-0.5 line-clamp-2">{s.desc}</div>
                <div className="text-[10px] text-text-tertiary mt-1 font-mono">/{s.name}</div>
              </div>
            ))}
          </div>
          {BUILTIN_SKILLS.filter(s => !skills.find(x => x.name === s.name)).length === 0 && (
            <div className="text-xs text-text-tertiary">所有内置技能已安装</div>
          )}
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
  deleting,
  onPreview,
  onDelete,
}: {
  skill: Skill;
  deleting: boolean;
  onPreview: () => void;
  onDelete: () => void;
}) {
  const meta = scopeMeta(skill.scope);
  return (
    <div className="relative rounded-lg border border-border-subtle bg-elevated hover:border-border-default transition-colors">
      <button onClick={onPreview} className="w-full text-left p-4 focus-ring rounded-lg">
        <div className="flex items-start gap-3">
          <div className="h-10 w-10 rounded-lg flex items-center justify-center shrink-0 bg-brand-soft text-brand">
            <Icon icon={CodeIcon} size={18} weight="duotone" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-base font-semibold text-text-primary">{skill.name}</span>
              <Pill tone="neutral" icon={meta.icon}>{meta.label}</Pill>
              <Pill tone="info">{skill.source}</Pill>
            </div>
            <div className="mt-1 text-sm text-text-secondary leading-relaxed line-clamp-2">{skill.description}</div>
            {skill.allowedTools.length > 0 && (
              <div className="mt-2 flex items-center gap-1 flex-wrap">
                {skill.allowedTools.map((t) => (
                  <span key={t} className="text-xs text-text-tertiary px-2 h-5 inline-flex items-center rounded bg-canvas border border-border-subtle font-mono">{t}</span>
                ))}
              </div>
            )}
            <div className="mt-2 text-xs text-text-tertiary font-mono truncate">{skill.path}</div>
            <div className="mt-1 text-xs text-text-tertiary">在对话中输入 /{skill.name} 使用该技能</div>
          </div>
        </div>
      </button>
      <button
        onClick={onDelete}
        disabled={deleting}
        className="absolute top-3 right-3 h-7 w-7 rounded-md flex items-center justify-center text-text-tertiary hover:text-danger hover:bg-danger/10 transition-colors focus-ring"
        title="删除技能"
        aria-label="删除技能"
      >
        {deleting ? <Icon icon={SpinnerIcon} size={12} className="animate-spin" /> : <Icon icon={TrashIcon} size={12} />}
      </button>
    </div>
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
