import { create } from "zustand";

/**
 * Workspace Dock 状态机
 *
 * 设计原则:
 *   - 三个槽位 (left sidebar / right panel / bottom panel) 用同一个 store
 *   - 宽度持久化到 localStorage,启动时 clamp 到合法区间
 *   - 写入 CSS 变量驱动 AppShell 的 grid template,组件不直接读宽度
 *   - 任何尺寸变更都通过 setter,不允许组件直接 setProperty
 *
 * 持久化 key:
 *   ds.workspace.v1 = JSON({ sidebarW, rightW, bottomH, leftSidebar, right, bottom })
 */

// ── 来自 tokens.css 的边界常量 (与 CSS 保持同步) ─────────────────────────
const BOUNDS = {
  sidebar: { min: 220, default: 220, max: 420 },
  right: { min: 260, default: 260, max: 720 },
  bottom: { min: 120, default: 260, maxVh: 0.7 }, // bottom 用 70vh 上限,运行时算
} as const;

export type PanelKind = "files" | "review" | "terminal" | "browser" | "tasks";

export type LeftSidebarMode = "expanded" | "hidden";

interface PersistedState {
  sidebarW: number;
  rightW: number;
  bottomH: number;
  leftSidebar: LeftSidebarMode;
  rightContent: PanelKind | null;
  bottomContent: PanelKind | null;
}

interface WorkspaceState extends PersistedState {
  // —— Sidebar (左) ——
  toggleLeftSidebar: () => void;
  setSidebarWidth: (px: number) => void;

  // —— Right panel ——
  toggleRightPanel: () => void;
  openInRight: (kind: PanelKind) => void;
  closeRight: () => void;
  setRightWidth: (px: number) => void;

  // —— Bottom panel ——
  toggleBottomPanel: () => void;
  openInBottom: (kind: PanelKind) => void;
  closeBottom: () => void;
  setBottomHeight: (px: number) => void;

  // —— 跨槽位:把内容移到另一槽位 ——
  swapToRight: (kind: PanelKind) => void;
  swapToBottom: (kind: PanelKind) => void;
}

// ── 持久化 ────────────────────────────────────────────────────────────────
const STORAGE_KEY = "ds.workspace.v1";

function loadPersisted(): PersistedState {
  const fallback: PersistedState = {
    sidebarW: BOUNDS.sidebar.default,
    rightW: BOUNDS.right.default,
    bottomH: BOUNDS.bottom.default,
    leftSidebar: "expanded",
    rightContent: null,
    bottomContent: null,
  };
  if (typeof window === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<PersistedState>;
    return {
      sidebarW: clamp(
        parsed.sidebarW ?? fallback.sidebarW,
        BOUNDS.sidebar.min,
        BOUNDS.sidebar.max,
      ),
      rightW: clamp(
        parsed.rightW ?? fallback.rightW,
        BOUNDS.right.min,
        BOUNDS.right.max,
      ),
      bottomH: clamp(
        parsed.bottomH ?? fallback.bottomH,
        BOUNDS.bottom.min,
        bottomMaxPx(),
      ),
      leftSidebar: parsed.leftSidebar ?? fallback.leftSidebar,
      rightContent: parsed.rightContent ?? null,
      bottomContent: parsed.bottomContent ?? null,
    };
  } catch {
    return fallback;
  }
}

function persist(state: PersistedState) {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* quota / private mode — 忽略 */
  }
}

function clamp(v: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(hi, v));
}

function bottomMaxPx(): number {
  if (typeof window === "undefined") return 600;
  return Math.round(window.innerHeight * BOUNDS.bottom.maxVh);
}

// ── CSS 变量同步 ─────────────────────────────────────────────────────────
function applyToCss(s: PersistedState) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.style.setProperty(
    "--ds-sidebar-w",
    s.leftSidebar === "hidden" ? "0px" : `${s.sidebarW}px`,
  );
  root.style.setProperty(
    "--ds-right-w",
    s.rightContent ? `${s.rightW}px` : "0px",
  );
  root.style.setProperty(
    "--ds-bottom-h",
    s.bottomContent ? `${s.bottomH}px` : "0px",
  );
}

// ── Store ────────────────────────────────────────────────────────────────
const initial = loadPersisted();
applyToCss(initial);

export const useWorkspaceStore = create<WorkspaceState>((set, get) => {
  const update = (patch: Partial<PersistedState>) => {
    const next: PersistedState = {
      sidebarW: get().sidebarW,
      rightW: get().rightW,
      bottomH: get().bottomH,
      leftSidebar: get().leftSidebar,
      rightContent: get().rightContent,
      bottomContent: get().bottomContent,
      ...patch,
    };
    set(patch);
    applyToCss(next);
    persist(next);
  };

  return {
    ...initial,

    toggleLeftSidebar: () =>
      update({
        leftSidebar: get().leftSidebar === "expanded" ? "hidden" : "expanded",
      }),

    setSidebarWidth: (px) =>
      update({
        sidebarW: clamp(px, BOUNDS.sidebar.min, BOUNDS.sidebar.max),
      }),

    toggleRightPanel: () => {
      const cur = get().rightContent;
      update({ rightContent: cur ? null : "files" });
    },

    openInRight: (kind) => {
      const { bottomContent } = get();
      // 如果同一 kind 在 bottom,从 bottom 移走
      update({
        rightContent: kind,
        bottomContent: bottomContent === kind ? null : bottomContent,
      });
    },

    closeRight: () => update({ rightContent: null }),

    setRightWidth: (px) =>
      update({ rightW: clamp(px, BOUNDS.right.min, BOUNDS.right.max) }),

    toggleBottomPanel: () => {
      const cur = get().bottomContent;
      update({ bottomContent: cur ? null : "terminal" });
    },

    openInBottom: (kind) => {
      const { rightContent } = get();
      update({
        bottomContent: kind,
        rightContent: rightContent === kind ? null : rightContent,
      });
    },

    closeBottom: () => update({ bottomContent: null }),

    setBottomHeight: (px) =>
      update({
        bottomH: clamp(px, BOUNDS.bottom.min, bottomMaxPx()),
      }),

    swapToRight: (kind) => {
      update({
        rightContent: kind,
        bottomContent: get().bottomContent === kind ? null : get().bottomContent,
      });
    },

    swapToBottom: (kind) => {
      update({
        bottomContent: kind,
        rightContent: get().rightContent === kind ? null : get().rightContent,
      });
    },
  };
});

// 监听窗口高度变化,重 clamp bottom 高度
if (typeof window !== "undefined") {
  window.addEventListener("resize", () => {
    const s = useWorkspaceStore.getState();
    const max = bottomMaxPx();
    if (s.bottomH > max) {
      s.setBottomHeight(max);
    }
  });
}
