import { useWorkspaceStore } from "@/stores/workspaceStore";
import { ResizeHandle } from "./ResizeHandle";
import { PanelRouter } from "./PanelRouter";

/**
 * 右侧面板容器.
 * 宽度由 store + CSS 变量驱动,这里只渲染内容 + 拖拽手柄.
 *
 * 拖拽方向:左边缘 + reverse(向左拖增宽).
 */
export function RightPanel() {
  const rightContent = useWorkspaceStore((s) => s.rightContent);
  const rightW = useWorkspaceStore((s) => s.rightW);
  const setRightWidth = useWorkspaceStore((s) => s.setRightWidth);

  if (!rightContent && !useWorkspaceStore.getState().rightContent) {
    // 关闭时返回 null,grid track 由 CSS 变量已是 0
    return null;
  }

  return (
    <div className="relative h-full bg-elevated overflow-hidden">
      <ResizeHandle
        axis="x"
        side="left"
        current={rightW}
        onResize={setRightWidth}
        reverse
      />
      <PanelRouter kind={rightContent} slot="right" />
    </div>
  );
}
