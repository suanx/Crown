import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { agentClient, type PtySession } from "@/api";
import { useActiveThread } from "@/stores/chatStore";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CloseIcon,
  SwapToRightIcon,
  TerminalIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";

interface TerminalTab {
  id: string;
  title: string;
  cwd: string | null;
  ptyId: string | null;
}

interface TerminalRuntime {
  id: string;
  terminal: Terminal;
  fit: FitAddon;
  host: HTMLDivElement;
  ptyId: string | null;
  cwd: string | null;
}

const runtimes = new Map<string, TerminalRuntime>();
let tabCounter = 1;

export interface TerminalViewProps {
  slot: "right" | "bottom";
}

export function TerminalView({ slot }: TerminalViewProps) {
  const activeThread = useActiveThread();
  const cwd = activeThread?.projectId && activeThread.cwd ? activeThread.cwd : null;
  const closePanel = useWorkspaceStore((s) =>
    slot === "right" ? s.closeRight : s.closeBottom,
  );
  const swapPanel = useWorkspaceStore((s) =>
    slot === "right" ? s.swapToBottom : s.swapToRight,
  );
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);

  const activeRuntime = useMemo(
    () => (activeId ? runtimes.get(activeId) ?? null : null),
    [activeId],
  );

  const fitActive = useCallback(() => {
    const runtime = activeId ? runtimes.get(activeId) : null;
    if (!runtime) return;
    try {
      runtime.fit.fit();
      if (runtime.ptyId) {
        void agentClient.ptyResize(
          runtime.ptyId,
          runtime.terminal.cols,
          runtime.terminal.rows,
        );
      }
    } catch (err) {
      if (import.meta.env.DEV) {
        console.warn("[terminal] resize failed:", err);
      }
    }
  }, [activeId]);

  const attachHosts = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    for (const runtime of runtimes.values()) {
      if (runtime.host.parentElement !== container) {
        container.appendChild(runtime.host);
      }
      runtime.host.style.display = runtime.id === activeId ? "block" : "none";
    }
    requestAnimationFrame(fitActive);
  }, [activeId, fitActive]);

  const refreshTabsFromRuntimes = useCallback(() => {
    setTabs(
      Array.from(runtimes.values()).map((runtime, index) => ({
        id: runtime.id,
        title: `终端 ${index + 1}`,
        cwd: runtime.cwd,
        ptyId: runtime.ptyId,
      })),
    );
  }, []);

  const createTerminal = useCallback(
    async (initial?: PtySession) => {
      const id = initial?.ptyId ?? `local-${Date.now()}-${tabCounter++}`;
      if (runtimes.has(id)) {
        setActiveId(id);
        return id;
      }

      const terminal = new Terminal({
        cursorBlink: true,
        convertEol: true,
        fontFamily:
          '"JetBrains Mono", "SF Mono", "Cascadia Code", Consolas, monospace',
        fontSize: 13,
        lineHeight: 1.35,
        scrollback: 8000,
        allowProposedApi: true,
        theme: terminalTheme(),
      });
      const fit = new FitAddon();
      terminal.loadAddon(fit);
      try {
        terminal.loadAddon(new WebglAddon());
      } catch (err) {
        if (import.meta.env.DEV) {
          console.warn("[terminal] webgl addon unavailable:", err);
        }
      }

      const host = document.createElement("div");
      host.className = "terminal-session h-full w-full";
      terminal.open(host);

      const runtime: TerminalRuntime = {
        id,
        terminal,
        fit,
        host,
        ptyId: initial?.ptyId ?? null,
        cwd: initial?.cwd ?? cwd,
      };
      runtimes.set(id, runtime);

      terminal.onData((data) => {
        if (runtime.ptyId) void agentClient.ptyWrite(runtime.ptyId, data);
      });

      setActiveId(id);
      refreshTabsFromRuntimes();
      requestAnimationFrame(() => {
        attachHosts();
        try {
          fit.fit();
        } catch {
          // xterm 在未挂载时可能无法计算尺寸，下一次 attach 会重试。
        }
      });

      if (initial?.ptyId) {
        try {
          const snapshot = await agentClient.ptySnapshot(initial.ptyId);
          runtime.terminal.reset();
          runtime.terminal.write(snapshot.output);
        } catch (err) {
          if (import.meta.env.DEV) {
            console.warn("[terminal] snapshot failed:", err);
          }
        }
        return id;
      }

      try {
        const ptyId = await agentClient.ptySpawn({
          cwd,
          cols: Math.max(1, terminal.cols),
          rows: Math.max(1, terminal.rows),
        });
        runtime.ptyId = ptyId;
        refreshTabsFromRuntimes();
        terminal.focus();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        terminal.writeln(`启动终端失败: ${message}`);
      }
      return id;
    },
    [attachHosts, cwd, refreshTabsFromRuntimes],
  );

  const closeTab = useCallback(
    (id: string) => {
      const runtime = runtimes.get(id);
      if (!runtime) return;
      const orderedIds = Array.from(runtimes.keys());
      const index = orderedIds.indexOf(id);
      runtimes.delete(id);
      runtime.host.remove();
      if (runtime.ptyId) void agentClient.ptyKill(runtime.ptyId);
      runtime.terminal.dispose();
      setTabs((prev) => prev.filter((tab) => tab.id !== id));
      setActiveId((current) => {
        if (current !== id) return current;
        const nextIds = orderedIds.filter((nextId) => nextId !== id);
        return nextIds[Math.min(index, nextIds.length - 1)] ?? null;
      });
    },
    [],
  );

  useEffect(() => {
    const unsubData = agentClient.onPtyData((event) => {
      for (const runtime of runtimes.values()) {
        if (runtime.ptyId === event.ptyId) {
          runtime.terminal.write(event.data);
          break;
        }
      }
    });
    const unsubExit = agentClient.onPtyExit((event) => {
      for (const runtime of runtimes.values()) {
        if (runtime.ptyId === event.ptyId) {
          runtime.terminal.writeln("");
          runtime.terminal.writeln("[进程已退出]");
          runtime.ptyId = null;
          refreshTabsFromRuntimes();
          break;
        }
      }
    });
    return () => {
      unsubData();
      unsubExit();
    };
  }, [refreshTabsFromRuntimes]);

  useEffect(() => {
    let cancelled = false;
    const boot = async () => {
      if (runtimes.size > 0) {
        refreshTabsFromRuntimes();
        setActiveId((current) => current ?? runtimes.keys().next().value ?? null);
        return;
      }
      try {
        const sessions = await agentClient.ptyList();
        if (cancelled) return;
        if (sessions.length > 0) {
          for (const session of sessions) {
            await createTerminal(session);
          }
          return;
        }
      } catch (err) {
        if (import.meta.env.DEV) {
          console.warn("[terminal] list failed:", err);
        }
      }
      if (!cancelled) void createTerminal();
    };
    void boot();
    return () => {
      cancelled = true;
    };
  }, [createTerminal, refreshTabsFromRuntimes]);

  useEffect(() => {
    attachHosts();
  }, [attachHosts, tabs]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => fitActive());
    observer.observe(container);
    return () => observer.disconnect();
  }, [fitActive]);

  useEffect(() => {
    activeRuntime?.terminal.focus();
  }, [activeRuntime]);

  return (
    <div className="h-full min-h-0 flex flex-col">
      <div className="h-9 px-3 flex items-center gap-2 border-b border-border-subtle shrink-0">
        <Icon
          icon={TerminalIcon}
          size={14}
          weight="duotone"
          className="text-text-secondary shrink-0"
        />
        <span className="text-sm font-medium text-text-primary shrink-0">
          终端
        </span>
        <div className="min-w-0 flex-1 flex items-center gap-1 overflow-x-auto">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={cn(
                "group h-7 min-w-0 max-w-40 inline-flex items-center rounded-md text-xs transition-colors shrink-0",
                activeId === tab.id
                  ? "bg-elevated text-text-primary"
                  : "text-text-tertiary hover:bg-hover hover:text-text-secondary",
              )}
            >
              <button
                type="button"
                onClick={() => setActiveId(tab.id)}
                title={tab.cwd ?? tab.title}
                className="min-w-0 flex-1 h-full inline-flex items-center gap-1.5 rounded-l-md pl-2 focus-ring"
              >
                <Icon icon={TerminalIcon} size={12} />
                <span className="truncate">{tab.title}</span>
              </button>
              <button
                type="button"
                onClick={() => closeTab(tab.id)}
                className="grid size-5 shrink-0 place-items-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary focus-ring"
                aria-label={`关闭${tab.title}`}
              >
                <Icon icon={CloseIcon} size={11} />
              </button>
            </div>
          ))}
        </div>
        <div className="shrink-0 flex items-center gap-1">
          <button
            type="button"
            title="新建终端"
            onClick={() => void createTerminal()}
            className="h-7 px-2 inline-flex items-center gap-1 rounded-md text-xs text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
          >
            <Icon icon={TerminalIcon} size={12} />
            新会话
          </button>
          <button
            type="button"
            onClick={() => swapPanel("terminal")}
            title={slot === "right" ? "移到底部" : "移到右侧"}
            aria-label="切换槽位"
            className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
          >
            <Icon icon={SwapToRightIcon} size={13} />
          </button>
          <button
            type="button"
            onClick={closePanel}
            title="关闭"
            aria-label="关闭面板"
            className="h-7 w-7 flex items-center justify-center rounded-md text-text-tertiary hover:bg-hover hover:text-text-primary transition-colors focus-ring"
          >
            <Icon icon={CloseIcon} size={14} />
          </button>
        </div>
      </div>
      <div
        ref={containerRef}
        className="terminal-host flex-1 min-h-0 overflow-hidden px-2 py-2"
        style={{ backgroundColor: "var(--ds-code-bg)" }}
      />
    </div>
  );
}

function terminalTheme(): NonNullable<ConstructorParameters<typeof Terminal>[0]>["theme"] {
  return {
    background: cssVar("--ds-code-bg", "#171717"),
    foreground: cssVar("--ds-code-text", "#e5e5e5"),
    cursor: cssVar("--ds-text-primary", "#ffffff"),
    selectionBackground: "rgba(96, 165, 250, 0.35)",
    black: "#1f2937",
    red: "#ef4444",
    green: "#22c55e",
    yellow: "#eab308",
    blue: "#3b82f6",
    magenta: "#a855f7",
    cyan: "#06b6d4",
    white: "#e5e7eb",
    brightBlack: "#6b7280",
    brightRed: "#f87171",
    brightGreen: "#4ade80",
    brightYellow: "#facc15",
    brightBlue: "#60a5fa",
    brightMagenta: "#c084fc",
    brightCyan: "#22d3ee",
    brightWhite: "#f9fafb",
  };
}

function cssVar(name: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}
