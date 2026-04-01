import { useCallback, useMemo, useState } from 'react';
import type React from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext } from '../../lib/store/ChatStoreContext';
import type { ContextMenuState, ContextMenuTarget } from './NotepadContextMenu';
import type { NotepadViewMode } from './NotepadEditor';
import {
  buildContextMenuStyle,
  copyTextToClipboard,
  downloadMarkdownFile,
  normalizeNoteMeta,
} from './controllerHelpers';
import { useNotepadPanelEffects } from './useNotepadPanelEffects';
import {
  buildFolderTree,
  normalizeFolderPath,
  parseTags,
  type NoteMeta,
} from './utils';

interface UseNotepadPanelControllerParams {
  isOpen: boolean;
}

interface UseNotepadPanelControllerResult {
  availableFoldersCount: number;
  contextMenu: ContextMenuState | null;
  contextMenuStyle: React.CSSProperties | undefined;
  content: string;
  dirty: boolean;
  error: string | null;
  expandedFolders: Set<string>;
  folderTree: ReturnType<typeof buildFolderTree>;
  loading: boolean;
  notesCount: number;
  searchQuery: string;
  selectedFolder: string;
  selectedNoteId: string;
  selectedNoteMeta: NoteMeta | null;
  tagsText: string;
  title: string;
  viewMode: NotepadViewMode;
  setSearchQuery: React.Dispatch<React.SetStateAction<string>>;
  setViewMode: React.Dispatch<React.SetStateAction<NotepadViewMode>>;
  handleRefresh: () => Promise<void>;
  handleCreateFolder: () => Promise<void>;
  handleCreateNote: () => Promise<void>;
  handleToggleFolderExpanded: (folderPath: string) => void;
  handleSelectFolder: (folderPath: string) => void;
  handleOpenNote: (noteId: string) => void;
  handleFolderContextMenu: (event: React.MouseEvent, folderPath: string) => void;
  handleNoteContextMenu: (event: React.MouseEvent, note: NoteMeta) => void;
  handleCopyText: () => Promise<void>;
  handleCopyAsMd: () => Promise<void>;
  handleSave: () => Promise<void>;
  handleDelete: () => Promise<void>;
  handleTitleChange: (value: string) => void;
  handleTagsTextChange: (value: string) => void;
  handleContentChange: (value: string) => void;
  handleContextCreateFolder: () => Promise<void>;
  handleContextCreateNote: () => Promise<void>;
  handleContextCopyText: () => Promise<void>;
  handleContextCopyAsMd: () => Promise<void>;
  handleContextDelete: () => Promise<void>;
  handleContextDeleteSelectedNote: () => Promise<void>;
}

export const useNotepadPanelController = ({
  isOpen,
}: UseNotepadPanelControllerParams): UseNotepadPanelControllerResult => {
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);

  const [folders, setFolders] = useState<string[]>([]);
  const [notes, setNotes] = useState<NoteMeta[]>([]);
  const [selectedFolder, setSelectedFolder] = useState('');
  const [selectedNoteId, setSelectedNoteId] = useState('');
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
      .map((item) => String(item || '').trim())
      .filter((item) => item.length > 0);
    setFolders(['', ...normalized]);
  }, [apiClient]);

  const loadNotes = useCallback(async () => {
    const res = await apiClient.listNotepadNotes({
      recursive: true,
      query: searchQuery || undefined,
      limit: 500,
    });
    const list = Array.isArray(res?.notes) ? res.notes : [];
    setNotes(list.map(normalizeNoteMeta));
  }, [apiClient, normalizeNoteMeta, searchQuery]);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await apiClient.notepadInit();
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载记事本失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadFolders, loadNotes]);

  const openNote = useCallback(async (id: string) => {
    if (!id) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await apiClient.getNotepadNote(id);
      const note = res?.note;
      const noteFolder = String(note?.folder || '');
      setSelectedNoteId(String(note?.id || id));
      setSelectedFolder(noteFolder);
      setTitle(String(note?.title || ''));
      setTagsText(Array.isArray(note?.tags) ? note.tags.join(', ') : '');
      setContent(String(res?.content || ''));
      ensureFolderExpanded(noteFolder);
      setDirty(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : '打开笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded]);

  useNotepadPanelEffects({
    isOpen,
    selectedFolder,
    contextMenu,
    refreshAll,
    loadNotes,
    ensureFolderExpanded,
    setContextMenu,
  });

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

  const handleCreateFolderInternal = useCallback(async (parentFolder?: string) => {
    const baseFolder = normalizeFolderPath(parentFolder ?? selectedFolder);
    const promptTitle = baseFolder
      ? `在目录 "${baseFolder}" 下新建子目录（支持输入相对路径）`
      : '请输入新文件夹路径（例如 work/ideas）';
    const raw = window.prompt(promptTitle, '');
    if (raw === null) {
      return;
    }
    const input = normalizeFolderPath(raw);
    if (!input) {
      return;
    }

    const folder = baseFolder && !input.startsWith(`${baseFolder}/`) && input !== baseFolder
      ? `${baseFolder}/${input}`
      : input;
    setLoading(true);
    setError(null);
    try {
      await apiClient.createNotepadFolder({ folder });
      setSelectedFolder(folder);
      ensureFolderExpanded(folder);
      await Promise.all([loadFolders(), loadNotes()]);
    } catch (err) {
      setError(err instanceof Error ? err.message : '创建文件夹失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadFolders, loadNotes, selectedFolder]);

  const handleCreateNoteInternal = useCallback(async (folderOverride?: string) => {
    const targetFolder = normalizeFolderPath(folderOverride ?? selectedFolder);
    const noteTitle = window.prompt('请输入笔记标题', '');
    if (noteTitle === null) {
      return;
    }
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
    } catch (err) {
      setError(err instanceof Error ? err.message : '创建笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, ensureFolderExpanded, loadNotes, openNote, resetEditor, selectedFolder]);

  const handleSaveNote = useCallback(async () => {
    if (!selectedNoteId) {
      return;
    }
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
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存笔记失败');
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
    const remoteNote = res?.note;
    return {
      title: String(remoteNote?.title || fallbackTitle || 'Untitled'),
      content: String(res?.content || ''),
    };
  }, [apiClient, content, selectedNoteId, title]);

  const handleCopyTextInternal = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      await copyTextToClipboard(payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : '复制文本失败');
    }
  }, [copyTextToClipboard, resolveNotePayload]);

  const handleCopyAsMdFileInternal = useCallback(async (note?: NoteMeta | null) => {
    try {
      const payload = await resolveNotePayload(note);
      downloadMarkdownFile(payload.title, payload.content || '');
    } catch (err) {
      setError(err instanceof Error ? err.message : '导出 .md 失败');
    }
  }, [resolveNotePayload]);

  const handleDeleteNoteById = useCallback(async (noteId: string, titleHint?: string) => {
    const normalizedId = String(noteId || '').trim();
    if (!normalizedId) {
      return;
    }
    const confirmed = window.confirm(`确认删除笔记“${titleHint || '当前笔记'}”？此操作不可恢复。`);
    if (!confirmed) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      await apiClient.deleteNotepadNote(normalizedId);
      if (selectedNoteId === normalizedId) {
        resetEditor();
      }
      await loadNotes();
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除笔记失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, loadNotes, resetEditor, selectedNoteId]);

  const handleDeleteNote = useCallback(async () => {
    if (!selectedNoteId) {
      return;
    }
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
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除目录失败');
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
    if (!contextMenu) {
      return;
    }
    const baseFolder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateFolderInternal(baseFolder);
  }, [contextMenu, handleCreateFolderInternal]);

  const handleContextCreateNote = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const folder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await handleCreateNoteInternal(folder);
  }, [contextMenu, handleCreateNoteInternal]);

  const handleContextDelete = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const target = contextMenu.target;
    setContextMenu(null);
    if (target.type === 'folder') {
      await handleDeleteFolder(target.folderPath);
      return;
    }
    await handleDeleteNoteById(target.note.id, target.note.title || undefined);
  }, [contextMenu, handleDeleteFolder, handleDeleteNoteById]);

  const handleContextDeleteSelectedNote = useCallback(async () => {
    if (!selectedNoteMeta) {
      return;
    }
    setContextMenu(null);
    await handleDeleteNoteById(selectedNoteMeta.id, selectedNoteMeta.title || undefined);
  }, [handleDeleteNoteById, selectedNoteMeta]);

  const handleContextCopyText = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyTextInternal(targetNote);
  }, [contextMenu, handleCopyTextInternal, selectedNoteMeta]);

  const handleContextCopyAsMd = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError('请先选中笔记');
      return;
    }
    await handleCopyAsMdFileInternal(targetNote);
  }, [contextMenu, handleCopyAsMdFileInternal, selectedNoteMeta]);

  const contextMenuStyle = useMemo(
    () => buildContextMenuStyle(contextMenu),
    [contextMenu],
  );

  return {
    availableFoldersCount: availableFolders.length,
    contextMenu,
    contextMenuStyle,
    content,
    dirty,
    error,
    expandedFolders,
    folderTree,
    loading,
    notesCount: notes.length,
    searchQuery,
    selectedFolder,
    selectedNoteId,
    selectedNoteMeta,
    tagsText,
    title,
    viewMode,
    setSearchQuery,
    setViewMode,
    handleRefresh: refreshAll,
    handleCreateFolder: () => handleCreateFolderInternal(),
    handleCreateNote: () => handleCreateNoteInternal(),
    handleToggleFolderExpanded: toggleFolderExpanded,
    handleSelectFolder: handleTreeSelectFolder,
    handleOpenNote: handleTreeOpenNote,
    handleFolderContextMenu: handleTreeFolderContextMenu,
    handleNoteContextMenu: handleTreeNoteContextMenu,
    handleCopyText: () => handleCopyTextInternal(selectedNoteMeta),
    handleCopyAsMd: () => handleCopyAsMdFileInternal(selectedNoteMeta),
    handleSave: handleSaveNote,
    handleDelete: handleDeleteNote,
    handleTitleChange: (value: string) => {
      setTitle(value);
      setDirty(true);
    },
    handleTagsTextChange: (value: string) => {
      setTagsText(value);
      setDirty(true);
    },
    handleContentChange: (value: string) => {
      setContent(value);
      setDirty(true);
    },
    handleContextCreateFolder,
    handleContextCreateNote,
    handleContextCopyText,
    handleContextCopyAsMd,
    handleContextDelete,
    handleContextDeleteSelectedNote,
  };
};
