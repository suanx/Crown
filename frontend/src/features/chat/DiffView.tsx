import { Icon } from "@/shared/icons/Icon";
import { FileIcon } from "@/shared/icons/set";

export interface DiffViewProps {
  path: string;
  before: string;
  after: string;
}

/**
 * 极简 line-level diff 渲染.
 * 不做 LCS,直接按行对齐 (够用于 mock 展示).
 * 后续可替换 monaco-diff / diff-match-patch.
 */
export function DiffView({ path, before, after }: DiffViewProps) {
  const beforeLines = before.split("\n");
  const afterLines = after.split("\n");
  const max = Math.max(beforeLines.length, afterLines.length);

  return (
    <div
      className="rounded-lg overflow-hidden border border-border-subtle"
      style={{ backgroundColor: "var(--ds-code-bg)" }}
    >
      <div
        className="flex items-center gap-2 px-3 py-1.5 border-b text-xs font-mono"
        style={{
          color: "var(--ds-code-text)",
          borderColor: "rgba(255,255,255,0.08)",
        }}
      >
        <Icon icon={FileIcon} size={12} className="opacity-60" />
        <span>{path}</span>
        <span className="ml-auto opacity-60">
          <span className="text-[#4ade80]">+{afterLines.length}</span>{" "}
          <span className="text-[#f87171]">-{beforeLines.length}</span>
        </span>
      </div>
      <div
        className="overflow-x-auto text-sm leading-[1.6] font-mono py-1"
        style={{ color: "var(--ds-code-text)" }}
      >
        {Array.from({ length: max }).map((_, i) => {
          const b = beforeLines[i];
          const a = afterLines[i];
          if (b !== undefined && b === a) {
            return <DiffLine key={i} mark=" " text={b} />;
          }
          return (
            <div key={i}>
              {b !== undefined && (
                <DiffLine
                  mark="-"
                  text={b}
                  bg="rgba(248, 113, 113, 0.15)"
                  fg="#f87171"
                />
              )}
              {a !== undefined && (
                <DiffLine
                  mark="+"
                  text={a}
                  bg="rgba(74, 222, 128, 0.12)"
                  fg="#4ade80"
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function DiffLine({
  mark,
  text,
  bg,
  fg,
}: {
  mark: string;
  text: string;
  bg?: string;
  fg?: string;
}) {
  return (
    <div
      className="px-3 flex items-baseline gap-3"
      style={{ backgroundColor: bg }}
    >
      <span
        className="select-none inline-block w-3 text-center opacity-70"
        style={{ color: fg }}
      >
        {mark}
      </span>
      <span style={{ color: fg }}>{text || "\u00A0"}</span>
    </div>
  );
}
