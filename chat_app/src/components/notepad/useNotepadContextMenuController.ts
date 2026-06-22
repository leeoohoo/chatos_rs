import { useCallback, useMemo, useState } from 'react';
import type React from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ContextMenuState, ContextMenuTarget } from './NotepadContextMenu';
import { buildContextMenuStyle } from './controllerHelpers';
import type { NoteMeta } from './utils';

interface UseNotepadContextMenuControllerOptions {
  selectedNoteMeta: NoteMeta | null;
  t: TranslateFn;
  setSelectedFolder: React.Dispatch<React.SetStateAction<string>>;
  ensureFolderExpanded: (folderPath: string) => void;
  createFolder: (parentFolder?: string) => Promise<void>;
  createNote: (folderOverride?: string) => Promise<void>;
  deleteFolder: (folderPath?: string) => Promise<void>;
  deleteNoteById: (noteId: string, titleHint?: string) => Promise<void>;
  copyText: (note?: NoteMeta | null) => Promise<void>;
  copyAsMd: (note?: NoteMeta | null) => Promise<void>;
  setError: (message: string | null) => void;
}

export const useNotepadContextMenuController = ({
  selectedNoteMeta,
  t,
  setSelectedFolder,
  ensureFolderExpanded,
  createFolder,
  createNote,
  deleteFolder,
  deleteNoteById,
  copyText,
  copyAsMd,
  setError,
}: UseNotepadContextMenuControllerOptions) => {
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

  const openContextMenu = useCallback((event: React.MouseEvent, target: ContextMenuTarget) => {
    event.preventDefault();
    event.stopPropagation();
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      target,
    });
  }, []);

  const handleFolderContextMenu = useCallback((event: React.MouseEvent, folderPath: string) => {
    setSelectedFolder(folderPath);
    ensureFolderExpanded(folderPath);
    openContextMenu(event, { type: 'folder', folderPath });
  }, [ensureFolderExpanded, openContextMenu, setSelectedFolder]);

  const handleNoteContextMenu = useCallback((event: React.MouseEvent, note: NoteMeta) => {
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
    await createFolder(baseFolder);
  }, [contextMenu, createFolder]);

  const handleContextCreateNote = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const folder = contextMenu.target.type === 'folder'
      ? contextMenu.target.folderPath
      : contextMenu.target.note.folder;
    setContextMenu(null);
    await createNote(folder);
  }, [contextMenu, createNote]);

  const handleContextDelete = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const target = contextMenu.target;
    setContextMenu(null);
    if (target.type === 'folder') {
      await deleteFolder(target.folderPath);
      return;
    }
    await deleteNoteById(target.note.id, target.note.title || undefined);
  }, [contextMenu, deleteFolder, deleteNoteById]);

  const handleContextDeleteSelectedNote = useCallback(async () => {
    if (!selectedNoteMeta) {
      return;
    }
    setContextMenu(null);
    await deleteNoteById(selectedNoteMeta.id, selectedNoteMeta.title || undefined);
  }, [deleteNoteById, selectedNoteMeta]);

  const handleContextCopyText = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError(t('notepad.error.selectNoteFirst'));
      return;
    }
    await copyText(targetNote);
  }, [contextMenu, copyText, selectedNoteMeta, setError, t]);

  const handleContextCopyAsMd = useCallback(async () => {
    if (!contextMenu) {
      return;
    }
    const targetNote = contextMenu.target.type === 'note'
      ? contextMenu.target.note
      : selectedNoteMeta;
    setContextMenu(null);
    if (!targetNote) {
      setError(t('notepad.error.selectNoteFirst'));
      return;
    }
    await copyAsMd(targetNote);
  }, [contextMenu, copyAsMd, selectedNoteMeta, setError, t]);

  const contextMenuStyle = useMemo(
    () => buildContextMenuStyle(contextMenu),
    [contextMenu],
  );

  return {
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
  };
};
