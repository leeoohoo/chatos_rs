import { useCallback, useMemo, useState } from 'react';
import type React from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useNotepadRealtime } from '../../lib/realtime/useNotepadRealtime';
import { useChatApiClientFromContext } from '../../lib/store/ChatStoreContext';
import type { ContextMenuState } from './NotepadContextMenu';
import type { NotepadViewMode } from './NotepadEditor';
import { useNotepadPanelEffects } from './useNotepadPanelEffects';
import {
  buildFolderTree,
  normalizeFolderPath,
  type NoteMeta,
} from './utils';
import { useDialogService } from '../ui/DialogProvider';
import { useNotepadContextMenuController } from './useNotepadContextMenuController';
import { useNotepadEditorState } from './useNotepadEditorState';
import { useNotepadExportActions } from './useNotepadExportActions';
import { useNotepadFolderExpansion } from './useNotepadFolderExpansion';
import { useNotepadData } from './useNotepadData';
import { useNotepadOpenNote } from './useNotepadOpenNote';
import { useNotepadCrudActions } from './useNotepadCrudActions';

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
  const { confirm, prompt } = useDialogService();

  const [selectedFolder, setSelectedFolder] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [viewMode, setViewMode] = useState<NotepadViewMode>('split');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshNonce, setRefreshNonce] = useState(0);
  const {
    selectedNoteId,
    setSelectedNoteId,
    title,
    setTitle,
    tagsText,
    setTagsText,
    content,
    setContent,
    dirty,
    setDirty,
    resetEditor,
    handleTitleChange,
    handleTagsTextChange,
    handleContentChange,
  } = useNotepadEditorState();
  const {
    expandedFolders,
    ensureFolderExpanded,
    toggleFolderExpanded,
  } = useNotepadFolderExpansion();

  const {
    notes,
    availableFolders,
    folderTree,
    selectedNoteMeta,
    ensureInit,
    hydrateFolders,
    hydrateNotes,
    loadFolders,
    loadNotes,
    loadNoteDetail,
    getCachedNoteDetail,
    markFoldersStale,
    markNotesStale,
    markNoteDetailStale,
    upsertCachedNoteDetail,
    upsertCachedNote,
    removeCachedNote,
    applyFolderToCache,
    removeFolderFromCache,
    renameFolderInCache,
  } = useNotepadData({
    apiClient,
    searchQuery,
    selectedNoteId,
  });

  const refreshAll = useCallback(async (options?: { force?: boolean }) => {
    setLoading(true);
    setError(null);
    try {
      if (options?.force) {
        markFoldersStale();
        markNotesStale();
      }
      await ensureInit(options);
      await Promise.all([
        loadFolders(options),
        loadNotes(options),
      ]);
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载记事本失败');
    } finally {
      setLoading(false);
    }
  }, [ensureInit, loadFolders, loadNotes, markFoldersStale, markNotesStale]);

  const openNote = useNotepadOpenNote({
    ensureFolderExpanded,
    loadNoteDetail,
    setContent,
    setDirty,
    setError,
    setLoading,
    setSelectedFolder,
    setSelectedNoteId,
    setTagsText,
    setTitle,
  });

  const {
    copyText: handleCopyTextInternal,
    copyAsMd: handleCopyAsMdFileInternal,
  } = useNotepadExportActions({
    getCachedNoteDetail,
    loadNoteDetail,
    selectedNoteId,
    title,
    content,
    setError,
  });

  const {
    createFolder: handleCreateFolderInternal,
    createNote: handleCreateNoteInternal,
    saveNote: handleSaveNote,
    deleteNoteById: handleDeleteNoteById,
    deleteNote: handleDeleteNote,
    deleteFolder: handleDeleteFolder,
  } = useNotepadCrudActions({
    apiClient,
    confirm,
    content,
    ensureFolderExpanded,
    loadNotes,
    markNotesStale,
    upsertCachedNoteDetail,
    upsertCachedNote,
    removeCachedNote,
    applyFolderToCache,
    removeFolderFromCache,
    notes,
    openNote,
    prompt,
    resetEditor,
    selectedFolder,
    selectedNoteId,
    setDirty,
    setError,
    setLoading,
    setSelectedFolder,
    tagsText,
    title,
  });

  const handleTreeSelectFolder = useCallback((folderPath: string) => {
    setSelectedFolder(folderPath);
    ensureFolderExpanded(folderPath);
  }, [ensureFolderExpanded]);

  const handleTreeOpenNote = useCallback((noteId: string) => {
    void openNote(noteId);
  }, [openNote]);

  const renameFolderPath = useCallback((folderPath: string, fromPath: string, toPath: string) => {
    const normalizedFolder = normalizeFolderPath(folderPath);
    const normalizedFrom = normalizeFolderPath(fromPath);
    const normalizedTo = normalizeFolderPath(toPath);
    if (!normalizedFrom || !normalizedTo) {
      return normalizedFolder;
    }
    if (normalizedFolder === normalizedFrom) {
      return normalizedTo;
    }
    const prefix = `${normalizedFrom}/`;
    if (normalizedFolder.startsWith(prefix)) {
      const suffix = normalizedFolder.slice(prefix.length);
      return suffix ? `${normalizedTo}/${suffix}` : normalizedTo;
    }
    return normalizedFolder;
  }, []);

  useNotepadRealtime({
    enabled: true,
    onInvalidate: async (payload) => {
      const reason = String(payload.reason || '').trim();
      const payloadFolder = String(payload.folder || '').trim();
      const payloadFrom = String(payload.from || '').trim();
      const payloadTo = String(payload.to || '').trim();
      const payloadNoteId = String(payload.note_id || '').trim();
      const currentNoteId = String(selectedNoteId || '').trim();
      const normalizedSelectedFolder = normalizeFolderPath(selectedFolder);
      const currentListContainsRenamedFolder = Boolean(
        payloadFrom
        && notes.some((note) => {
          const noteFolder = normalizeFolderPath(note.folder);
          return noteFolder === payloadFrom || noteFolder.startsWith(`${payloadFrom}/`);
        }),
      );

      if (reason === 'folder_created' && payloadFolder) {
        applyFolderToCache(payloadFolder);
      } else if (reason === 'folder_deleted' && payloadFolder) {
        removeFolderFromCache(payloadFolder);
        if (
          normalizedSelectedFolder
          && (normalizedSelectedFolder === payloadFolder || normalizedSelectedFolder.startsWith(`${payloadFolder}/`))
        ) {
          setSelectedFolder('');
        }
        if (selectedNoteMeta) {
          const selectedNoteFolder = normalizeFolderPath(selectedNoteMeta.folder);
          if (selectedNoteFolder === payloadFolder || selectedNoteFolder.startsWith(`${payloadFolder}/`)) {
            resetEditor();
          }
        }
      } else if (reason === 'folder_renamed' && payloadFrom && payloadTo) {
        renameFolderInCache(payloadFrom, payloadTo);
        if (
          normalizedSelectedFolder
          && (normalizedSelectedFolder === payloadFrom || normalizedSelectedFolder.startsWith(`${payloadFrom}/`))
        ) {
          const nextSelectedFolder = renameFolderPath(normalizedSelectedFolder, payloadFrom, payloadTo);
          setSelectedFolder(nextSelectedFolder);
          ensureFolderExpanded(nextSelectedFolder);
        }
      } else if (reason === 'note_deleted' && payloadNoteId) {
        removeCachedNote(payloadNoteId);
        if (payloadNoteId === currentNoteId) {
          resetEditor();
        }
      }

      if (reason === 'folder_renamed') {
        markFoldersStale();
        markNotesStale();
      } else if (reason === 'note_created' || reason === 'note_updated') {
        markNotesStale();
      }
      if (payloadNoteId && reason !== 'note_deleted') {
        markNoteDetailStale(payloadNoteId);
      }

      if (!isOpen) {
        setRefreshNonce((value) => value + 1);
        return;
      }

      if (payloadNoteId && currentNoteId && payloadNoteId === currentNoteId && !dirty) {
        await openNote(currentNoteId);
        return;
      }

      if (reason === 'folder_renamed') {
        hydrateFolders();
      }
      if (reason === 'note_deleted') {
        hydrateNotes();
      }

      if ((reason === 'note_created' || reason === 'note_updated') && payloadNoteId) {
        await loadNoteDetail(payloadNoteId, { force: true });
        hydrateNotes();
        return;
      }

      const affectsSelectedFolder = !selectedFolder
        || payloadFolder === selectedFolder
        || payloadFolder.startsWith(`${selectedFolder}/`)
        || payloadFrom === selectedFolder
        || payloadFrom.startsWith(`${selectedFolder}/`)
        || payloadTo === selectedFolder
        || payloadTo.startsWith(`${selectedFolder}/`);

      if (reason === 'folder_renamed' && (affectsSelectedFolder || currentListContainsRenamedFolder)) {
        await loadNotes({ force: true });
      }
    },
  });

  const {
    contextMenu,
    setContextMenu,
    contextMenuStyle,
    handleFolderContextMenu,
    handleNoteContextMenu,
    handleContextCreateFolder,
    handleContextCreateNote,
    handleContextCopyText,
    handleContextCopyAsMd,
    handleContextDelete,
    handleContextDeleteSelectedNote,
  } = useNotepadContextMenuController({
    selectedNoteMeta,
    setSelectedFolder,
    ensureFolderExpanded,
    createFolder: handleCreateFolderInternal,
    createNote: handleCreateNoteInternal,
    deleteFolder: handleDeleteFolder,
    deleteNoteById: handleDeleteNoteById,
    copyText: handleCopyTextInternal,
    copyAsMd: handleCopyAsMdFileInternal,
    setError,
  });

  useNotepadPanelEffects({
    isOpen,
    selectedFolder,
    searchQuery,
    contextMenu,
    refreshAll,
    loadNotes,
    hydrateFolders,
    hydrateNotes,
    ensureFolderExpanded,
    setContextMenu,
    refreshNonce,
  });

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
    handleRefresh: () => refreshAll({ force: true }),
    handleCreateFolder: () => handleCreateFolderInternal(),
    handleCreateNote: () => handleCreateNoteInternal(),
    handleToggleFolderExpanded: toggleFolderExpanded,
    handleSelectFolder: handleTreeSelectFolder,
    handleOpenNote: handleTreeOpenNote,
    handleFolderContextMenu,
    handleNoteContextMenu,
    handleCopyText: () => handleCopyTextInternal(selectedNoteMeta),
    handleCopyAsMd: () => handleCopyAsMdFileInternal(selectedNoteMeta),
    handleSave: handleSaveNote,
    handleDelete: handleDeleteNote,
    handleTitleChange,
    handleTagsTextChange,
    handleContentChange,
    handleContextCreateFolder,
    handleContextCreateNote,
    handleContextCopyText,
    handleContextCopyAsMd,
    handleContextDelete,
    handleContextDeleteSelectedNote,
  };
};
