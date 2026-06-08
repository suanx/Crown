import { useState, useRef, useEffect } from "react";
import { BrandLogo } from "@/shared/icons/BrandLogo";
import { Icon } from "@/shared/icons/Icon";
import {
  CodeIcon,
  BugIcon,
  TestIcon,
  TerminalIcon,
  GlobeIcon,
} from "@/shared/icons/set";
import { ComposeBar } from "./ComposeBar";
import { agentClient } from "@/api";
import { useChatStore } from "@/stores/chatStore";
import { useSessionStore } from "@/stores/sessionStore";
import { useRouterStore } from "@/stores/routerStore";
import { useUiStore } from "@/stores/uiStore";

const QUICK_ACTIONS = [
  { icon: CodeIcon, label: "读代码", prompt: "帮我读一下当前项目结构" },
  { icon: BugIcon, label: "修 bug", prompt: "运行测试看看有什么报错" },
  { icon: TestIcon, label: "写测试", prompt: "帮我给这个文件加单元测试" },
  { icon: TerminalIcon, label: "跑命令", prompt: "" },
  { icon: GlobeIcon, label: "搜网页", prompt: "" },
] as const;

/**
 * 欢迎页 — Claude 桌面端风格 (输入框居中 → 发送后滑到底部).
 *
 * 布局原理:
 *   ┌─────────── flex-col ───────────┐
 *   │  TopSpacer   (flex:1 → flex:1) │  ← 发送后吃掉所有空间
 *   │  ─────────── content ───────── │
 *   │  Hero (logo + title)           │  ← 发送时 fade out
 *   │  ComposeBar                    │
 *   │  QuickActions                  │  ← 发送时 fade out
 *   │  ─────────── /content ──────── │
 *   │  BotSpacer  (flex:1 → flex:0) │  ← 发送后缩到 pb-6
 *   └────────────────────────────────┘
 *
 * idle:  两个 spacer 等高 → 内容垂直居中 (≈Claude 效果)
 * send:  TopSpacer flex → ∞, BotSpacer → 0+24px → 内容自然滑到底部
 *        与 ChatPage 的 pb-6 max-w-[760px] 对齐,切页无跳动
 *
 * flex 属性可 transition → 丝滑动画,无 absolute,无重叠 bug.
 */
export function WelcomePage() {
  const navigate = useRouterStore((s) => s.navigate);
  const loadThreads = useSessionStore((s) => s.loadThreads);
  const sendMessage = useChatStore((s) => s.sendMessage);
  const reloadThread = useChatStore((s) => s.reloadThread);

  // 入场渐显
  const [entered, setEntered] = useState(false);
  useEffect(() => {
    const raf = requestAnimationFrame(() => setEntered(true));
    return () => cancelAnimationFrame(raf);
  }, []);

  // 发送动画
  const [phase, setPhase] = useState<"idle" | "sending">("idle");
  const sendingRef = useRef(false);

  async function handleFirstSend(text: string) {
    if (sendingRef.current) return;
    sendingRef.current = true;
    setPhase("sending");

    try {
      const uiState = useUiStore.getState();
      const [summary] = await Promise.all([
        agentClient.createThread({
          model: uiState.currentModel,
          providerId: uiState.currentProviderId,
          thinkingEffort: uiState.currentThinkingEffort,
        }),
        // 等动画完成 (300ms transition + 50ms 余量)
        new Promise((r) => setTimeout(r, 350)),
      ]);

      void loadThreads();
      navigate({ page: "chat", threadId: summary.id });

      // 把 WelcomePage 阶段选择的权限模式应用到新 thread
      const selectedMode = uiState.permissionMode;
      if (selectedMode !== "default") {
        void agentClient.updateThread({
          threadId: summary.id,
          permissionMode: selectedMode,
        });
      }
      await reloadThread(summary.id);
      await sendMessage(summary.id, text);
    } catch (err) {
      setPhase("idle");
      sendingRef.current = false;
      if (import.meta.env.DEV) {
        // eslint-disable-next-line no-console
        console.warn("[WelcomePage] start new thread failed:", err);
      }
    }
  }

  const isSending = phase === "sending";
  const heroVisible = entered && !isSending;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* 顶部 Spacer — idle:flex-1 居中; sending:flex-1 吃满空间 → 内容下移 */}
      <div
        className="transition-[flex] duration-300 ease-out"
        style={{ flex: isSending ? "1 1 auto" : "1 1 0%", minHeight: 0 }}
      />

      {/* 内容块 — hero + compose + actions (整块一起移动) */}
      <div className="shrink-0 px-6">
        <div className="max-w-[760px] mx-auto">
          {/* Hero: logo + title */}
          <div
            className="flex flex-col items-center mb-6 transition-all duration-300 ease-out"
            style={{
              opacity: heroVisible ? 1 : 0,
              transform: heroVisible ? "translateY(0) scale(1)" : "translateY(-12px) scale(0.97)",
            }}
          >
            <div
              className="text-brand mb-3 transition-all duration-500 ease-out"
              style={{
                opacity: heroVisible ? 1 : 0,
                transform: heroVisible ? "translateY(0)" : "translateY(6px)",
              }}
            >
              <BrandLogo size={40} />
            </div>
            <h1
              className="text-2xl text-text-primary tracking-tight font-medium transition-all duration-500 ease-out"
              style={{
                opacity: heroVisible ? 1 : 0,
                transform: heroVisible ? "translateY(0)" : "translateY(6px)",
                transitionDelay: isSending ? "0ms" : "80ms",
              }}
            >
              你好,准备开始什么?
            </h1>
          </div>

          {/* ComposeBar — 始终可见 */}
          <ComposeBar
            autoFocus
            placeholder="给 Agent 发条消息开始..."
            onSend={(text) => void handleFirstSend(text)}
          />

          {/* 快捷动作 */}
          <div
            className="mt-4 flex flex-wrap justify-center gap-2 transition-opacity duration-200 ease-out"
            style={{ opacity: heroVisible ? 1 : 0 }}
          >
            {QUICK_ACTIONS.map((q) => {
              const disabled = !q.prompt || isSending;
              return (
                <button
                  key={q.label}
                  disabled={disabled}
                  onClick={() => {
                    if (q.prompt) void handleFirstSend(q.prompt);
                  }}
                  className="h-8 px-3 inline-flex items-center gap-1.5 rounded-md text-xs text-text-secondary bg-elevated border border-border-subtle hover:bg-hover hover:text-text-primary transition-colors focus-ring disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-elevated disabled:hover:text-text-secondary"
                  title={disabled ? "未实装" : q.prompt}
                >
                  <Icon icon={q.icon} size={13} className="text-brand" />
                  {q.label}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* 底部 Spacer — idle:flex-1 居中; sending:固定 24px (= ChatPage 的 pb-6) */}
      <div
        className="transition-[flex] duration-300 ease-out"
        style={{
          flex: isSending ? "0 0 24px" : "1 1 0%",
          minHeight: isSending ? "24px" : 0,
        }}
      />
    </div>
  );
}
