import { useCallback, useMemo, useState } from 'react';
import type React from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext } from '../../lib/store/ChatStoreContext';
import type { ContextMenuState } from './NotepadContextMenu';
import type { NotepadViewMode } from './NotepadEditor';
import { useNotepadPanelEffects } from './useNotepadPanelEffects';
import {
  buildFolderTree,
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
    loadFolders,
    loadNotes,
  } = useNotepadData({
    apiClient,
    searchQuery,
    selectedNoteId,
  });

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

  const openNote = useNotepadOpenNote({
    apiClient,
    ensureFolderExpanded,
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
    apiClient,
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
    loadFolders,
    loadNotes,
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
    contextMenu,
    refreshAll,
    loadNotes,
    ensureFolderExpanded,
    setContextMenu,
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
    handleRefresh: refreshAll,
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
