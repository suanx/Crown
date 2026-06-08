import type { MessageUsage } from "@/api";
import { formatTokens, formatDuration } from "@/shared/lib/format";
import { useMessageDuration } from "@/stores/chatStore";

/**
 * 消息底部 meta —— 对齐 Claude 桌面端的极简一行：`6s · ↓10.5k`。
 *
 *   - 耗时：本条消息所在 turn 的 wall-clock 秒数（前端观测，store 记录）。
 *   - ↓tokens：输出 token 数（下箭头表示"模型产出"）。
 *
 * 缓存命中率已移除：对话场景下 DeepSeek 前缀缓存命中率恒为 ~95-100%，
 * 信息量低且反直觉（一句"hi"也显示 99%）。累计缓存收益在 Billing 面板看。
 *
 * 对齐要点（修"没对齐/间距怪"）：
 *   - 不给整行套 font-mono（中文无等宽字形会回退异体字、基线错乱），只给纯
 *     数字片段挂 tabular-nums。
 *   - 统一 gap-2，分隔符 `·` 作独立 dim 元素，左缘与正文严格对齐。
 */
export function MessageMeta({
  usage,
  messageId,
}: {
  usage: MessageUsage;
  messageId: string;
}) {
  const durationMs = useMessageDuration(messageId);

  const parts: React.ReactNode[] = [];
  if (durationMs != null && durationMs > 0) {
    parts.push(
      <span key="dur" className="tabular-nums">
        {formatDuration(durationMs)}
      </span>,
    );
  }
  parts.push(
    <span key="out" className="tabular-nums" title={`输出 ${usage.outputTokens} tokens`}>
      ↓{formatTokens(usage.outputTokens)}
    </span>,
  );

  return (
    <div className="flex items-center gap-2 text-xs text-text-tertiary leading-none">
      {parts.map((p, i) => (
        <span key={i} className="inline-flex items-center gap-2">
          {i > 0 && <span className="opacity-30 select-none">·</span>}
          {p}
        </span>
      ))}
    </div>
  );
}
