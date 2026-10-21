import { useEffect, useMemo, useRef, useState } from "react";
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
}

export function NotepadView({ hostReady, ownerUserId, port }: NotepadViewProps) {
  const state = useNotepadStore();
  const {
    activate, clearError, createFolder, createNote, deleteFolder, deleteSelectedNote,
    insertImage, refresh, renameFolder, reset, save, selectFolder, selectNote, setEditorMode,
    setSearchQuery, syncExternalChanges, updateDraft,
  } = state;
  const [tagsText, setTagsText] = useState("");
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const tree = useMemo(() => buildNotepadTree(state.folders, state.notes), [state.folders, state.notes]);
  const dirty = isNotepadDirty(state);
  const busy = state.isLoading || state.isLoadingNote || state.isSaving || state.isMutating;

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
    if (!dirty || state.isSaving || state.isMutating) return undefined;
    const timer = window.setTimeout(() => void save(port), 1_200);
    return () => window.clearTimeout(timer);
  }, [dirty, port, save, state.draft, state.isMutating, state.isSaving]);

  useEffect(() => {
    if (!dirty) return undefined;
    const beforeUnload = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", beforeUnload);
    return () => window.removeEventListener("beforeunload", beforeUnload);
  }, [dirty]);

  if (!hostReady) {
    return <NotepadState icon="⌁" title="正在连接本机服务" detail="连接就绪后会自动读取本机记事本。" />;
  }

  const promptCreateFolder = () => {
    const name = window.prompt("新建文件夹名称");
    if (!name?.trim()) return;
    const folder = state.selectedFolder ? `${state.selectedFolder}/${name}` : name;
    void createFolder(port, folder);
  };
  const promptCreateNote = () => {
    const title = window.prompt("新建笔记标题", "未命名笔记");
    if (title == null) return;
    void createNote(port, title);
  };
  const promptRenameFolder = () => {
    if (!state.selectedFolder) return;
    const value = window.prompt("重命名文件夹", state.selectedFolder);
    if (!value?.trim() || value.trim() === state.selectedFolder) return;
    void renameFolder(port, state.selectedFolder, value);
  };
  const confirmDeleteFolder = () => {
    if (!state.selectedFolder || !window.confirm(`删除“${state.selectedFolder}”及其中全部笔记？此操作不可撤销。`)) return;
    void deleteFolder(port, state.selectedFolder);
  };
  const confirmDeleteNote = () => {
    if (!state.selectedNoteId || !window.confirm("删除当前笔记？此操作不可撤销。")) return;
    void deleteSelectedNote(port);
  };

  return (
    <section aria-label="记事本" className="notepad-workspace">
      <header className="notepad-header">
        <div className="notepad-header__icon" aria-hidden="true">▤</div>
        <div><p>LOCAL NOTEBOOK</p><h2>记事本</h2><span>文件夹、Markdown 与标签均保存在本机 Host</span></div>
        {busy && <span className="mini-spinner" aria-label="正在处理" />}
        <button aria-label="刷新记事本" disabled={busy || dirty} onClick={() => void refresh(port)} type="button">↻</button>
      </header>
      <div className="notepad-split">
        <aside className="notepad-sidebar">
          <div className="notepad-search"><span aria-hidden="true">⌕</span><input aria-label="搜索标题或文件夹" onChange={(event) => setSearchQuery(event.target.value)} placeholder="搜索标题或文件夹" type="search" value={state.searchQuery} /></div>
          <div className="notepad-folder-actions">
            <span title={state.selectedFolder || "根目录"}>{state.selectedFolder || "根目录"}</span>
            <button aria-label="新建文件夹" disabled={busy} onClick={promptCreateFolder} type="button">＋▱</button>
            <button aria-label="新建笔记" disabled={busy} onClick={promptCreateNote} type="button">＋</button>
          </div>
          {state.selectedFolder && (
            <div className="notepad-folder-menu">
              <button disabled={busy} onClick={promptRenameFolder} type="button">重命名目录</button>
              <button className="is-danger" disabled={busy} onClick={confirmDeleteFolder} type="button">删除目录</button>
            </div>
          )}
          <div className="notepad-tree" role="tree">
            <TreeRow
              active={state.selectedNoteId == null && state.selectedFolder === ""}
              row={{ id: "folder:", kind: "folder", depth: 0, title: "根目录", folder: "", note: null }}
              onFolder={selectFolder}
              onNote={(noteId) => void selectNote(port, noteId)}
            />
            {tree.map((row) => <TreeRow active={row.kind === "folder" ? state.selectedNoteId == null && state.selectedFolder === row.folder : state.selectedNoteId === row.note?.noteId} key={row.id} onFolder={selectFolder} onNote={(noteId) => void selectNote(port, noteId)} row={row} />)}
            {!state.isLoading && tree.length === 0 && <div className="notepad-tree-empty"><span aria-hidden="true">▤</span><p>暂无笔记</p><small>新建文件夹或笔记开始记录</small></div>}
          </div>
        </aside>
        <main className="notepad-editor">
          {state.error && <div className="notepad-error" role="alert"><span aria-hidden="true">!</span><p>{state.error}</p><button aria-label="关闭错误" onClick={clearError} type="button">×</button></div>}
          {state.selectedNoteId ? (
            <>
              <div className="notepad-editor-toolbar">
                <div><strong>{dirty ? "尚未保存" : "已保存"}</strong><small>版本 {state.notes.find((note) => note.noteId === state.selectedNoteId)?.version ?? "—"}</small></div>
                <ModePicker mode={state.editorMode} onChange={setEditorMode} />
                <button className="notepad-image" disabled={busy} onClick={() => void insertImage(port, "picker", editorRef.current?.selectionStart)} type="button">插入图片</button>
                <button className="notepad-save" disabled={!dirty || busy} onClick={() => void save(port)} type="button">{state.isSaving ? "保存中…" : "保存"}</button>
                <button className="notepad-delete" disabled={busy} onClick={confirmDeleteNote} type="button">删除</button>
              </div>
              <div className="notepad-fields">
                <input aria-label="笔记标题" disabled={busy} maxLength={1_000} onChange={(event) => updateDraft({ title: event.target.value })} placeholder="笔记标题" value={state.draft.title} />
                <input aria-label="笔记文件夹" disabled={busy} maxLength={1_000} onChange={(event) => updateDraft({ folder: event.target.value })} placeholder="根目录" value={state.draft.folder} />
                <input aria-label="笔记标签" disabled={busy} maxLength={8_256} onBlur={() => updateDraft({ tags: parseNotepadTags(tagsText) })} onChange={(event) => setTagsText(event.target.value)} placeholder="标签，以逗号分隔" value={tagsText} />
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
            </>
          ) : (
            <NotepadState icon="▤" title="选择一条笔记" detail="从左侧打开笔记，或在当前目录中新建一条笔记。" />
          )}
        </main>
      </div>
    </section>
  );
}

function TreeRow({ row, active, onFolder, onNote }: { row: NotepadTreeRow; active: boolean; onFolder(folder: string): void; onNote(noteId: string): void }) {
  return (
    <button aria-selected={active} className={`notepad-tree-row ${active ? "is-active" : ""}`} onClick={() => row.kind === "folder" ? onFolder(row.folder) : onNote(row.note!.noteId)} role="treeitem" style={{ paddingLeft: 10 + row.depth * 15 }} type="button">
      <span aria-hidden="true">{row.kind === "folder" ? "▱" : "▤"}</span>
      <span><strong>{row.title}</strong>{row.note && <small>{formatNoteTime(row.note.updatedAtUnixMs)}</small>}</span>
    </button>
  );
}

function ModePicker({ mode, onChange }: { mode: NotepadEditorMode; onChange(mode: NotepadEditorMode): void }) {
  return <div aria-label="编辑器模式" className="notepad-mode-picker" role="group">{(["edit", "preview", "split"] as const).map((value) => <button aria-pressed={mode === value} className={mode === value ? "is-active" : ""} key={value} onClick={() => onChange(value)} type="button">{{ edit: "编辑", preview: "预览", split: "分栏" }[value]}</button>)}</div>;
}

function NotepadState({ icon, title, detail }: { icon: string; title: string; detail: string }) {
  return <div className="notepad-state"><span aria-hidden="true">{icon}</span><h2>{title}</h2><p>{detail}</p></div>;
}

function formatNoteTime(timestamp: number) {
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "short", timeStyle: "short" }).format(new Date(timestamp));
}
