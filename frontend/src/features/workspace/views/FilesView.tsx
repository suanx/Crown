import { useCallback, useEffect, useMemo, useState } from "react";
import { agentClient, type FsEntry, type FsFile, type GrepMatch } from "@/api";
import { useActiveThread } from "@/stores/chatStore";
import { Icon } from "@/shared/icons/Icon";
import {
  CaretDownIcon,
  CaretRightIcon,
  FileIcon,
  FolderIcon,
  RefreshIcon,
  SearchIcon,
  FileSearchIcon,
  CloseIcon,
} from "@/shared/icons/set";
import { cn } from "@/shared/lib/cn";
import { PanelHeader } from "../PanelHeader";

interface DirectoryState {
  entries: FsEntry[];
  loading: boolean;
  error: string | null;
}

export interface FilesViewProps {
  slot: "right" | "bottom";
}

export function FilesView({ slot }: FilesViewProps) {
  const activeThread = useActiveThread();
  const root = activeThread?.projectId && activeThread.cwd ? activeThread.cwd : null;
  const [openPaths, setOpenPaths] = useState<Set<string>>(() => new Set());
  const [directories, setDirectories] = useState<Record<string, DirectoryState>>({});
  const [selected, setSelected] = useState<FsEntry | null>(null);
  const [preview, setPreview] = useState<FsFile | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<GrepMatch[]>([]);
  const [searching, setSearching] = useState(false);
  const [showSearch, setShowSearch] = useState(false);

  async function handleSearch() {
    const q = searchQuery.trim();
    if (!q || !root) return;
    setSearching(true);
    setShowSearch(true);
    try {
      const results = await agentClient.fsGrep(q, root, undefined, 100);
      setSearchResults(results);
    } catch { setSearchResults([]); }
    finally { setSearching(false); }
  }

  function clearSearch() {
    setSearchQuery("");
    setSearchResults([]);
    setShowSearch(false);
  }


  const loadDirectory = useCallback(async (path: string, force = false) => {
    setDirectories((prev) => {
      if (!force && prev[path]?.entries.length) return prev;
      return {
        ...prev,
        [path]: {
          entries: prev[path]?.entries ?? [],
          loading: true,
          error: null,
        },
      };
    });

    try {
      const entries = await agentClient.fsListDirectory(path);
      setDirectories((prev) => ({
        ...prev,
        [path]: { entries, loading: false, error: null },
      }));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setDirectories((prev) => ({
        ...prev,
        [path]: {
          entries: prev[path]?.entries ?? [],
          loading: false,
          error: message,
        },
      }));
    }
  }, []);

  useEffect(() => {
    setSelected(null);
    setPreview(null);
    setPreviewError(null);
    setDirectories({});
    if (!root) {
      setOpenPaths(new Set());
      return;
    }
    setOpenPaths(new Set([root]));
    void loadDirectory(root, true);
  }, [loadDirectory, root]);

  const rootName = useMemo(() => {
    if (!root) return "文件";
    const normalized = root.replace(/[\\/]+$/, "");
    const parts = normalized.split(/[\\/]/);
    return parts[parts.length - 1] || normalized;
  }, [root]);

  const toggleDirectory = useCallback(
    (entry: FsEntry) => {
      setOpenPaths((prev) => {
        const next = new Set(prev);
        if (next.has(entry.path)) {
          next.delete(entry.path);
        } else {
          next.add(entry.path);
          void loadDirectory(entry.path);
        }
        return next;
      });
    },
    [loadDirectory],
  );

  const openFile = useCallback(async (entry: FsEntry) => {
    setSelected(entry);
    setPreview(null);
    setPreviewError(null);
    setPreviewLoading(true);
    try {
      const file = await agentClient.fsReadFile(entry.path);
      setPreview(file);
    } catch (err) {
      setPreviewError(err instanceof Error ? err.message : String(err));
    } finally {
      setPreviewLoading(false);
    }
  }, []);

  const refreshRoot = useCallback(() => {
    if (!root) return;
    setDirectories({});
    setOpenPaths(new Set([root]));
    void loadDirectory(root, true);
  }, [loadDirectory, root]);

  return (
    <div className="h-full min-h-0 flex flex-col">
      <PanelHeader slot={slot} kind="files" />
      <div className={cn("flex-1 min-h-0", slot === "bottom" ? "flex" : "flex flex-col")}>
        <div className={cn("min-h-0 scrollable px-2 py-2 text-sm", slot === "bottom" ? "w-80 border-r border-border-subtle" : "flex-1")}>
          {/* Search bar */}
          <div className="flex items-center gap-1 px-2 pb-1.5 mb-1 border-b border-border-subtle">
            <Icon icon={SearchIcon} size={11} className="text-text-tertiary shrink-0" />
            <input value={searchQuery} onChange={e => setSearchQuery(e.target.value)}
              onKeyDown={e => { if (e.key === "Enter") void handleSearch(); }}
              placeholder="搜索文件内容..." className="flex-1 bg-transparent text-[11px] text-text-primary placeholder:text-text-tertiary outline-none" />
            {searchQuery && <button onClick={clearSearch} className="text-text-tertiary hover:text-text-primary"><Icon icon={CloseIcon} size={10} /></button>}
            <button onClick={() => void handleSearch()} disabled={!searchQuery.trim() || searching} className="text-[10px] text-text-tertiary hover:text-text-primary disabled:opacity-40 shrink-0">{searching ? "..." : "搜"}</button>
          </div>

          {showSearch ? (
            <div className="flex-1">
              {searchResults.length === 0 && !searching && <div className="p-3 text-center text-[11px] text-text-tertiary">无匹配</div>}
              {searchResults.map((r, i) => (
                <button key={i} onClick={() => { setSelected({ name: r.path.split(/[\\/]/).pop() || "", path: r.path, isDir: false, size: 0, modifiedMs: 0 }); void openFile({ name: r.path.split(/[\\/]/).pop() || "", path: r.path, isDir: false, size: 0, modifiedMs: 0 }); }}
                  className="w-full text-left px-2 py-1 hover:bg-hover transition-colors rounded">
                  <div className="flex items-center gap-1 text-[11px]">
                    <Icon icon={FileSearchIcon} size={10} className="text-text-tertiary shrink-0" />
                    <span className="text-text-primary truncate font-mono">{r.path.split(/[\\/]/).pop()}</span>
                    <span className="text-text-tertiary shrink-0">:{r.lineNumber}</span>
                  </div>
                  <div className="text-[10px] text-text-tertiary truncate mt-0.5 pl-4 font-mono">{r.line}</div>
                </button>
              ))}
              <button onClick={clearSearch} className="w-full text-center text-[10px] text-text-tertiary py-2 hover:text-text-primary">清除搜索</button>
            </div>
          ) : (
            <>
              <div className="mb-2 flex items-center gap-2 px-2 text-[11px] text-text-tertiary">
                <span className="min-w-0 flex-1 truncate">{root ?? "未进入项目"}</span>
                <button type="button" onClick={refreshRoot} disabled={!root} className="grid size-6 place-items-center rounded-md text-text-tertiary transition-colors hover:bg-hover hover:text-text-primary disabled:opacity-40" aria-label="刷新文件树">
                  <Icon icon={RefreshIcon} size={13} />
                </button>
              </div>
              {root ? (
                <DirectoryTree name={rootName} path={root} depth={0} directory={directories[root]}
                  openPaths={openPaths} directories={directories} selectedPath={selected?.path ?? null}
                  onToggle={toggleDirectory} onOpenFile={openFile} />
              ) : (
                <EmptyState text="在项目中打开对话后查看文件" />
              )}
            </>
          )}
        </div>
        <FilePreview
          slot={slot}
          selected={selected}
          preview={preview}
          loading={previewLoading}
          error={previewError}
        />
      </div>
    </div>
  );
}

function DirectoryTree({
  name,
  path,
  depth,
  directory,
  openPaths,
  directories,
  selectedPath,
  onToggle,
  onOpenFile,
}: {
  name: string;
  path: string;
  depth: number;
  directory: DirectoryState | undefined;
  openPaths: Set<string>;
  directories: Record<string, DirectoryState>;
  selectedPath: string | null;
  onToggle: (entry: FsEntry) => void;
  onOpenFile: (entry: FsEntry) => void;
}) {
  const rootEntry: FsEntry = {
    name,
    path,
    isDir: true,
    size: 0,
    modifiedMs: 0,
  };
  const open = openPaths.has(path);

  return (
    <>
      <DirectoryRow entry={rootEntry} depth={depth} open={open} onToggle={onToggle} />
      {open && (
        <DirectoryChildren
          depth={depth + 1}
          directory={directory}
          openPaths={openPaths}
          directories={directories}
          selectedPath={selectedPath}
          onToggle={onToggle}
          onOpenFile={onOpenFile}
        />
      )}
    </>
  );
}

function DirectoryChildren({
  depth,
  directory,
  openPaths,
  directories,
  selectedPath,
  onToggle,
  onOpenFile,
}: {
  depth: number;
  directory: DirectoryState | undefined;
  openPaths: Set<string>;
  directories: Record<string, DirectoryState>;
  selectedPath: string | null;
  onToggle: (entry: FsEntry) => void;
  onOpenFile: (entry: FsEntry) => void;
}) {
  if (!directory || directory.loading) {
    return <InlineState depth={depth} text="读取中..." />;
  }
  if (directory.error) {
    return <InlineState depth={depth} text={directory.error} tone="danger" />;
  }
  if (directory.entries.length === 0) {
    return <InlineState depth={depth} text="空目录" />;
  }

  return (
    <>
      {directory.entries.map((entry) =>
        entry.isDir ? (
          <div key={entry.path}>
            <DirectoryRow
              entry={entry}
              depth={depth}
              open={openPaths.has(entry.path)}
              onToggle={onToggle}
            />
            {openPaths.has(entry.path) && (
              <DirectoryChildren
                depth={depth + 1}
                directory={directories[entry.path]}
                openPaths={openPaths}
                directories={directories}
                selectedPath={selectedPath}
                onToggle={onToggle}
                onOpenFile={onOpenFile}
              />
            )}
          </div>
        ) : (
          <FileRow
            key={entry.path}
            entry={entry}
            depth={depth}
            selected={selectedPath === entry.path}
            onOpenFile={onOpenFile}
          />
        ),
      )}
    </>
  );
}

function DirectoryRow({
  entry,
  depth,
  open,
  onToggle,
}: {
  entry: FsEntry;
  depth: number;
  open: boolean;
  onToggle: (entry: FsEntry) => void;
}) {
  return (
    <Row depth={depth} onClick={() => onToggle(entry)}>
      <Icon
        icon={open ? CaretDownIcon : CaretRightIcon}
        size={11}
        className="text-text-tertiary shrink-0"
      />
      <Icon
        icon={FolderIcon}
        size={13}
        weight="duotone"
        className="text-brand shrink-0"
      />
      <span className="truncate font-medium text-text-primary">{entry.name}</span>
    </Row>
  );
}

function FileRow({
  entry,
  depth,
  selected,
  onOpenFile,
}: {
  entry: FsEntry;
  depth: number;
  selected: boolean;
  onOpenFile: (entry: FsEntry) => void;
}) {
  return (
    <Row depth={depth} selected={selected} onClick={() => onOpenFile(entry)}>
      <span className="w-[11px] shrink-0" />
      <Icon icon={FileIcon} size={13} className="text-text-tertiary shrink-0" />
      <span className="truncate">{entry.name}</span>
    </Row>
  );
}

function Row({
  depth,
  children,
  onClick,
  selected = false,
}: {
  depth: number;
  children: React.ReactNode;
  onClick: () => void;
  selected?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "w-full h-7 flex items-center gap-1.5 rounded-md transition-colors text-left focus-ring",
        selected
          ? "bg-brand/10 text-text-primary"
          : "text-text-secondary hover:bg-hover hover:text-text-primary",
      )}
      style={{ paddingLeft: 8 + depth * 12, paddingRight: 8 }}
    >
      {children}
    </button>
  );
}

function InlineState({
  depth,
  text,
  tone = "muted",
}: {
  depth: number;
  text: string;
  tone?: "muted" | "danger";
}) {
  return (
    <div
      className={cn(
        "h-7 flex items-center truncate text-xs",
        tone === "danger" ? "text-danger" : "text-text-tertiary",
      )}
      style={{ paddingLeft: 8 + depth * 12, paddingRight: 8 }}
    >
      {text}
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className="px-2 py-8 text-center text-xs text-text-tertiary">
      {text}
    </div>
  );
}

function FilePreview({
  slot,
  selected,
  preview,
  loading,
  error,
}: {
  slot: "right" | "bottom";
  selected: FsEntry | null;
  preview: FsFile | null;
  loading: boolean;
  error: string | null;
}) {
  const containerClass = cn(
    "min-h-0 border-border-subtle bg-bg-primary/40",
    slot === "bottom"
      ? "flex-1 border-l"
      : "max-h-[45%] border-t",
  );

  return (
    <div className={containerClass}>
      <div className="h-8 flex items-center gap-2 border-b border-border-subtle px-3 text-xs">
        <span className="min-w-0 flex-1 truncate text-text-secondary">
          {selected ? selected.name : "选择文件预览"}
        </span>
        {preview?.truncated && (
          <span className="shrink-0 text-[11px] text-text-tertiary">已截断</span>
        )}
      </div>
      <div className="h-[calc(100%-2rem)] min-h-0 scrollable p-3">
        {!selected && <EmptyState text="点击左侧文件查看内容" />}
        {selected && loading && <EmptyState text="正在读取文件" />}
        {selected && error && <EmptyState text={error} />}
        {selected && preview?.isBinary && <EmptyState text="二进制文件无法预览" />}
        {selected && preview && !preview.isBinary && (
          <pre className="whitespace-pre-wrap break-words font-mono text-[12px] leading-5 text-text-secondary">
            {preview.content || "空文件"}
          </pre>
        )}
      </div>
    </div>
  );
}
