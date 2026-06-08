import { useEffect, useRef, useState, useCallback } from "react";
import type { Message } from "@/api";
import { cn } from "@/shared/lib/cn";

/**
 * 对话时间轴 Scrubber —— 贴对话区右边缘的极窄竖向刻度，每根杠对应一条用户
 * 消息（一个对话节点）。
 *
 * 设计意图（用户裁定「艺术一点、别普通、不占空间、无依赖」）：
 *   - 收起态：~14px 宽的一列细横杠，当前视口所在节点 = 蓝色长 pill。
 *   - 悬停态：整列向左展开成浮层，逐行显示该消息文字预览（右对齐）。
 *   - macOS Dock 磁吸：光标在某根杠附近时，该杠及邻近杠按到光标的距离平滑
 *     变长变亮（越近越长），用一条 rAF 驱动的 pointer-Y 实现。
 *   - active pill 弹性滑动、点击 smooth-scroll + 目标消息高亮闪动。
 *   - 全程纯 CSS transform + 一个 rAF 循环，无第三方依赖。
 *   - 尊重 prefers-reduced-motion：磁吸/滑动退化为静态。
 *
 * 数据：父级 MessageList 给每条消息根元素打 `data-msg-id`，本组件只取
 * **用户消息**作为节点（更稀疏、更有「对话节点」感）。位置按真实 offsetTop
 * 归一化，不假设等高。
 */

interface ScrubberNode {
  id: string;
  /** 预览文字（用户消息前若干字）。 */
  preview: string;
}

interface MessageScrubberProps {
  /** 当前线程的消息（用于派生用户节点 + 预览）。 */
  messages: Message[];
  /** 滚动容器（读 scrollTop/scrollHeight，写 smooth scroll）。 */
  scrollRef: React.RefObject<HTMLDivElement | null>;
}

const MAGNIFY_RADIUS = 64; // px，磁吸影响半径
const PREVIEW_LEN = 22; // 预览截断字数

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
  );
}

export function MessageScrubber({ messages, scrollRef }: MessageScrubberProps) {
  const userNodes = useUserNodes(messages);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [hovered, setHovered] = useState(false);
  // 光标在 scrubber 内的 Y（px，相对 scrubber 顶部）；null = 不在其上。
  const pointerYRef = useRef<number | null>(null);
  const railRef = useRef<HTMLDivElement>(null);
  // 每根杠的 DOM，用于 rAF 磁吸写 transform（绕开 React 每帧 setState）。
  const tickRefs = useRef<Map<string, HTMLButtonElement>>(new Map());
  const rafRef = useRef<number | null>(null);
  const reduced = prefersReducedMotion();

  // 滚轮导航要在原生事件回调里读到「当前 active」与「最新节点列表」，但回调
  // 不能频繁重绑（active 滚动时一直变）。用 ref 镜像，渲染期同步即可。
  const activeIdRef = useRef<string | null>(null);
  activeIdRef.current = activeId;
  const userNodesRef = useRef<ScrubberNode[]>(userNodes);
  userNodesRef.current = userNodes;

  // ── active 节点检测：滚动时取「视口顶部下方最近的用户节点」 ──
  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    let raf = 0;
    const recompute = () => {
      raf = 0;
      const top = scroller.scrollTop;
      const probe = top + scroller.clientHeight * 0.3; // 视口上 1/3 处为准线
      let current: string | null = userNodes[0]?.id ?? null;
      for (const n of userNodes) {
        const el = scroller.querySelector<HTMLElement>(
          `[data-msg-id="${cssEscape(n.id)}"]`,
        );
        if (!el) continue;
        if (el.offsetTop <= probe) current = n.id;
        else break;
      }
      setActiveId(current);
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(recompute);
    };
    recompute();
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      scroller.removeEventListener("scroll", onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [userNodes, scrollRef]);

  // ── 磁吸放大：rAF 读 pointerY，给每根杠写 scaleX + 亮度 ──
  useEffect(() => {
    if (reduced) return;
    const loop = () => {
      rafRef.current = requestAnimationFrame(loop);
      const py = pointerYRef.current;
      for (const [, el] of tickRefs.current) {
        const center = el.offsetTop + el.offsetHeight / 2;
        let m = 0; // 0..1 放大强度
        if (py !== null) {
          const d = Math.abs(py - center);
          if (d < MAGNIFY_RADIUS) m = 1 - d / MAGNIFY_RADIUS;
        }
        // 越近越长（scaleX 1→2.4）、越亮（opacity 基线→1）。
        const sx = 1 + m * m * 1.4;
        el.style.setProperty("--mag", m.toFixed(3));
        el.style.setProperty("--sx", sx.toFixed(3));
      }
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [reduced, userNodes]);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const rail = railRef.current;
    if (!rail) return;
    pointerYRef.current = e.clientY - rail.getBoundingClientRect().top;
  }, []);

  const onPointerLeave = useCallback(() => {
    pointerYRef.current = null;
    setHovered(false);
  }, []);

  const jumpTo = useCallback(
    (id: string) => {
      const scroller = scrollRef.current;
      if (!scroller) return;
      const el = scroller.querySelector<HTMLElement>(
        `[data-msg-id="${cssEscape(id)}"]`,
      );
      if (!el) return;
      scroller.scrollTo({
        top: el.offsetTop - 16,
        behavior: reduced ? "auto" : "smooth",
      });
      // 目标消息高亮闪动（CSS 动画类，自动移除）。
      el.classList.remove("msg-flash");
      void el.offsetWidth; // reflow 重置动画
      el.classList.add("msg-flash");
    },
    [scrollRef, reduced],
  );

  // jumpTo 的稳定镜像：滚轮原生监听器只绑一次，不随 jumpTo 重建而重绑。
  const jumpToRef = useRef(jumpTo);
  jumpToRef.current = jumpTo;

  // ── 滚轮导航：光标悬停在 scrubber 上时，滚轮一格 = 跳到上/下一个对话节点 ──
  // 用原生非 passive 监听器以便 preventDefault，阻止滚轮穿透继续滚动对话区
  // （否则会同时触发普通滚动，导致跳一下又被拉回）。一次连续滚动用 cooldown
  // 节流成「一格一跳」，不会因触控板惯性瞬间冲到底。
  useEffect(() => {
    const rail = railRef.current;
    if (!rail) return;
    let cooldown = false;
    const onWheel = (e: WheelEvent) => {
      const nodes = userNodesRef.current;
      if (nodes.length < 2) return;
      e.preventDefault(); // 接管：不让对话区跟着滚
      if (cooldown) return;
      const dir = e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0;
      if (dir === 0) return;
      const curId = activeIdRef.current ?? nodes[0]?.id ?? null;
      const idx = Math.max(
        0,
        nodes.findIndex((n) => n.id === curId),
      );
      const nextIdx = Math.min(nodes.length - 1, Math.max(0, idx + dir));
      if (nextIdx === idx) return; // 已在两端，无可跳
      jumpToRef.current(nodes[nextIdx].id);
      cooldown = true;
      window.setTimeout(() => {
        cooldown = false;
      }, 280);
    };
    rail.addEventListener("wheel", onWheel, { passive: false });
    return () => rail.removeEventListener("wheel", onWheel);
  }, []);

  if (userNodes.length < 2) return null; // 太少不值得显示

  return (
    <div
      ref={railRef}
      onPointerMove={onPointerMove}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={onPointerLeave}
      className={cn(
        // 外层只负责定位 + 悬停热区 + pointer 跟踪：贴右边缘、纵向铺满以便
        // rAF 读 pointer-Y，横向靠右对齐让内容盒贴边。pl-7 扩出一段透明热
        // 区方便从对话区右缘滑入；pr-3 让杠列右沿避开 8px 滚动条（留 4px
        // 余量），不再与滚动条重叠。本身不画任何背板。
        "absolute right-0 top-0 bottom-0 z-20 flex flex-col items-end justify-center",
        "py-10 pl-7 pr-3 select-none",
      )}
      style={{ pointerEvents: "auto" }}
      aria-label="对话时间轴"
    >
      {/* 内容盒：收缩包裹（w-fit），背板紧贴实际内容——不再撑满高度/宽度，
          因此不会出现"巨大半空 + 透出背景光斑"。悬停态浮出深色毛玻璃背板，
          收起态完全透明只露出右侧细杠列。 */}
      <div
        className={cn(
          "flex flex-col items-end gap-1 w-fit max-w-[240px] rounded-2xl",
          "transition-[background-color,box-shadow,border-color,padding] duration-300 ease-out",
          hovered
            ? "bg-elevated/95 backdrop-blur-md border border-white/[0.10] shadow-[0_10px_34px_-8px_rgba(0,0,0,0.6)] px-2.5 py-2.5"
            : "bg-transparent border border-transparent shadow-none px-1 py-1",
        )}
      >
        {userNodes.map((n) => {
          const active = n.id === activeId;
          return (
            <button
              key={n.id}
              ref={(el) => {
                if (el) tickRefs.current.set(n.id, el);
                else tickRefs.current.delete(n.id);
              }}
              onClick={() => jumpTo(n.id)}
              data-active={active ? "true" : "false"}
              className={cn(
                // 自然宽度（fit-content）+ items-end 右对齐：杠永远对齐右沿，
                // 文字向左自然延伸；最长的一行决定内容盒宽度，其余行靠右。
                "scrubber-tick group/tick relative flex items-center justify-end gap-2.5 rounded-lg",
                hovered ? "py-1 pl-2.5 pr-1 hover:bg-white/[0.06]" : "h-3",
              )}
              title={n.preview}
            >
              {/* 预览文字：仅悬停态**渲染**（收起态彻底不占位、不溢出到对话区）。
                  用 max-w 截断而非 flex-1 撑满——短文字自然窄、紧贴杠左侧，
                  长文字截断；text-left 让首字对齐易读。 */}
              {hovered && (
                <span
                  className={cn(
                    "scrubber-label truncate text-xs text-left max-w-[188px] animate-fade-in",
                    active
                      ? "text-text-primary font-medium"
                      : "text-text-secondary",
                  )}
                >
                  {n.preview}
                </span>
              )}
              {/* 刻度杠：active=蓝色长 pill，其余=灰短杠；磁吸用 --sx 放大 */}
              <span
                className={cn(
                  "scrubber-bar shrink-0 rounded-full transition-colors",
                  active
                    ? "bg-brand h-[3px] w-5"
                    : "bg-text-tertiary/40 group-hover/tick:bg-text-secondary h-[2px] w-2.5",
                )}
              />
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** 派生用户消息节点 + 预览文字。 */
function useUserNodes(messages: Message[]): ScrubberNode[] {
  const [nodes, setNodes] = useState<ScrubberNode[]>([]);
  useEffect(() => {
    const next: ScrubberNode[] = messages
      .filter((m) => m.role === "user")
      .map((m) => ({
        id: m.id,
        preview: previewOf(m.content),
      }));
    setNodes(next);
  }, [messages]);
  return nodes;
}

/** 取用户消息正文前若干字作预览，剥掉斜杠命令注入的 system-reminder。 */
function previewOf(content: string): string {
  const stripped = content
    .replace(/^<system-reminder>[\s\S]*?<\/system-reminder>\s*/i, "")
    .replace(/\s+/g, " ")
    .trim();
  if (stripped.length <= PREVIEW_LEN) return stripped || "(空消息)";
  return stripped.slice(0, PREVIEW_LEN) + "…";
}

/** 简易 CSS 属性选择器转义（消息 id 是 ULID/本地 id，仅含安全字符，但稳妥起见）。 */
function cssEscape(s: string): string {
  if (typeof CSS !== "undefined" && CSS.escape) return CSS.escape(s);
  return s.replace(/["\\]/g, "\\$&");
}
