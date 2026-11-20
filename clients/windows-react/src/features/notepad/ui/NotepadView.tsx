import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import {
  buildNotepadTree,
  parseNotepadTags,
  type NotepadEditorMode,
  type NotepadTreeRow,
} from "../domain/notepad";
import { isNotepadDirty, useNotepadStore } from "../application/notepadStore";
import type { NotepadPort } from "../ports/NotepadPort";
import { SafeMarkdownPreview } from "./SafeMarkdownPreview";
import "./notepad.css";

interface NotepadViewProps {
  hostReady: boolean;
  ownerUserId: string;
  port: NotepadPort;
  onClose(): void;
}

type ContextTarget = {
  row: NotepadTreeRow;
  x: number;
  y: number;
};

export function NotepadView({ hostReady, ownerUserId, port, onClose }: NotepadViewProps) {
  const state = useNotepadStore();
  const {
    activate, clearError, createFolder, createNote, deleteFolder, deleteSelectedNote,
    insertImage, refresh, renameFolder, reset, save, selectFolder, selectNote, setEditorMode,
    setSearchQuery, syncExternalChanges, updateDraft,
  } = state;
  const [tagsText, setTagsText] = useState("");
  const [actionError, setActionError] = useState<string | null>(null);
  const [contextTarget, setContextTarget] = useState<ContextTarget | null>(null);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const tree = useMemo(() => buildNotepadTree(state.folders, state.notes), [state.folders, state.notes]);
  const dirty = isNotepadDirty(state);
  const busy = state.isLoading || state.isLoadingNote || state.isSaving || state.isMutating;
  const visibleError = state.error ?? actionError;

  useEffect(() => {
    if (hostReady) void activate(port, ownerUserId);
    else reset();
    return reset;
  }, [activate, hostReady, ownerUserId, port, reset]);

  useEffect(() => {
    setTagsText(state.draft.tags.join(", "));
  }, [state.selectedNoteId, state.savedDraft.tags]);

  useEffect(() => {
    if (!hostReady) return undefined;
    const timer = window.setTimeout(() => void refresh(port, false), 260);
    return () => window.clearTimeout(timer);
  }, [hostReady, port, refresh, state.searchQuery]);

  useEffect(() => {
    if (!hostReady) return undefined;
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void syncExternalChanges(port);
    }, 300_000);
    return () => window.clearInterval(timer);
  }, [hostReady, port, syncExternalChanges]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || busy) return;
      if (contextTarget) {
        setContextTarget(null);
        return;
      }
      void close();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  });

  useEffect(() => {
    if (!dirty) return undefined;
    const beforeUnload = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [dirty]);

  const close = async () => {
    if (busy) return;
    if (dirty && !(await save(port))) return;
    onClose();
  };

  const promptCreateFolder = (parent = state.selectedFolder) => {
    const name = window.prompt(parent ? `在“${parent}”中新建子文件夹` : "在根目录下新建文件夹");
    if (!name?.trim()) return;
    const folder = parent ? `${parent}/${name}` : name;
    void createFolder(port, folder);
  };
  const promptCreateNote = (folder = state.selectedFolder) => {
    const title = window.prompt(folder ? `在“${folder}”中新建笔记` : "在根目录下新建笔记", "未命名笔记");
    if (title == null) return;
    void createNote(port, title, folder);
  };
  const promptRenameFolder = (folder = state.selectedFolder) => {
    if (!folder) return;
    const value = window.prompt("重命名文件夹", folder);
    if (!value?.trim() || value.trim() === folder) return;
    void renameFolder(port, folder, value);
  };
  const confirmDeleteFolder = (folder = state.selectedFolder) => {
    if (!folder || !window.confirm(`“${folder}”以及其中的全部笔记都会被删除，此操作无法撤销。`)) return;
    void deleteFolder(port, folder);
  };
  const confirmDeleteNote = () => {
    if (!state.selectedNoteId || !window.confirm(`删除“${state.draft.title.trim() || "未命名笔记"}”？此操作无法撤销。`)) return;
    void deleteSelectedNote(port);
  };
  const copy = async () => {
    try {
      await port.copyText(state.draft.content);
      setActionError(null);
    } catch (failure) {
      setActionError(errorMessage(failure));
    }
  };
  const exportMarkdown = async () => {
    try {
      await port.exportMarkdown(ownerUserId, state.draft.title, state.draft.content);
      setActionError(null);
    } catch (failure) {
      setActionError(errorMessage(failure));
    }
  };
  const openContextMenu = (event: MouseEvent, row: NotepadTreeRow) => {
    event.preventDefault();
    setContextTarget({ row, x: event.clientX, y: event.clientY });
  };

  const rootRow: NotepadTreeRow = {
    id: "folder:", kind: "folder", depth: 0, title: "根目录", folder: "", note: null,
  };

  return (
    <div className="notepad-sheet-backdrop" onMouseDown={() => setContextTarget(null)} role="presentation">
      <section aria-label="记事本" aria-modal="true" className="notepad-workspace" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header className="notepad-header">
          <div className="notepad-header__title"><span aria-hidden="true">▤</span><strong>记事本</strong>{busy && <span className="mini-spinner" aria-label="正在处理" />}</div>
          <div className="notepad-header__actions">
            <button disabled={busy || dirty} onClick={() => void refresh(port)} type="button"><span aria-hidden="true">↻</span>刷新</button>
            <button disabled={busy} onClick={() => void close()} type="button">关闭</button>
          </div>
        </header>
        <div className="notepad-split">
          <aside className="notepad-sidebar">
            <div className="notepad-search"><input autoFocus aria-label="搜索标题或文件夹" onChange={(event) => setSearchQuery(event.target.value)} placeholder="搜索标题或文件夹" type="search" value={state.searchQuery} /></div>
            <div className="notepad-current-folder">当前目录：{state.selectedFolder || "根目录"}</div>
            <div className="notepad-tree" role="tree">
              <TreeRow active={state.selectedNoteId == null && state.selectedFolder === ""} row={rootRow} onContextMenu={openContextMenu} onFolder={selectFolder} onNote={(noteId) => void selectNote(port, noteId)} />
              {tree.map((row) => <TreeRow active={row.kind === "folder" ? state.selectedNoteId == null && state.selectedFolder === row.folder : state.selectedNoteId === row.note?.noteId} key={row.id} onContextMenu={openContextMenu} onFolder={selectFolder} onNote={(noteId) => void selectNote(port, noteId)} row={row} />)}
              {!state.isLoading && tree.length === 0 && <div className="notepad-tree-empty"><span aria-hidden="true">▤</span><p>暂无笔记</p><small>创建文件夹或笔记开始记录。</small></div>}
            </div>
          </aside>
          <main className="notepad-editor">
            <div className="notepad-editor-toolbar">
              <strong>{state.selectedNoteId ? "编辑笔记" : "未选择笔记"}</strong>
              {dirty && <span className="notepad-unsaved">未保存</span>}
              <ModePicker disabled={!state.selectedNoteId} mode={state.editorMode} onChange={setEditorMode} />
              <button disabled={!state.selectedNoteId} onClick={() => void copy()} type="button"><span aria-hidden="true">▧</span>复制</button>
              <button disabled={!state.selectedNoteId} onClick={() => void exportMarkdown()} type="button"><span aria-hidden="true">⇩</span>导出</button>
              <button className="notepad-save" disabled={!dirty || busy} onClick={() => void save(port)} type="button"><span aria-hidden="true">⇩</span>{state.isSaving ? "保存中…" : "保存"}</button>
              <button className="notepad-delete" disabled={!state.selectedNoteId || busy} onClick={confirmDeleteNote} type="button"><span aria-hidden="true">♲</span>删除</button>
            </div>
            {visibleError && <div className="notepad-error" role="alert"><span aria-hidden="true">!</span><p>{visibleError}</p><button aria-label="关闭错误" onClick={() => { clearError(); setActionError(null); }} type="button">×</button></div>}
            {hostReady ? state.selectedNoteId ? (
              <div className="notepad-editor-content">
                <div className="notepad-fields">
                  <input aria-label="笔记标题" disabled={busy} maxLength={1_000} onChange={(event) => updateDraft({ title: event.target.value })} placeholder="标题" value={state.draft.title} />
                  <input aria-label="笔记标签" disabled={busy} maxLength={8_256} onBlur={() => updateDraft({ tags: parseNotepadTags(tagsText) })} onChange={(event) => setTagsText(event.target.value)} placeholder="标签（使用逗号分隔）" value={tagsText} />
                </div>
                <div className={`notepad-document is-${state.editorMode}`}>
                  {(state.editorMode === "edit" || state.editorMode === "split") && <textarea aria-label="Markdown 正文" disabled={state.isLoadingNote || state.isMutating} maxLength={512 * 1_024} onChange={(event) => updateDraft({ content: event.target.value })} onPaste={(event) => {
                    if (![...event.clipboardData.items].some((item) => item.type.startsWith("image/"))) return;
                    event.preventDefault();
                    void insertImage(port, "clipboard", event.currentTarget.selectionStart);
                  }} placeholder="# 开始记录…" ref={editorRef} spellCheck value={state.draft.content} />}
                  {(state.editorMode === "preview" || state.editorMode === "split") && <SafeMarkdownPreview markdown={state.draft.content} />}
                  {state.isLoadingNote && <div className="notepad-loading"><span className="mini-spinner" />正在读取笔记…</div>}
                </div>
              </div>
            ) : <NotepadState icon="▤" title="选择一条笔记" detail="从左侧打开笔记，或新建一条笔记。" /> : <NotepadState icon="⌁" title="正在连接本机服务" detail="连接就绪后会自动读取本机记事本。" />}
          </main>
        </div>
        {contextTarget && (
          <div className="notepad-context-menu" onMouseDown={(event) => event.stopPropagation()} role="menu" style={{ left: contextTarget.x, top: contextTarget.y }}>
            {contextTarget.row.kind === "folder" ? <>
              <button onClick={() => { setContextTarget(null); promptCreateFolder(contextTarget.row.folder); }} role="menuitem" type="button">新建子文件夹</button>
              <button onClick={() => { setContextTarget(null); promptCreateNote(contextTarget.row.folder); }} role="menuitem" type="button">新建笔记</button>
              {contextTarget.row.folder && <hr />}
              {contextTarget.row.folder && <button onClick={() => { setContextTarget(null); promptRenameFolder(contextTarget.row.folder); }} role="menuitem" type="button">重命名文件夹</button>}
              {contextTarget.row.folder && <button className="is-danger" onClick={() => { setContextTarget(null); confirmDeleteFolder(contextTarget.row.folder); }} role="menuitem" type="button">删除文件夹</button>}
            </> : <>
              <button disabled={contextTarget.row.note?.noteId !== state.selectedNoteId} onClick={() => { setContextTarget(null); void copy(); }} role="menuitem" type="button">复制文本</button>
              <button disabled={contextTarget.row.note?.noteId !== state.selectedNoteId} onClick={() => { setContextTarget(null); void exportMarkdown(); }} role="menuitem" type="button">导出 Markdown</button>
              <hr />
              <button className="is-danger" onClick={() => {
                const noteId = contextTarget.row.note!.noteId;
                const title = contextTarget.row.note!.title.trim() || "未命名笔记";
                setContextTarget(null);
                if (!window.confirm(`删除“${title}”？此操作无法撤销。`)) return;
                void selectNote(port, noteId).then((selected) => {
                  if (selected) void useNotepadStore.getState().deleteSelectedNote(port);
                });
              }} role="menuitem" type="button">删除笔记</button>
            </>}
          </div>
        )}
      </section>
    </div>
  );
}

function TreeRow({ row, active, onFolder, onNote, onContextMenu }: { row: NotepadTreeRow; active: boolean; onFolder(folder: string): void; onNote(noteId: string): void; onContextMenu(event: MouseEvent, row: NotepadTreeRow): void }) {
  return (
    <button aria-selected={active} className={`notepad-tree-row ${active ? "is-active" : ""}`} onClick={() => row.kind === "folder" ? onFolder(row.folder) : onNote(row.note!.noteId)} onContextMenu={(event) => onContextMenu(event, row)} role="treeitem" style={{ paddingLeft: 10 + row.depth * 15 }} type="button">
      <span aria-hidden="true">{row.kind === "folder" ? "▱" : "▤"}</span>
      <span><strong>{row.title}</strong>{row.note && <small>{formatNoteTime(row.note.updatedAtUnixMs)}</small>}</span>
    </button>
  );
}

function ModePicker({ mode, disabled, onChange }: { mode: NotepadEditorMode; disabled: boolean; onChange(mode: NotepadEditorMode): void }) {
  return <div aria-label="编辑器模式" className="notepad-mode-picker" role="group">{(["edit", "preview", "split"] as const).map((value) => <button aria-pressed={mode === value} className={mode === value ? "is-active" : ""} disabled={disabled} key={value} onClick={() => onChange(value)} type="button">{{ edit: "编辑", preview: "预览", split: "分栏" }[value]}</button>)}</div>;
}

function NotepadState({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  return <div className="notepad-state"><span aria-hidden="true">{icon}</span><h2>{title}</h2><p>{detail}</p></div>;
}

function formatNoteTime(timestamp: number) {
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "short", timeStyle: "short" }).format(new Date(timestamp));
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
