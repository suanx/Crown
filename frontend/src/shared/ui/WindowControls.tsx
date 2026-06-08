import { useEffect, useState } from "react";

/**
 * 自绘窗口控制按钮 — 最小化 / 最大化 / 关闭.
 *
 * 双模式:
 *   - Tauri 环境: 调用 @tauri-apps/api/window 原生 API
 *   - 浏览器 dev 环境: 优雅降级 (fullscreen toggle / close tab)
 */

interface WinApi {
  minimize: () => void;
  toggleMaximize: () => void;
  close: () => void;
}

// Lazy-init: 首次渲染时尝试加载 Tauri API
let winApiPromise: Promise<WinApi | null> | null = null;

function getWinApi(): Promise<WinApi | null> {
  if (!winApiPromise) {
    winApiPromise = import("@tauri-apps/api/window")
      .then((mod) => {
        const win = mod.getCurrentWindow();
        return {
          minimize: () => void win.minimize(),
          toggleMaximize: () => void win.toggleMaximize(),
          close: () => void win.close(),
        };
      })
      .catch(() => null); // 浏览器环境 — 模块不存在
  }
  return winApiPromise;
}

export function WindowControls() {
  const [api, setApi] = useState<WinApi | null>(null);

  useEffect(() => {
    void getWinApi().then(setApi);
  }, []);

  function handleMinimize() {
    if (api) {
      api.minimize();
    }
    // 浏览器无法最小化
  }

  function handleToggleMaximize() {
    if (api) {
      api.toggleMaximize();
    } else if (document.fullscreenElement) {
      void document.exitFullscreen();
    } else {
      void document.documentElement.requestFullscreen();
    }
  }

  function handleClose() {
    if (api) {
      api.close();
    } else {
      window.close();
    }
  }

  return (
    <div className="flex items-center gap-0 shrink-0">
      <WinBtn
        label="最小化"
        onClick={handleMinimize}
        className="hover:bg-hover"
      >
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path d="M1 5h8" stroke="currentColor" strokeWidth="1.2" />
        </svg>
      </WinBtn>

      <WinBtn
        label="最大化"
        onClick={handleToggleMaximize}
        className="hover:bg-hover"
      >
        <svg width="10" height="10" viewBox="0 0 10 10">
          <rect
            x="1.5"
            y="1.5"
            width="7"
            height="7"
            rx="1"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.2"
          />
        </svg>
      </WinBtn>

      <WinBtn
        label="关闭"
        onClick={handleClose}
        className="hover:bg-danger hover:text-white"
      >
        <svg width="10" height="10" viewBox="0 0 10 10">
          <path
            d="M2 2l6 6M8 2l-6 6"
            stroke="currentColor"
            strokeWidth="1.2"
            strokeLinecap="round"
          />
        </svg>
      </WinBtn>
    </div>
  );
}

function WinBtn({
  label,
  onClick,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={label}
      className={`h-8 w-11 flex items-center justify-center text-text-tertiary transition-colors ${className ?? ""}`}
    >
      {children}
    </button>
  );
}
