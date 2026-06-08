/**
 * ============================================================================
 * segments → 渲染单元分组
 * ============================================================================
 *
 * chatStore 的 `segments[]` 是 text / reasoning / tool 按真实时序交错的扁平
 * 数组。对话流去卡片化要求把**连续相邻的 tool 段**收成一个「工具组」
 * （对齐 Claude 桌面端 `使用了 N 个工具` / `搜索网络` 折叠头），而 text /
 * reasoning 段各自独立渲染。
 *
 * 这是纯函数，便于推理与测试：输入 segments，输出 RenderUnit[]，不依赖 React。
 *
 * 分组规则：
 *   - 遇到 text   → 收尾当前工具组，单独产出一个 text 单元。
 *   - 遇到 reasoning → 收尾当前工具组，单独产出一个 reasoning 单元。
 *   - 遇到 tool   → 累积进当前工具组（与前一个 tool 相邻则同组）。
 *   - 文本/思考一旦插入，就**打断**工具组：后续 tool 另起新组。
 *
 * 这样「工具前说的话 / 工具后说的话」天然分流（无需参考端的
 * toolTextEndOffset hack —— 我们的 segments 已是交错时序）。
 */

import type { Segment, ToolSegment } from "@/api";

export interface TextUnit {
  kind: "text";
  text: string;
  /** 在原 segments 中的下标，用作 React key 稳定锚点。 */
  index: number;
}

export interface ReasoningUnit {
  kind: "reasoning";
  text: string;
  index: number;
}

export interface ToolGroupUnit {
  kind: "toolGroup";
  tools: ToolSegment[];
  /** 组内第一个工具段在原 segments 中的下标，用作 React key。 */
  index: number;
}

export type RenderUnit = TextUnit | ReasoningUnit | ToolGroupUnit;

/**
 * 把交错的 segments 折叠成渲染单元：连续 tool 段合并为一个 toolGroup，
 * text / reasoning 各自独立。空 text / 空 reasoning 段被跳过（不产出空单元）。
 */
export function groupSegments(segments: Segment[]): RenderUnit[] {
  const units: RenderUnit[] = [];
  let current: ToolGroupUnit | null = null;

  const flush = () => {
    if (current) {
      units.push(current);
      current = null;
    }
  };

  segments.forEach((seg, index) => {
    if (seg.kind === "tool") {
      if (current) {
        current.tools.push(seg);
      } else {
        current = { kind: "toolGroup", tools: [seg], index };
      }
      return;
    }

    // 非工具段：先收尾工具组，再视情况产出文本/思考单元。
    flush();

    if (seg.kind === "text") {
      if (seg.text.trim().length === 0) return;
      units.push({ kind: "text", text: seg.text, index });
    } else if (seg.kind === "reasoning") {
      if (seg.text.trim().length === 0) return;
      units.push({ kind: "reasoning", text: seg.text, index });
    }
  });

  flush();
  return units;
}
