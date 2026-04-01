import { useEffect } from 'react';
import type React from 'react';

import type { ContextMenuState } from './NotepadContextMenu';

interface UseNotepadPanelEffectsParams {
  isOpen: boolean;
  selectedFolder: string;
  contextMenu: ContextMenuState | null;
  refreshAll: () => Promise<void>;
  loadNotes: () => Promise<void>;
  ensureFolderExpanded: (folderPath: string) => void;
  setContextMenu: React.Dispatch<React.SetStateAction<ContextMenuState | null>>;
}

export const useNotepadPanelEffects = ({
  isOpen,
  selectedFolder,
  contextMenu,
  refreshAll,
  loadNotes,
  ensureFolderExpanded,
  setContextMenu,
}: UseNotepadPanelEffectsParams) => {
  useEffect(() => {
    if (!isOpen) {
      return;
    }
    void refreshAll();
  }, [isOpen, refreshAll]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    void loadNotes();
  }, [isOpen, loadNotes]);

  useEffect(() => {
    if (!selectedFolder) {
      return;
    }
    ensureFolderExpanded(selectedFolder);
  }, [ensureFolderExpanded, selectedFolder]);

  useEffect(() => {
    if (!contextMenu) {
      return undefined;
    }

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
  }, [contextMenu, setContextMenu]);
};
