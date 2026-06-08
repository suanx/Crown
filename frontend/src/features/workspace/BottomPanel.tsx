import { useWorkspaceStore } from "@/stores/workspaceStore";
import { ResizeHandle } from "./ResizeHandle";
import { PanelRouter } from "./PanelRouter";

export function BottomPanel() {
  const bottomContent = useWorkspaceStore((s) => s.bottomContent);
  const bottomH = useWorkspaceStore((s) => s.bottomH);
  const setBottomHeight = useWorkspaceStore((s) => s.setBottomHeight);

  if (!bottomContent && !useWorkspaceStore.getState().bottomContent) {
    return null;
  }

  return (
    <div className="relative h-full bg-elevated overflow-hidden border-t border-border-subtle">
      <ResizeHandle
        axis="y"
        side="top"
        current={bottomH}
        onResize={setBottomHeight}
        reverse
      />
      <PanelRouter kind={bottomContent} slot="bottom" />
    </div>
  );
}
