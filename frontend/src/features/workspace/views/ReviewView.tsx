import { PanelHeader } from "../PanelHeader";
import { DiffView } from "@/features/chat/DiffView";
import { Icon } from "@/shared/icons/Icon";
import { FileIcon } from "@/shared/icons/set";
import { Pill } from "@/shared/ui/Pill";

const MOCK_CHANGES = [
  {
    path: "src/main.rs",
    additions: 8,
    deletions: 3,
    before: `fn main() {
    println!("Hello, world!");
}`,
    after: `use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, axum!" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}`,
  },
];

export interface ReviewViewProps {
  slot: "right" | "bottom";
}

export function ReviewView({ slot }: ReviewViewProps) {
  return (
    <div className="h-full flex flex-col">
      <PanelHeader slot={slot} kind="review" />
      <div className="flex-1 min-h-0 scrollable px-3 py-3 space-y-3">
        <div className="flex items-center gap-2 text-xs text-text-tertiary">
          <Icon icon={FileIcon} size={12} />
          {MOCK_CHANGES.length} 个文件已修改
          <Pill tone="success">+{MOCK_CHANGES.reduce((s, c) => s + c.additions, 0)}</Pill>
          <Pill tone="danger">−{MOCK_CHANGES.reduce((s, c) => s + c.deletions, 0)}</Pill>
        </div>

        {MOCK_CHANGES.map((c) => (
          <DiffView key={c.path} path={c.path} before={c.before} after={c.after} />
        ))}
      </div>
    </div>
  );
}
