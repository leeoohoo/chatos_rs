import { useEffect, useState } from "react";
import { useProjectDirectoryStore } from "../application/projectDirectoryStore";
import {
  displayProjectPath,
  fileNameFromPath,
  flattenDirectoryTree,
  projectImageDataUrl,
  type ProjectContentMatch,
  type ProjectFileContent,
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
  const state = useProjectDirectoryStore();
  const [createOpen, setCreateOpen] = useState(false);
  const [createKind, setCreateKind] = useState<"file" | "directory">("file");
  const [createName, setCreateName] = useState("");
  const [deleteEntry, setDeleteEntry] = useState<ProjectFileEntry | null>(null);

  useEffect(() => {
    void state.activate(port, ownerUserId, projectId);
  }, [ownerUserId, port, projectId]); // Store actions are stable; project identity owns activation.

  useEffect(() => () => useProjectDirectoryStore.getState().reset(), [ownerUserId, projectId]);

  useEffect(() => {
    const query = state.searchText.trim();
    if (!query) return;
    const timer = window.setTimeout(() => {
      void useProjectDirectoryStore.getState().search(port, query);
    }, 280);
    return () => window.clearTimeout(timer);
  }, [port, state.searchText]);

  const visibleEntries = flattenDirectoryTree(state.listings, state.expandedPaths, state.loadingPaths);
  const root = state.listings[""];
  const searching = state.searchText.trim().length > 0;

  return (
    <div className="project-directory" aria-label="项目目录">
      <aside className="project-directory__sidebar">
        <header className="project-directory__toolbar">
          <label className="project-directory-search">
            <span aria-hidden="true">⌕</span>
            <input
              aria-label="搜索项目文件"
              onChange={(event) => state.setSearchText(event.target.value)}
              placeholder="搜索项目文件"
              type="search"
              value={state.searchText}
            />
          </label>
          <button
            aria-label="新建项目文件或文件夹"
            disabled={!root || state.mutationKind != null}
            onClick={() => setCreateOpen(true)}
            title="新建文件或文件夹"
            type="button"
          >＋</button>
          <button disabled={state.rootLoading || state.mutationKind != null} onClick={() => void state.refresh(port)} title="刷新目录" type="button">↻</button>
        </header>
        {state.error && (
          <div className="project-directory__error" role="alert">
            <span>{state.error}</span>
            <button onClick={() => void state.refresh(port)} type="button">重试</button>
          </div>
        )}
        {searching ? (
          <SearchResults port={port} />
        ) : state.rootLoading && !root ? (
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
                selected={state.selectedEntry?.path === item.entry.path}
                onActivate={() => void state.toggle(port, item.entry)}
              />
            ))}
          </div>
        )}
        {!searching && root?.isTruncated && <p className="project-directory__truncated">目录超过 5,000 项，只显示前 5,000 项。</p>}
      </aside>
      <section className="project-directory__detail">
        {state.mutationError && <div className="project-directory-mutation-error" role="alert">{state.mutationError}</div>}
        <div className="project-directory__detail-content">
          <FilePreview onRequestDelete={setDeleteEntry} port={port} />
        </div>
      </section>
      {createOpen && (
        <CreateEntrySheet
          kind={createKind}
          mutationKind={state.mutationKind}
          name={createName}
          parentPath={creationParentPath(state.selectedEntry)}
          onCancel={() => {
            if (state.mutationKind) return;
            setCreateOpen(false);
            setCreateName("");
          }}
          onKindChange={setCreateKind}
          onNameChange={setCreateName}
          onSubmit={() => {
            const name = createName.trim();
            if (!name || state.mutationKind) return;
            setCreateOpen(false);
            setCreateName("");
            void state.createEntry(port, name, createKind);
          }}
        />
      )}
      {deleteEntry && (
        <DeleteEntryDialog
          entry={deleteEntry}
          pending={state.mutationKind === "delete"}
          onCancel={() => {
            if (state.mutationKind !== "delete") setDeleteEntry(null);
          }}
          onConfirm={() => {
            setDeleteEntry(null);
            void state.deleteSelected(port);
          }}
        />
      )}
    </div>
  );
}

function SearchResults({ port }: { port: ProjectDirectoryPort }) {
  const snapshot = useProjectDirectoryStore((state) => state.searchSnapshot);
  const loading = useProjectDirectoryStore((state) => state.searchLoading);
  const error = useProjectDirectoryStore((state) => state.searchError);
  const query = useProjectDirectoryStore((state) => state.searchText.trim());
  const openEntry = useProjectDirectoryStore((state) => state.openSearchEntry);
  const openMatch = useProjectDirectoryStore((state) => state.openContentMatch);
  const hasResults = (snapshot?.entries.length ?? 0) + (snapshot?.content.length ?? 0) > 0;
  return (
    <div className="project-directory-search-results" aria-label="项目搜索结果">
      {error && <div className="project-directory-search-results__error" role="alert">{error}</div>}
      {snapshot?.content.length ? (
        <SearchSection title="文件内容">
          {snapshot.content.map((match) => (
            <ContentMatchRow key={`${match.path}:${match.line}:${match.column}`} match={match} onOpen={() => void openMatch(port, match)} />
          ))}
        </SearchSection>
      ) : null}
      {snapshot?.entries.length ? (
        <SearchSection title="文件和目录">
          {snapshot.entries.map((entry) => (
            <button className="project-search-row" key={entry.path} onClick={() => void openEntry(port, entry)} type="button">
              <span aria-hidden="true">{entry.isDirectory ? "▰" : "▤"}</span>
              <span><strong>{entry.name}</strong><small>{displayProjectPath(entry.path)}</small></span>
            </button>
          ))}
        </SearchSection>
      ) : null}
      {snapshot && (snapshot.entryTruncated || snapshot.contentTruncated) && (
        <p className="project-directory__truncated">搜索达到 macOS 同源的时间、访问量或结果上限，结果可能不完整。</p>
      )}
      {loading ? (
        <div className="project-directory-search-results__status"><span className="mini-spinner" aria-hidden="true" />正在搜索文件内容…</div>
      ) : !hasResults && !error ? (
        <div className="project-directory__empty">没有找到“{query}”</div>
      ) : null}
    </div>
  );
}

function SearchSection({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="project-search-section"><h3>{title}</h3>{children}</section>;
}

function ContentMatchRow({ match, onOpen }: { match: ProjectContentMatch; onOpen(): void }) {
  return (
    <button className="project-search-row project-search-row--content" onClick={onOpen} type="button">
      <span aria-hidden="true">⌕</span>
      <span>
        <strong>{fileNameFromPath(match.path)} <em>第 {match.line} 行</em></strong>
        <code>{match.text.trim()}</code>
        <small>{displayProjectPath(match.path)}</small>
      </span>
    </button>
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

function FilePreview({ port, onRequestDelete }: { port: ProjectDirectoryPort; onRequestDelete(entry: ProjectFileEntry): void }) {
  const entry = useProjectDirectoryStore((state) => state.selectedEntry);
  const file = useProjectDirectoryStore((state) => state.selectedFile);
  const targetLine = useProjectDirectoryStore((state) => state.selectedLine);
  const loading = useProjectDirectoryStore((state) => state.previewLoading);
  const error = useProjectDirectoryStore((state) => state.previewError);
  const openFile = useProjectDirectoryStore((state) => state.openFile);
  if (!entry) {
    return <Welcome icon="▱" title="选择一个文件">从左侧项目目录中选择文件以预览；绝对路径不会发送到 React。</Welcome>;
  }
  if (entry.isDirectory) return <EntryDetail entry={entry} onRequestDelete={() => onRequestDelete(entry)} />;
  if (loading) return <Welcome icon={<span className="mini-spinner" />} title="正在读取文件">{displayProjectPath(entry.path)}</Welcome>;
  if (error) {
    return (
      <Welcome icon="!" title="无法预览文件">
        <span>{error}</span>
        <button className="project-file-preview__retry" onClick={() => void openFile(port, entry, targetLine ?? undefined)} type="button">重试</button>
      </Welcome>
    );
  }
  if (!file) return <EntryDetail entry={entry} onRequestDelete={() => onRequestDelete(entry)} />;
  return <FileContent file={file} onRequestDelete={() => onRequestDelete(entry)} port={port} targetLine={targetLine} />;
}

function FileContent({
  file,
  targetLine,
  port,
  onRequestDelete,
}: {
  file: ProjectFileContent;
  targetLine: number | null;
  port: ProjectDirectoryPort;
  onRequestDelete(): void;
}) {
  const imageUrl = projectImageDataUrl(file);
  const draft = useProjectDirectoryStore((state) => state.draft);
  const isEditing = useProjectDirectoryStore((state) => state.isEditing);
  const mutationKind = useProjectDirectoryStore((state) => state.mutationKind);
  const beginEditing = useProjectDirectoryStore((state) => state.beginEditing);
  const setDraft = useProjectDirectoryStore((state) => state.setDraft);
  const cancelEditing = useProjectDirectoryStore((state) => state.cancelEditing);
  const save = useProjectDirectoryStore((state) => state.save);
  return (
    <article className="project-file-preview">
      <header>
        <div><strong>{file.name}</strong><code>{displayProjectPath(file.path)}</code></div>
        <div className="project-file-preview__actions">
          <span>{formatBytes(file.size)} · {file.isWritable ? "可写" : "只读"}</span>
          {isEditing ? (
            <>
              <button disabled={mutationKind != null} onClick={cancelEditing} type="button">取消</button>
              <button className="primary-button" disabled={mutationKind != null} onClick={() => void save(port)} type="button">
                {mutationKind === "save" ? <><span className="mini-spinner" aria-hidden="true" />保存中…</> : "保存"}
              </button>
            </>
          ) : (
            <>
              <button className="danger-link" disabled={mutationKind != null} onClick={onRequestDelete} type="button">删除</button>
              {file.isWritable && !file.isBinary && (
                <button className="primary-button" disabled={mutationKind != null} onClick={beginEditing} type="button">编辑</button>
              )}
            </>
          )}
        </div>
      </header>
      {isEditing ? (
        <textarea
          aria-label={`${file.name} 编辑内容`}
          className="project-file-editor"
          onChange={(event) => setDraft(event.target.value)}
          spellCheck={false}
          value={draft}
        />
      ) : imageUrl ? (
        <div className="project-file-preview__image"><img alt={file.name} src={imageUrl} /></div>
      ) : file.isBinary ? (
        <Welcome icon="▤" title="二进制文件">此文件不能在文本预览器中显示。</Welcome>
      ) : (
        <pre className="project-file-preview__text" aria-label={`${file.name} 文件内容`}>
          <code>{targetLine ? highlightedContent(file.content, targetLine) : file.content}</code>
        </pre>
      )}
    </article>
  );
}

function highlightedContent(content: string, targetLine: number) {
  return content.split("\n").map((line, index, lines) => (
    <span className={index + 1 === targetLine ? "is-target-line" : undefined} key={index}>
      {line}{index < lines.length - 1 ? "\n" : ""}
    </span>
  ));
}

function Welcome({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <div className="project-directory__welcome">
      <span aria-hidden="true">{icon}</span><h2>{title}</h2><p>{children}</p>
    </div>
  );
}

function EntryDetail({ entry, onRequestDelete }: { entry: ProjectFileEntry; onRequestDelete(): void }) {
  const mutationKind = useProjectDirectoryStore((state) => state.mutationKind);
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
      <button className="project-entry-delete" disabled={mutationKind != null} onClick={onRequestDelete} type="button">删除{entry.isDirectory ? "文件夹" : "文件"}</button>
    </div>
  );
}

function CreateEntrySheet({
  kind,
  name,
  parentPath,
  mutationKind,
  onKindChange,
  onNameChange,
  onCancel,
  onSubmit,
}: {
  kind: "file" | "directory";
  name: string;
  parentPath: string;
  mutationKind: "save" | "create" | "delete" | null;
  onKindChange(kind: "file" | "directory"): void;
  onNameChange(name: string): void;
  onCancel(): void;
  onSubmit(): void;
}) {
  return (
    <div className="project-entry-sheet-backdrop">
      <section
        aria-labelledby="create-project-entry-title"
        aria-modal="true"
        className="project-entry-sheet"
        onKeyDown={(event) => { if (event.key === "Escape") onCancel(); }}
        role="dialog"
      >
        <span className="project-entry-sheet__icon" aria-hidden="true">＋</span>
        <h2 id="create-project-entry-title">新建项目内容</h2>
        <p>将在 <code>{displayProjectPath(parentPath)}</code> 中创建，路径由本机 Host 安全校验。</p>
        <div aria-label="创建类型" className="project-entry-kind-picker" role="group">
          <button aria-pressed={kind === "file"} className={kind === "file" ? "is-selected" : ""} onClick={() => onKindChange("file")} type="button">文件</button>
          <button aria-pressed={kind === "directory"} className={kind === "directory" ? "is-selected" : ""} onClick={() => onKindChange("directory")} type="button">文件夹</button>
        </div>
        <label className="project-entry-name-field">
          <span>{kind === "file" ? "文件名称" : "文件夹名称"}</span>
          <input
            autoFocus
            disabled={mutationKind != null}
            maxLength={1_024}
            onChange={(event) => onNameChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") onSubmit();
              if (event.key === "Escape") onCancel();
            }}
            placeholder={kind === "file" ? "例如 README.md" : "例如 Sources"}
            value={name}
          />
        </label>
        <footer>
          <button disabled={mutationKind != null} onClick={onCancel} type="button">取消</button>
          <button className="primary-button" disabled={!name.trim() || mutationKind != null} onClick={onSubmit} type="button">创建</button>
        </footer>
      </section>
    </div>
  );
}

function DeleteEntryDialog({
  entry,
  pending,
  onCancel,
  onConfirm,
}: {
  entry: ProjectFileEntry;
  pending: boolean;
  onCancel(): void;
  onConfirm(): void;
}) {
  return (
    <div className="project-entry-sheet-backdrop">
      <section
        aria-labelledby="delete-project-entry-title"
        aria-modal="true"
        className="project-entry-dialog"
        onKeyDown={(event) => { if (event.key === "Escape") onCancel(); }}
        role="alertdialog"
      >
        <span className="project-entry-dialog__icon" aria-hidden="true">!</span>
        <h2 id="delete-project-entry-title">删除“{entry.name}”？</h2>
        <p>{entry.isDirectory ? "文件夹及其中全部内容将被删除。" : "文件将从本机项目目录中删除。"}此操作无法撤销。</p>
        <div>
          <button autoFocus disabled={pending} onClick={onCancel} type="button">取消</button>
          <button className="danger-button" disabled={pending} onClick={onConfirm} type="button">{pending ? "删除中…" : "删除"}</button>
        </div>
      </section>
    </div>
  );
}

function creationParentPath(entry: ProjectFileEntry | null) {
  if (!entry) return "";
  if (entry.isDirectory) return entry.path;
  const separator = entry.path.lastIndexOf("/");
  return separator < 0 ? "" : entry.path.slice(0, separator);
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
