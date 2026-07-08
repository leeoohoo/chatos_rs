// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';
import type React from 'react';

import type { ContextMenuState } from './NotepadContextMenu';

interface UseNotepadPanelEffectsParams {
  isOpen: boolean;
  selectedFolder: string;
  searchQuery: string;
  contextMenu: ContextMenuState | null;
  refreshAll: (options?: { force?: boolean }) => Promise<void>;
  loadNotes: (options?: { force?: boolean }) => Promise<void>;
  hydrateFolders: () => void;
  hydrateNotes: () => void;
  ensureFolderExpanded: (folderPath: string) => void;
  setContextMenu: React.Dispatch<React.SetStateAction<ContextMenuState | null>>;
  refreshNonce?: number;
}

export const useNotepadPanelEffects = ({
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
  refreshNonce = 0,
}: UseNotepadPanelEffectsParams) => {
  const lastSearchQueryRef = useRef(searchQuery);
  const wasOpenRef = useRef(false);
  const lastRefreshNonceRef = useRef(refreshNonce);

  useEffect(() => {
    if (!isOpen) {
      wasOpenRef.current = false;
      return;
    }
    if (wasOpenRef.current) {
      return;
    }
    wasOpenRef.current = true;
    hydrateFolders();
    hydrateNotes();
    lastSearchQueryRef.current = searchQuery;
    void refreshAll();
  }, [hydrateFolders, hydrateNotes, isOpen, refreshAll, searchQuery]);

  useEffect(() => {
    if (!isOpen) {
      return;
    }
    if (lastSearchQueryRef.current === searchQuery) {
      return;
    }
    lastSearchQueryRef.current = searchQuery;
    void loadNotes();
  }, [isOpen, loadNotes, searchQuery]);

  useEffect(() => {
    if (!selectedFolder) {
      return;
    }
    ensureFolderExpanded(selectedFolder);
  }, [ensureFolderExpanded, selectedFolder]);

  useEffect(() => {
    if (!isOpen) {
      lastRefreshNonceRef.current = refreshNonce;
      return;
    }
    if (lastRefreshNonceRef.current === refreshNonce) {
      return;
    }
    lastRefreshNonceRef.current = refreshNonce;
    hydrateFolders();
    hydrateNotes();
    void refreshAll({ force: true });
  }, [hydrateFolders, hydrateNotes, isOpen, refreshAll, refreshNonce]);

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
