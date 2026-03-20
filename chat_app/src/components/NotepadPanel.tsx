import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { NotepadContextMenu, type ContextMenuState, type ContextMenuTarget } from './notepad/NotepadContextMenu';
import { NotepadEditor, type NotepadViewMode } from './notepad/NotepadEditor';
import { NotepadSidebar } from './notepad/NotepadSidebar';
import {
  buildFolderTree,
  normalizeFolderPath,
  parseTags,
  sanitizeFileName,
  type NoteMeta,
} from './notepad/utils';

interface NotepadPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

const NotepadPanel: React.FC<NotepadPanelProps> = ({ isOpen, onClose }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const [folders, setFolders] = useState<string[]>([]);
  const [notes, setNotes] = useState<NoteMeta[]>([]);
  const [selectedFolder, setSelectedFolder] = useState<string>('');
  const [selectedNoteId, setSelectedNoteId] = useState<string>('');
  const [title, setTitle] = useState('');
  const [tagsText, setTagsText] = useState('');
  const [content, setContent] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [viewMode, setViewMode] = useState<NotepadViewMode>('split');
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['']));
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  const availableFolders = useMemo(
    () => folders.filter((item) => item.trim().length > 0),
    [folders]
  );
  const folderTree = useMemo(
    () => buildFolderTree(availableFolders, notes),
    [availableFolders, notes]
  );
  const selectedNoteMeta = useMemo(
    () => notes.find((item) => item.id === selectedNoteId) || null,
    [notes, selectedNoteId]
  );

  const ensureFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      next.add('');
      if (!normalized) {
        return next;
      }
      let current = '';
      const parts = normalized.split('/').filter((item) => item.trim().length > 0);
      for (const part of parts) {
        current = current ? `${current}/${part}` : part;
        next.add(current);
      }
      return next;
    });
  }, []);

  const resetEditor = useCallback(() => {
    setSelectedNoteId('');
    setTitle('');
    setTagsText('');
    setContent('');
    setDirty(false);
  }, []);

  const loadFolders = useCallback(async () => {
    const res = await apiClient.listNotepadFolders();
    const list = Array.isArray(res?.folders) ? res.folders : [];
    const normalized = list
      .map((item: any) => String(item || '').trim())
      .filter((item: string) => item.length > 0);
    setFolders(['', ...normalized]);
  }, [apiClient]);

  const loadNotes = useCallback(async () => {
    const res = await apiClient.listNotepadNotes({
      recursive: true,
      query: searchQuery || undefined,
      limit: 500,
    });
    const list = Array.isArray(res?.notes) ? res.notes : [];
    setNotes(list);
  }, [apiClient, searchQuery]);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await apiClient.notepadInit();
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err: any) {
      setError(err?.message || '加载记事本失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadFolders, loadNotes]);

  const openNote = useCallback(async (id: string) => {
    if (!id) return;
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.getNotepadNote(id);
      const note = res?.note || {};
      const noteFolder = String(note.folder || '');
      setSelectedNoteId(String(note.id || id));
      setSelectedFolder(noteFolder);
      setTitle(String(note.title || ''));
      setTagsText(Array.isArray(note.tags) ? note.tags.join(', ') : '');
      setContent(String(res?.content || ''));
      ensureFolderExpanded(noteFolder);
      setDirty(false);
    } catch (err: any) {
      setError(err?.message || '打开笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded]);

  useEffect(() => {
    if (!isOpen) return;
    void refreshAll();
  }, [isOpen, refreshAll]);

  useEffect(() => {
    if (!isOpen) return;
    void loadNotes();
  }, [isOpen, loadNotes]);

  useEffect(() => {
    if (!selectedFolder) return;
    ensureFolderExpanded(selectedFolder);
  }, [ensureFolderExpanded, selectedFolder]);

  useEffect(() => {
    if (!contextMenu) return undefined;

    const closeMenu = () => setContextMenu(null);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeMenu();
      }
    };

    window.addEventListener('click', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.addEventListener('scroll', closeMenu, true);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('click', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.removeEventListener('scroll', closeMenu, true);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [contextMenu]);

  const toggleFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(normalized)) {
        next.delete(normalized);
      } else {
        next.add(normalized);
      }
      return next;
    });
  }, []);

  const handleCreateFolder = useCallback(async (parentFolder?: string) => {
    const baseFolder = normalizeFolderPath(parentFolder ?? selectedFolder);
    const promptTitle = baseFolder
      ? `在目录 "${baseFolder}" 下新建子目录（支持输入相对路径）`
      : '请输入新文件夹路径（例如 work/ideas）';
    const raw = window.prompt(promptTitle, '');
    if (raw === null) return;
    const input = normalizeFolderPath(raw);
    if (!input) return;

    const folder = baseFolder && !input.startsWith(`${baseFolder}/`) && input !== baseFolder
      ? `${baseFolder}/${input}`
      : input;
    setLoading(true);
    setError(null);
    try {
      await apiClient.createNotepadFolder({
        folder,
      });
      setSelectedFolder(folder);
      ensureFolderExpanded(folder);
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err: any) {
      setError(err?.message || '创建文件夹失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadFolders, loadNotes, selectedFolder]);

  const handleCreateNote = useCallback(async (folderOverride?: string) => {
    const targetFolder = normalizeFolderPath(folderOverride ?? selectedFolder);
    const noteTitle = window.prompt('请输入笔记标题', '');
    if (noteTitle === null) return;
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.createNotepadNote({
        folder: targetFolder,
        title: noteTitle.trim(),
      });
      if (targetFolder) {
        setSelectedFolder(targetFolder);
        ensureFolderExpanded(targetFolder);
      }
      await loadNotes();
      const id = String(res?.note?.id || '');
      if (id) {
        await openNote(id);
      } else {
        resetEditor();
      }
    } catch (err: any) {
      setError(err?.message || '创建笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadNotes, openNote, resetEditor, selectedFolder]);

  const handleSaveNote = useCallback(async () => {
    if (!selectedNoteId) return;
    setLoading(true);
    setError(null);
    try {
      await apiClient.updateNotepadNote(selectedNoteId, {
        title: title.trim(),
        content,
        tags: parseTags(tagsText),
      });
      setDirty(false);
      await loadNotes();
    } catch (err: any) {
      setError(err?.message || '保存笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, content, loadNotes, selectedNoteId, tagsText, title]);

  const resolveNotePayload = useCallback(async (note?: NoteMeta | null): Promise<{ title: string; content: string }> => {
    const targetNoteId = String(note?.id || selectedNoteId || '').trim();
    const fallbackTitle = String(note?.title || title || 'Untitled').trim() || 'Untitled';

    if (targetNoteId && targetNoteId === selectedNoteId) {
      return {
        title: title.trim() || fallbackTitle,
        content,
      };
    }

    if (!targetNoteId) {
      return {
        title: fallbackTitle,
        content,
      };
    }

    const res = await apiClient.getNotepadNote(targetNoteId);
    const remoteNote = res?.note || {};
    return {
      title: String(remoteNote.title || fallbackTitle || 'Untitled'),
      content: String(res?.content || ''),
    };
  }, [apiClient, content, selectedNoteId, title]);

  const copyTextToClipboard = useCallback(async (text: string) => {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }

    if (typeof document !== 'undefined') {
      const textarea = document.createElement('textarea');
      textarea.value = text;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      return;
    }

    throw new Error('clipboard is not available');
  }, []);

  const handleCopyText = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      await copyTextToClipboard(payload.content || '');
    } catch (err: any) {
      setError(err?.message || '复制文本失败');
    }
  }, [copyTextToClipboard, resolveNotePayload]);

  const handleCopyAsMdFile = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      if (typeof document === 'undefined') {
        throw new Error('document is not available');
      }
      const filename = `${sanitizeFileName(payload.title)}.md`;
      const blob = new Blob([payload.content || ''], { type: 'text/markdown;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename;
      anchor.style.display = 'none';
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);
      URL.revokeObjectURL(url);
    } catch (err: any) {
      setError(err?.message || '导出 .md 失败');
    }
  }, [resolveNotePayload]);

  const handleDeleteNoteById = useCallback(async (noteId: string, titleHint?: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) return;
    const confirmed = window.confirm(`确认删除笔记“${titleHint || '当前笔记'}”？此操作不可恢复。`);
    if (!confirmed) return;

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadNote(normalizedId);
      if (selectedNoteId === normalizedId) {
        resetEditor();
      }
      await loadNotes();
    } catch (err: any) {
      setError(err?.message || '删除笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadNotes, resetEditor, selectedNoteId]);

  const handleDeleteNote = useCallback(async () => {
    if (!selectedNoteId) return;
    const target = notes.find((item) => item.id === selectedNoteId);
    await handleDeleteNoteById(selectedNoteId, target?.title || undefined);
  }, [handleDeleteNoteById, notes, selectedNoteId]);

  const handleDeleteFolder = useCallback(async (folderPath?: string) => {
    const folder = normalizeFolderPath(folderPath ?? selectedFolder);
    if (!folder) {
      return;
    }

    const confirmed = window.confirm(`确认删除目录“${folder}”吗？会同时删除该目录下所有笔记。`);
    if (!confirmed) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadFolder({
        folder,
        recursive: true,
      });

      if (selectedFolder === folder || selectedFolder.startsWith(`${folder}/`)) {
        setSelectedFolder('');
      }

      const selectedNote = notes.find((item) => item.id === selectedNoteId);
      const selectedNoteFolder = normalizeFolderPath(selectedNote?.folder);
      if (selectedNote && (selectedNoteFolder === folder || selectedNoteFolder.startsWith(`${folder}/`))) {
        resetEditor();
      }

      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err: any) {
      setError(err?.message || '删除目录失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadFolders, loadNotes, notes, resetEditor, selectedFolder, selectedNoteId]);

  const openContextMenu = useCallback((event: React.MouseEvent, target: ContextMenuTarget) => {
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      target,
    });
  }, []);

  const handleTreeSelectFolder = useCallback((folderPath: string) => {
    setSelectedFolder(folderPath);
    ensureFolderExpanded(folderPath);
  }, [ensureFolderExpanded]);

  const handleTreeOpenNote = useCallback((noteId: string) => {
    void openNote(noteId);
  }, [openNote]);

  const handleTreeFolderContextMenu = useCallback((event: React.MouseEvent, folderPath: string) => {
    setSelectedFolder(folderPath);
    ensureFolderExpanded(folderPath);
    openContextMenu(event, { type: 'folder', folderPath });
  }, [ensureFolderExpanded, openContextMenu]);

  const handleTreeNoteContextMenu = useCallback((event: React.MouseEvent, note: NoteMeta) => {
    openContextMenu(event, { type: 'note', note });
  }, [openContextMenu]);

  const handleContextCreateFolder = useCallback(async () => {
    if (!contextMenu) return;
    const baseFolder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateFolder(baseFolder);
  }, [contextMenu, handleCreateFolder]);

  const handleContextCreateNote = useCallback(async () => {
    if (!contextMenu) return;
    const folder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateNote(folder);
  }, [contextMenu, handleCreateNote]);

  const handleContextDelete = useCallback(async () => {
    if (!contextMenu) return;
    const target = contextMenu.target;
    setContextMenu(null);
    if (target.type === 'folder') {
      await handleDeleteFolder(target.folderPath);
      return;
    }
    await handleDeleteNoteById(target.note.id, target.note.title || undefined);
  }, [contextMenu, handleDeleteFolder, handleDeleteNoteById]);

  const handleContextDeleteSelectedNote = useCallback(async () => {
    if (!selectedNoteMeta) return;
    setContextMenu(null);
    await handleDeleteNoteById(selectedNoteMeta.id, selectedNoteMeta.title || undefined);
  }, [handleDeleteNoteById, selectedNoteMeta]);

  const handleContextCopyText = useCallback(async () => {
    if (!contextMenu) return;
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyText(targetNote);
  }, [contextMenu, handleCopyText, selectedNoteMeta]);

  const handleContextCopyAsMd = useCallback(async () => {
    if (!contextMenu) return;
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyAsMdFile(targetNote);
  }, [contextMenu, handleCopyAsMdFile, selectedNoteMeta]);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) {
      return undefined;
    }
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 190 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    };
  }, [contextMenu]);

  if (!isOpen) return null;

  return (
    <>
      <div className="fixed inset-0 bg-black/50 z-40" onClick={onClose} />
      <div className="fixed inset-x-10 top-10 bottom-10 bg-card z-50 rounded-lg border border-border shadow-xl flex overflow-hidden">
        <NotepadSidebar
          onClose={onClose}
          onCreateFolder={() => { void handleCreateFolder(); }}
          onCreateNote={() => { void handleCreateNote(); }}
          searchQuery={searchQuery}
          onSearchQueryChange={setSearchQuery}
          selectedFolder={selectedFolder}
          loading={loading}
          notesCount={notes.length}
          availableFoldersCount={availableFolders.length}
          folderTree={folderTree}
          selectedNoteId={selectedNoteId}
          expandedFolders={expandedFolders}
          onToggleFolderExpanded={toggleFolderExpanded}
          onSelectFolder={handleTreeSelectFolder}
          onOpenNote={handleTreeOpenNote}
          onFolderContextMenu={handleTreeFolderContextMenu}
          onNoteContextMenu={handleTreeNoteContextMenu}
        />
        <NotepadEditor
          selectedNoteId={selectedNoteId}
          viewMode={viewMode}
          onViewModeChange={setViewMode}
          onRefresh={() => { void refreshAll(); }}
          onCopyText={() => { void handleCopyText(selectedNoteMeta); }}
          onCopyAsMd={() => { void handleCopyAsMdFile(selectedNoteMeta); }}
          onSave={() => { void handleSaveNote(); }}
          onDelete={() => { void handleDeleteNote(); }}
          dirty={dirty}
          error={error}
          title={title}
          onTitleChange={(value) => {
            setTitle(value);
            setDirty(true);
          }}
          tagsText={tagsText}
          onTagsTextChange={(value) => {
            setTagsText(value);
            setDirty(true);
          }}
          content={content}
          onContentChange={(value) => {
            setContent(value);
            setDirty(true);
          }}
        />
      </div>

      <NotepadContextMenu
        contextMenu={contextMenu}
        contextMenuStyle={contextMenuStyle}
        selectedNoteMeta={selectedNoteMeta}
        onContextCreateFolder={() => { void handleContextCreateFolder(); }}
        onContextCreateNote={() => { void handleContextCreateNote(); }}
        onContextCopyText={() => { void handleContextCopyText(); }}
        onContextCopyAsMd={() => { void handleContextCopyAsMd(); }}
        onContextDelete={() => { void handleContextDelete(); }}
        onContextDeleteSelectedNote={() => { void handleContextDeleteSelectedNote(); }}
      />
    </>
  );
};

export default NotepadPanel;
