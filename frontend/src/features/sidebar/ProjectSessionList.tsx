import { useEffect, useMemo, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useSessionStore } from "@/stores/sessionStore";
import { useProjectStore } from "@/stores/projectStore";
import { useRouterStore } from "@/stores/routerStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CaretDownIcon,
  CaretRightIcon,
  EditIcon,
  ProjectsIcon,
  PlusIcon,
  TrashIcon,
} from "@/shared/icons/set";
import { SessionItem } from "./SessionItem";
import { agentClient, type ProjectSummary, type ThreadSummary } from "@/api";
import { cn } from "@/shared/lib/cn";

/**
 * 项目化会话列表 — Codex 同款两层结构.
 *
 *   项目              [+]   ← 分组标题,hover 时右边显示 "+" 按钮
 *   📁 deepseek-agent ▾  3
 *      · Rust HTTP 搭建
 *      · accessToken 验证测试
 *   📁 deepseek-agent-frontend ▸  2
 *
 *   无项目
 *   · Python 数据清洗脚本
 *   · claudecli 命令无法识别
 *
 * 默认: 当前活跃项目展开,其他折叠. 没绑项目的对话归到"无项目"分组,默认展开.
 */
export function ProjectSessionList() {
  const threads = useSessionStore((s) => s.threads);
  const loadThreads = useSessionStore((s) => s.loadThreads);
  const projects = useProjectStore((s) => s.projects);
  const projectsLoading = useProjectStore((s) => s.loading);
  const projectsError = useProjectStore((s) => s.error);
  const loadProjects = useProjectStore((s) => s.loadProjects);
  const pickProjectDirectory = useProjectStore((s) => s.pickProjectDirectory);
  const createProject = useProjectStore((s) => s.createProject);
  const updateProject = useProjectStore((s) => s.updateProject);
  const deleteProject = useProjectStore((s) => s.deleteProject);
  const navigate = useRouterStore((s) => s.navigate);
  const route = useRouterStore((s) => s.current);
  const activeThreadId = route.page === "chat" ? route.threadId : null;

  // 当前活跃项目 = 当前对话所属项目 (或最近 streaming 项目)
  const activeProjectId = useMemo(() => {
    if (activeThreadId) {
      const t = threads.find((x) => x.id === activeThreadId);
      if (t?.projectId) return t.projectId;
    }
    const streaming = threads.find((t) => t.isStreaming && t.projectId);
    return streaming?.projectId ?? projects[0]?.id ?? null;
  }, [threads, activeThreadId, projects]);

  // 折叠状态 — 默认: 活跃项目 / 无项目 展开,其他折叠
  const [expanded, setExpanded] = useState<Record<string, boolean>>(() => {
    const map: Record<string, boolean> = { __unassigned: true };
    return map;
  });

  const grouped = useMemo(() => {
    const byProject = new Map<string, ThreadSummary[]>();
    const orphans: ThreadSummary[] = [];
    for (const t of threads) {
      if (t.projectId) {
        if (!byProject.has(t.projectId)) byProject.set(t.projectId, []);
        byProject.get(t.projectId)!.push(t);
      } else {
        orphans.push(t);
      }
    }
    return { byProject, orphans };
  }, [threads]);

  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [projectMenu, setProjectMenu] = useState<{
    projectId: string;
    x: number;
    y: number;
  } | null>(null);
  const [projectAction, setProjectAction] = useState<{
    type: "rename" | "delete";
    project: ProjectSummary;
    x: number;
    y: number;
  } | null>(null);
  const [projectBusy, setProjectBusy] = useState(false);

  useEffect(() => {
    if (!projectMenu) return;
    const close = () => setProjectMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [projectMenu]);

  useEffect(() => {
    if (!projectAction) return;
    const close = () => setProjectAction(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [projectAction]);

  function toggle(id: string) {
    setExpanded((m) => ({ ...m, [id]: !m[id] }));
  }

  async function handleCreateProject() {
    const path = await pickProjectDirectory();
    if (!path) return;
    const project = await createProject({
      name: projectNameFromPath(path),
      path,
    });
    setExpanded((m) => ({ ...m, [project.id]: true }));
    await loadThreads();
    await loadProjects();
  }

  return (
    <>
    <div className="flex-1 min-h-0 scrollable px-2 pb-3">
      {/* 项目分组标题 — hover 时显示 "+" 按钮 */}
      <SectionHeader
        label="项目"
        action={
          <button
            onClick={() => void handleCreateProject()}
            aria-label="新建项目"
            title="新建项目"
            className="h-6 w-6 flex items-center justify-center rounded text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
          >
            <Icon icon={PlusIcon} size={12} weight="bold" />
          </button>
        }
      />

      {/* 项目列表 */}
      <div className="space-y-0.5">
        {projectsLoading && (
          <div className="px-2 py-1 text-xs text-text-tertiary">正在加载项目...</div>
        )}
        {projectsError && (
          <div className="px-2 py-1 text-xs text-danger">{projectsError}</div>
        )}
        {!projectsLoading && !projectsError && projects.length === 0 && (
          <div className="px-2 py-1 text-xs text-text-tertiary">暂无项目</div>
        )}
        {projects.map((p) => {
          const items = grouped.byProject.get(p.id) ?? [];
          return (
            <ProjectGroup
              key={p.id}
              project={p}
              items={items}
              expanded={expanded[p.id] ?? p.id === activeProjectId}
              activeThreadId={activeThreadId}
              openMenuId={openMenuId}
              onToggle={() => toggle(p.id)}
              onNewThread={async () => {
                const summary = await agentClient.createThread({
                  projectId: p.id,
                  cwd: p.path,
                });
                await loadThreads();
                await loadProjects();
                navigate({ page: "chat", threadId: summary.id });
              }}
              onPickThread={(id) => navigate({ page: "chat", threadId: id })}
              onToggleMenu={(id) =>
                setOpenMenuId(openMenuId === id ? null : id)
              }
              onCloseMenu={() => setOpenMenuId(null)}
              onOpenProjectMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
                setOpenMenuId(null);
                setProjectMenu({
                  projectId: p.id,
                  x: event.clientX,
                  y: event.clientY,
                });
              }}
            />
          );
        })}
      </div>

      {projectMenu && (
        <ProjectContextMenu
          project={projects.find((p) => p.id === projectMenu.projectId) ?? null}
          x={projectMenu.x}
          y={projectMenu.y}
          onClose={() => setProjectMenu(null)}
          onNewThread={async (project) => {
            setProjectMenu(null);
            const summary = await agentClient.createThread({
              projectId: project.id,
              cwd: project.path,
            });
            await loadThreads();
            await loadProjects();
            navigate({ page: "chat", threadId: summary.id });
          }}
          onRename={async (project) => {
            const point = projectMenu;
            setProjectMenu(null);
            setProjectAction({
              type: "rename",
              project,
              x: point?.x ?? 0,
              y: point?.y ?? 0,
            });
          }}
          onDelete={async (project) => {
            const point = projectMenu;
            setProjectMenu(null);
            setProjectAction({
              type: "delete",
              project,
              x: point?.x ?? 0,
              y: point?.y ?? 0,
            });
          }}
        />
      )}

      {/* 无项目 */}
      {grouped.orphans.length > 0 && (
        <>
          <SectionHeader label="无项目" className="mt-4" />
          <div className="space-y-0.5">
            {grouped.orphans.map((t) => (
              <SessionItem
                key={t.id}
                thread={t}
                active={t.id === activeThreadId}
                menuOpen={openMenuId === t.id}
                onClick={() => navigate({ page: "chat", threadId: t.id })}
                onToggleMenu={() =>
                  setOpenMenuId(openMenuId === t.id ? null : t.id)
                }
                onCloseMenu={() => setOpenMenuId(null)}
              />
            ))}
          </div>
        </>
      )}
    </div>
    {projectAction?.type === "rename" && (
      <RenameProjectPanel
        action={projectAction}
        busy={projectBusy}
        onClose={() => setProjectAction(null)}
        onSubmit={async (name) => {
          if (name === projectAction.project.name) {
            setProjectAction(null);
            return;
          }
          setProjectBusy(true);
          try {
            await updateProject({
              projectId: projectAction.project.id,
              name,
            });
            setProjectAction(null);
          } finally {
            setProjectBusy(false);
          }
        }}
      />
    )}
    {projectAction?.type === "delete" && (
      <DeleteProjectPanel
        action={projectAction}
        busy={projectBusy}
        onClose={() => setProjectAction(null)}
        onConfirm={async () => {
          setProjectBusy(true);
          try {
            await deleteProject(projectAction.project.id);
            await loadThreads();
            await loadProjects();
            setProjectAction(null);
          } finally {
            setProjectBusy(false);
          }
        }}
      />
    )}
    </>
  );
}

/**
 * 分组标题行 — 与"无项目"统一外观.
 *   小字 uppercase tag + 右侧可选 action (常驻显示,不依赖 hover).
 */
function SectionHeader({
  label,
  action,
  className,
}: {
  label: string;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "h-7 px-2 flex items-center justify-between",
        className,
      )}
    >
      <span className="text-xs font-medium text-text-tertiary uppercase tracking-wide">
        {label}
      </span>
      {action}
    </div>
  );
}

interface GroupProps {
  project: ProjectSummary;
  items: ThreadSummary[];
  expanded: boolean;
  activeThreadId: string | null;
  openMenuId: string | null;
  onToggle: () => void;
  onNewThread: () => Promise<void>;
  onPickThread: (id: string) => void;
  onToggleMenu: (id: string) => void;
  onCloseMenu: () => void;
  onOpenProjectMenu: (event: React.MouseEvent) => void;
}

function ProjectGroup({
  project,
  items,
  expanded,
  activeThreadId,
  openMenuId,
  onToggle,
  onNewThread,
  onPickThread,
  onToggleMenu,
  onCloseMenu,
  onOpenProjectMenu,
}: GroupProps) {
  return (
    <div>
      <div
        onContextMenuCapture={onOpenProjectMenu}
        onContextMenu={onOpenProjectMenu}
        className="group flex items-center rounded-md pr-2 transition-colors hover:bg-hover"
      >
        <button
          onClick={onToggle}
          onContextMenuCapture={onOpenProjectMenu}
          onContextMenu={onOpenProjectMenu}
          className="min-w-0 flex-1 pl-2 pr-0 h-8 flex items-center gap-1.5 rounded-md transition-colors focus-ring"
        >
          <Icon
            icon={expanded ? CaretDownIcon : CaretRightIcon}
            size={11}
            className="text-text-tertiary shrink-0"
          />
          <Icon
            icon={ProjectsIcon}
            size={13}
            weight="duotone"
            className="text-brand shrink-0"
          />
          <span className="flex-1 min-w-0 text-left text-sm truncate text-text-primary font-medium">
            {project.name}
          </span>
        </button>
        <div className="relative size-6 shrink-0">
          <span className="absolute inset-0 inline-flex items-center justify-center text-xs text-text-tertiary font-mono tabular-nums transition-opacity group-hover:opacity-0">
            {items.length}
          </span>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              void onNewThread();
            }}
            aria-label={`在 ${project.name} 中新建对话`}
            title="新建项目对话"
            className="absolute inset-0 grid place-items-center rounded-md text-text-tertiary opacity-0 transition-opacity hover:bg-elevated hover:text-text-primary group-hover:opacity-100 focus:opacity-100 focus-ring"
          >
            <Icon icon={EditIcon} size={13} />
          </button>
        </div>
      </div>
      {expanded && (
        <div className="mt-0.5 ml-3 pl-2 border-l border-border-subtle space-y-0.5">
          {items.length === 0 ? (
            <div className="px-2 py-1 text-xs text-text-tertiary">暂无对话</div>
          ) : (
            items.map((t) => (
              <SessionItem
                key={t.id}
                thread={t}
                active={t.id === activeThreadId}
                menuOpen={openMenuId === t.id}
                onClick={() => onPickThread(t.id)}
                onToggleMenu={() => onToggleMenu(t.id)}
                onCloseMenu={onCloseMenu}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function ProjectContextMenu({
  project,
  x,
  y,
  onClose,
  onNewThread,
  onRename,
  onDelete,
}: {
  project: ProjectSummary | null;
  x: number;
  y: number;
  onClose: () => void;
  onNewThread: (project: ProjectSummary) => Promise<void>;
  onRename: (project: ProjectSummary) => Promise<void>;
  onDelete: (project: ProjectSummary) => Promise<void>;
}) {
  if (!project) return null;
  return createPortal(
    <>
      <div className="fixed inset-0 z-[80]" onClick={onClose} aria-hidden />
      <div
        className="fixed z-[90] min-w-[168px] py-1 bg-overlay border border-border-default rounded-md animate-scale-in"
        style={{
          left: Math.min(x, window.innerWidth - 184),
          top: Math.min(y, window.innerHeight - 168),
          boxShadow: "var(--ds-shadow-md)",
        }}
      >
        <ProjectMenuItem
          icon={EditIcon}
          label="新建对话"
          onClick={() => void onNewThread(project)}
        />
        <ProjectMenuItem
          icon={EditIcon}
          label="重命名"
          onClick={() => void onRename(project)}
        />
        <div className="my-1 h-px bg-border-subtle" />
        <ProjectMenuItem
          icon={TrashIcon}
          label="移除项目"
          tone="danger"
          onClick={() => void onDelete(project)}
        />
      </div>
    </>,
    document.body,
  );
}

function ProjectMenuItem({
  icon,
  label,
  tone = "default",
  onClick,
}: {
  icon: typeof EditIcon;
  label: string;
  tone?: "default" | "danger";
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "w-full h-8 px-3 flex items-center gap-2 text-sm text-left transition-colors hover:bg-hover",
        tone === "danger"
          ? "text-danger"
          : "text-text-secondary hover:text-text-primary",
      )}
    >
      <Icon icon={icon} size={14} />
      <span>{label}</span>
    </button>
  );
}

type ProjectAction = {
  type: "rename" | "delete";
  project: ProjectSummary;
  x: number;
  y: number;
};

function RenameProjectPanel({
  action,
  busy,
  onClose,
  onSubmit,
}: {
  action: ProjectAction;
  busy: boolean;
  onClose: () => void;
  onSubmit: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setName(action.project.name);
    setError(null);
  }, [action.project.name]);

  async function submit() {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("项目名称不能为空");
      return;
    }
    setError(null);
    try {
      await onSubmit(trimmed);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return createPortal(
    <>
      <div className="fixed inset-0 z-[80]" onClick={onClose} aria-hidden />
      <CompactPanel x={action.x} y={action.y}>
        <div className="px-3 pt-2 pb-2">
          <div className="mb-2 text-xs font-semibold text-text-primary">
            重命名
          </div>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            autoFocus
            disabled={busy}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submit();
            }}
            className="h-8 w-full rounded-md border border-border-focus bg-input-bg px-2 text-sm text-text-primary outline-none"
          />
          {error && (
            <div className="mt-1 text-[11px] leading-4 text-danger break-all">
              {error}
            </div>
          )}
        </div>
        <div className="flex h-9 items-center justify-end gap-1 border-t border-border-subtle px-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="h-7 rounded-md px-2 text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={busy}
            className="h-7 rounded-md bg-brand px-2.5 text-xs font-medium text-white hover:bg-brand-hover disabled:opacity-50"
          >
            保存
          </button>
        </div>
      </CompactPanel>
    </>,
    document.body,
  );
}

function DeleteProjectPanel({
  action,
  busy,
  onClose,
  onConfirm,
}: {
  action: ProjectAction;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}) {
  return createPortal(
    <>
      <div className="fixed inset-0 z-[80]" onClick={onClose} aria-hidden />
      <CompactPanel x={action.x} y={action.y}>
        <div className="px-3 pt-2 pb-2">
          <div className="mb-1 text-xs font-semibold text-text-primary">
            移除项目
          </div>
          <p className="text-xs leading-5 text-text-secondary">
            移除「
            <span className="text-text-primary">{action.project.name}</span>
            」？本地文件不会删除。
          </p>
        </div>
        <div className="flex h-9 items-center justify-end gap-1 border-t border-border-subtle px-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="h-7 rounded-md px-2 text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={() => void onConfirm()}
            disabled={busy}
            className="h-7 rounded-md bg-danger px-2.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
          >
            移除
          </button>
        </div>
      </CompactPanel>
    </>,
    document.body,
  );
}

function CompactPanel({
  x,
  y,
  children,
}: {
  x: number;
  y: number;
  children: ReactNode;
}) {
  return (
    <div
      className="fixed z-[90] w-[164px] rounded-lg border border-border-default bg-overlay animate-scale-in"
      style={{
        left: Math.min(x, window.innerWidth - 180),
        top: Math.min(y, window.innerHeight - 140),
        boxShadow: "var(--ds-shadow-md)",
      }}
    >
      {children}
    </div>
  );
}

function projectNameFromPath(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  const parts = normalized.split(/[\\/]/);
  return parts[parts.length - 1] || normalized || "未命名项目";
}
