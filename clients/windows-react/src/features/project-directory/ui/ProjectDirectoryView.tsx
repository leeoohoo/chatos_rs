import { useEffect } from "react";
import { useProjectDirectoryStore } from "../application/projectDirectoryStore";
import {
  displayProjectPath,
  flattenDirectoryTree,
  type ProjectFileEntry,
} from "../domain/projectDirectory";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";

export function ProjectDirectoryView({
  ownerUserId,
  projectId,
  port,
}: {
  ownerUserId: string;
  projectId: string;
  port: ProjectDirectoryPort;
}) {
  const listings = useProjectDirectoryStore((state) => state.listings);
  const expandedPaths = useProjectDirectoryStore((state) => state.expandedPaths);
  const loadingPaths = useProjectDirectoryStore((state) => state.loadingPaths);
  const selectedEntry = useProjectDirectoryStore((state) => state.selectedEntry);
  const rootLoading = useProjectDirectoryStore((state) => state.rootLoading);
  const error = useProjectDirectoryStore((state) => state.error);
  const activate = useProjectDirectoryStore((state) => state.activate);
  const toggle = useProjectDirectoryStore((state) => state.toggle);
  const select = useProjectDirectoryStore((state) => state.select);
  const refresh = useProjectDirectoryStore((state) => state.refresh);
  const reset = useProjectDirectoryStore((state) => state.reset);

  useEffect(() => {
    void activate(port, ownerUserId, projectId);
  }, [activate, ownerUserId, port, projectId]);

  useEffect(() => () => reset(), [ownerUserId, projectId, reset]);

  const visibleEntries = flattenDirectoryTree(listings, expandedPaths, loadingPaths);
  const root = listings[""];

  return (
    <div className="project-directory" aria-label="项目目录">
      <aside className="project-directory__sidebar">
        <header className="project-directory__toolbar">
          <div><span aria-hidden="true">▱</span><strong>项目文件</strong></div>
          <button disabled={rootLoading} onClick={() => void refresh(port)} title="刷新目录" type="button">↻</button>
        </header>
        {error && (
          <div className="project-directory__error" role="alert">
            <span>{error}</span>
            <button onClick={() => void refresh(port)} type="button">重试</button>
          </div>
        )}
        {rootLoading && !root ? (
          <div className="project-directory__empty"><span className="mini-spinner" aria-hidden="true" />正在读取项目目录…</div>
        ) : !root ? (
          <div className="project-directory__empty">项目目录暂时不可用。</div>
        ) : visibleEntries.length === 0 ? (
          <div className="project-directory__empty">当前项目目录为空。</div>
        ) : (
          <div className="project-directory__tree" role="tree">
            {visibleEntries.map((item) => (
              <DirectoryRow
                entry={item.entry}
                expanded={item.expanded}
                loading={item.loading}
                depth={item.depth}
                key={item.entry.path}
                selected={selectedEntry?.path === item.entry.path}
                onActivate={() => item.entry.isDirectory
                  ? void toggle(port, item.entry)
                  : select(item.entry)}
              />
            ))}
          </div>
        )}
        {root?.isTruncated && <p className="project-directory__truncated">目录超过 5,000 项，只显示前 5,000 项。</p>}
      </aside>
      <section className="project-directory__detail">
        {selectedEntry ? (
          <EntryDetail entry={selectedEntry} />
        ) : (
          <div className="project-directory__welcome">
            <span aria-hidden="true">▱</span>
            <h2>选择项目文件</h2>
            <p>目录树来自本机 Registry 与 workspace grant；绝对路径不会发送到 React。</p>
          </div>
        )}
      </section>
    </div>
  );
}

function DirectoryRow({
  entry,
  depth,
  expanded,
  loading,
  selected,
  onActivate,
}: {
  entry: ProjectFileEntry;
  depth: number;
  expanded: boolean;
  loading: boolean;
  selected: boolean;
  onActivate(): void;
}) {
  return (
    <button
      aria-expanded={entry.isDirectory ? expanded : undefined}
      aria-selected={selected}
      className={`project-directory-row ${selected ? "project-directory-row--selected" : ""}`}
      onClick={onActivate}
      role="treeitem"
      style={{ paddingLeft: `${10 + depth * 16}px` }}
      type="button"
    >
      <span className="project-directory-row__disclosure" aria-hidden="true">
        {loading ? <span className="mini-spinner" /> : entry.isDirectory ? (expanded ? "⌄" : "›") : ""}
      </span>
      <span className="project-directory-row__icon" aria-hidden="true">{entry.isDirectory ? "▰" : "▤"}</span>
      <span>{entry.name}</span>
    </button>
  );
}

function EntryDetail({ entry }: { entry: ProjectFileEntry }) {
  return (
    <div className="project-directory-detail-card">
      <span className="project-directory-detail-card__icon" aria-hidden="true">{entry.isDirectory ? "▰" : "▤"}</span>
      <h2>{entry.name}</h2>
      <code>{displayProjectPath(entry.path)}</code>
      <dl>
        <div><dt>类型</dt><dd>{entry.isDirectory ? "文件夹" : "文件"}</dd></div>
        <div><dt>访问</dt><dd>{entry.isWritable ? "可写" : "只读"}</dd></div>
        {!entry.isDirectory && <div><dt>大小</dt><dd>{formatBytes(entry.size ?? 0)}</dd></div>}
        <div><dt>修改时间</dt><dd>{formatTime(entry.modifiedAtUnixMs)}</dd></div>
      </dl>
      {!entry.isDirectory && <p>文件预览与编辑将在后续独立切片通过内容边界验收后开放。</p>}
    </div>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_024 * 1_024) return `${(bytes / 1_024).toFixed(1)} KB`;
  return `${(bytes / (1_024 * 1_024)).toFixed(1)} MB`;
}

function formatTime(value: number | null) {
  if (value == null) return "—";
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(value);
}
