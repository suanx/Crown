import type { PanelKind } from "@/stores/workspaceStore";
import { FilesView } from "./views/FilesView";
import { BrowserView } from "./views/BrowserView";
import { ReviewView } from "./views/ReviewView";
import { TerminalView } from "./views/TerminalView";
import { TasksView } from "./views/TasksView";
import { PanelEmpty } from "./PanelEmpty";

export function PanelRouter({
  kind,
  slot,
}: {
  kind: PanelKind | null;
  slot: "right" | "bottom";
}) {
  if (!kind) return <PanelEmpty slot={slot} />;
  switch (kind) {
    case "files":
      return <FilesView slot={slot} />;
    case "tasks":
      return <TasksView slot={slot} />;
    case "browser":
      return <BrowserView slot={slot} />;
    case "review":
      return <ReviewView slot={slot} />;
    case "terminal":
      return <TerminalView slot={slot} />;
  }
}
