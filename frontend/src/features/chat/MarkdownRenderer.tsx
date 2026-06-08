import { useEffect, useRef, useState, type ReactNode } from "react";
import { createIncremarkParser, type IncremarkParser } from "@incremark/core";
import type {
  Blockquote,
  Code,
  Heading,
  List,
  ListItem,
  Paragraph,
  PhrasingContent,
  Root,
  RootContent,
  Text,
  Link,
  InlineCode,
} from "mdast";
import { CodeBlock } from "./CodeBlock";
import { cn } from "@/shared/lib/cn";
import { sanitizeLinkUrl } from "@/shared/lib/safeUrl";

/**
 * 流式 markdown 渲染.
 *
 * 设计意图(用户裁定):**只用 Incremark 的增量 parser 算法,不要它的视觉**.
 *
 * **不用** @incremark/react 的 useIncremark hook —— 它内部维护一整套
 * setState 链 (completedBlocks / pendingBlocks / footnoteReferenceOrder /
 * displayBlocks 全是 useState + 一堆 useMemo + useEffect 互相喂),实测
 * 每个 ContentDelta 触发后会撞 React Maximum update depth 上限.
 *
 * 改用 @incremark/core 的低级 parser 直接对接:
 *   - useRef 持有 parser 实例,组件级单例
 *   - 自己一份 (ast, isFinalized) state,只在 append 后调 setAst 一次
 *   - 维护 lastContentRef,只 append 增量,不 reset/render 全量
 *
 * 渲染层完全自己写,用现有 Tailwind token + CodeBlock 组件,绕开
 * @incremark/react 的 IncremarkContent / theme css 视觉污染.
 *
 * 性能:
 *   - parser 是 O(增量) 的,长文档流式不退化
 *   - 未变 block 的 mdast 节点引用稳定,React reconciliation 命中跳过
 *   - 没有任何 hook 内部 setState 级联
 */
export interface MarkdownRendererProps {
  content: string;
  /** 流式中传 true,turn_complete 后传 false. */
  streaming?: boolean;
}

export function MarkdownRenderer({
  content,
  streaming = false,
}: MarkdownRendererProps) {
  const parserRef = useRef<IncremarkParser | null>(null);
  if (parserRef.current === null) {
    parserRef.current = createIncremarkParser();
  }

  const [ast, setAst] = useState<Root>(() => ({ type: "root", children: [] }));
  const lastContentRef = useRef("");
  const isFinalizedRef = useRef(false);
  // rAF 合批:同一帧内多个 token append 只 setAst 一次.flash 模型可达
  // 100+ tok/s,每个 delta 一次 setState 会撞 React nestedUpdateCount
  // 警告 (chatStore 那条 setState 已经触发一轮渲染,这里再连发等于栈
  // 套栈).rAF 把所有同帧 append 累成一次提交,React 60fps 限速.
  const rafIdRef = useRef<number | null>(null);

  useEffect(() => {
    const parser = parserRef.current;
    if (!parser) return;
    const prev = lastContentRef.current;

    if (content === prev && (streaming || isFinalizedRef.current)) return;

    if (content.startsWith(prev) && !isFinalizedRef.current) {
      const delta = content.slice(prev.length);
      if (delta) parser.append(delta);
    } else {
      parser.reset();
      isFinalizedRef.current = false;
      if (content) parser.append(content);
    }
    lastContentRef.current = content;

    const finalizeNow = !streaming && !isFinalizedRef.current;
    if (finalizeNow) {
      parser.finalize();
      isFinalizedRef.current = true;
    }

    // 流式中走 rAF 合批;finalize 走同步立即 commit,避免末尾少一帧.
    if (finalizeNow) {
      if (rafIdRef.current !== null) {
        cancelAnimationFrame(rafIdRef.current);
        rafIdRef.current = null;
      }
      setAst(parser.getAst());
    } else if (rafIdRef.current === null) {
      rafIdRef.current = requestAnimationFrame(() => {
        rafIdRef.current = null;
        const p = parserRef.current;
        if (p) setAst(p.getAst());
      });
    }
  }, [content, streaming]);

  // unmount cleanup
  useEffect(() => {
    return () => {
      if (rafIdRef.current !== null) {
        cancelAnimationFrame(rafIdRef.current);
        rafIdRef.current = null;
      }
    };
  }, []);

  const blocks = ast.children;
  if (blocks.length === 0) {
    return null;
  }

  return (
    <div className="space-y-3 leading-relaxed relative">
      {blocks.map((block, i) => (
        <RenderBlock key={getBlockKey(block, i)} node={block} />
      ))}
    </div>
  );
}

// ── 渲染分发 ─────────────────────────────────────────────────────────────

interface BlockProps {
  node: RootContent;
}

function RenderBlock({ node }: BlockProps): ReactNode {
  switch (node.type) {
    case "heading":
      return <RenderHeading node={node as Heading} />;
    case "paragraph":
      return <RenderParagraph node={node as Paragraph} />;
    case "code":
      return <RenderCode node={node as Code} />;
    case "list":
      return <RenderList node={node as List} />;
    case "blockquote":
      return <RenderBlockquote node={node as Blockquote} />;
    case "thematicBreak":
      return <hr className="border-border-subtle" />;
    case "html":
      return (
        <pre className="text-xs text-text-tertiary font-mono whitespace-pre-wrap">
          {(node as { value?: string }).value ?? ""}
        </pre>
      );
    default:
      return <RenderInlineList nodes={extractPhrasing(node)} />;
  }
}

function RenderHeading({ node }: { node: Heading }) {
  const cls =
    node.depth === 1
      ? "text-xl font-semibold text-text-primary"
      : node.depth === 2
        ? "text-lg font-semibold text-text-primary"
        : "text-base font-semibold text-text-primary";
  return (
    <div className={cls}>
      <RenderInlineList nodes={node.children} />
    </div>
  );
}

function RenderParagraph({ node }: { node: Paragraph }) {
  return (
    <p className="leading-relaxed text-text-primary">
      <RenderInlineList nodes={node.children} />
    </p>
  );
}

function RenderCode({ node }: { node: Code }) {
  return <CodeBlock lang={node.lang ?? "text"} code={node.value} />;
}

function RenderList({ node }: { node: List }) {
  const Tag = node.ordered ? "ol" : "ul";
  return (
    <Tag
      className={cn(
        "ml-4 space-y-1 leading-relaxed text-text-primary",
        node.ordered
          ? "list-decimal marker:text-text-tertiary"
          : "list-disc marker:text-text-tertiary",
      )}
    >
      {node.children.map((item, i) => (
        <RenderListItem key={i} node={item as ListItem} />
      ))}
    </Tag>
  );
}

function RenderListItem({ node }: { node: ListItem }) {
  return (
    <li>
      {node.children.map((child, i) => {
        if (child.type === "paragraph") {
          return (
            <span key={i}>
              <RenderInlineList nodes={(child as Paragraph).children} />
            </span>
          );
        }
        return <RenderBlock key={i} node={child} />;
      })}
    </li>
  );
}

function RenderBlockquote({ node }: { node: Blockquote }) {
  return (
    <blockquote className="border-l-2 border-border-default pl-4 text-text-secondary italic">
      {node.children.map((child, i) => (
        <RenderBlock key={i} node={child} />
      ))}
    </blockquote>
  );
}

// ── 行内渲染 ─────────────────────────────────────────────────────────────

function RenderInlineList({ nodes }: { nodes: PhrasingContent[] }) {
  return (
    <>
      {nodes.map((n, i) => (
        <RenderInline key={i} node={n} />
      ))}
    </>
  );
}

function RenderInline({ node }: { node: PhrasingContent }): ReactNode {
  switch (node.type) {
    case "text":
      return (node as Text).value;
    case "strong":
      return (
        <strong className="font-semibold text-text-primary">
          <RenderInlineList
            nodes={(node as { children: PhrasingContent[] }).children}
          />
        </strong>
      );
    case "emphasis":
      return (
        <em className="italic">
          <RenderInlineList
            nodes={(node as { children: PhrasingContent[] }).children}
          />
        </em>
      );
    case "delete":
      return (
        <del className="text-text-tertiary line-through">
          <RenderInlineList
            nodes={(node as { children: PhrasingContent[] }).children}
          />
        </del>
      );
    case "inlineCode":
      return (
        <code className="px-1.5 py-0.5 mx-0.5 rounded font-mono text-[13px] bg-elevated text-text-primary border border-border-subtle">
          {(node as InlineCode).value}
        </code>
      );
    case "link": {
      const link = node as Link;
      const safeHref = sanitizeLinkUrl(link.url);
      // Untrusted source (LLM output / web tool results): a blocked URL
      // (javascript:, data:, etc.) must not become a navigable href. Render
      // it as plain, non-navigable text instead so script can't execute.
      if (safeHref === null) {
        return (
          <span className="text-text-primary underline decoration-dotted">
            <RenderInlineList nodes={link.children} />
          </span>
        );
      }
      return (
        <a
          href={safeHref}
          target="_blank"
          rel="noopener noreferrer"
          className="text-brand hover:underline"
        >
          <RenderInlineList nodes={link.children} />
        </a>
      );
    }
    case "break":
      return <br />;
    default:
      if ("value" in node && typeof (node as { value: unknown }).value === "string") {
        return (node as { value: string }).value;
      }
      if ("children" in node) {
        return (
          <RenderInlineList
            nodes={(node as { children: PhrasingContent[] }).children}
          />
        );
      }
      return null;
  }
}

// ── helpers ──────────────────────────────────────────────────────────────

function extractPhrasing(node: unknown): PhrasingContent[] {
  if (!node || typeof node !== "object") return [];
  const children = (node as { children?: unknown }).children;
  return Array.isArray(children) ? (children as PhrasingContent[]) : [];
}

function getBlockKey(_block: RootContent, fallback: number): string {
  // 流式期间 parser 频繁重 parse 末尾未结束 block,position offset 还不稳定
  // (incremark 可能给多个 pending block 相同的 start offset 0).
  // 改用 fallback index 作为唯一 key;finalized 后 blocks 不再变,index 也稳定.
  return `b-${fallback}`;
}
