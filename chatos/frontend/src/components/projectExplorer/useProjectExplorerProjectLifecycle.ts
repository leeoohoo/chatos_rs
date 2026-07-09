// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
import type { FsEntry, FsReadResult } from '../../types';
import type { MoveConflictState } from './Overlays';
import type { ExplorerContextMenuState } from './useProjectExplorerState';

interface UseProjectExplorerProjectLifecycleOptions {
  projectId?: string | null;
  projectRootPath?: string | null;
  filesTabActive: boolean;
  toExpandedKey: (path: string) => string;
  keyToPath: (key: string) => string;
  loadEntries: (path: string, options?: { silent?: boolean; forceRefresh?: boolean }) => Promise<void>;
  tryLoadEntries: (path: string, options?: { silent?: boolean; forceRefresh?: boolean }) => Promise<boolean>;
  clearDragExpandTimer: () => void;
  clearDragAutoScroll: () => void;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
  setLoadingPaths: Dispatch<SetStateAction<Set<string>>>;
  setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
  setSelectedPath: Dispatch<SetStateAction<string | null>>;
  setSelectedFile: Dispatch<SetStateAction<FsReadResult | null>>;
  setActionMessage: Dispatch<SetStateAction<string | null>>;
  setActionError: Dispatch<SetStateAction<string | null>>;
  setActionLoading: Dispatch<SetStateAction<boolean>>;
  setContextMenu: Dispatch<SetStateAction<ExplorerContextMenuState | null>>;
  setMoveConflict: Dispatch<SetStateAction<MoveConflictState | null>>;
  setDraggingEntryPath: Dispatch<SetStateAction<string | null>>;
  setDropTargetDirPath: Dispatch<SetStateAction<string | null>>;
  setExpandedReady: Dispatch<SetStateAction<boolean>>;
}

export const useProjectExplorerProjectLifecycle = ({
  projectId,
  projectRootPath,
  filesTabActive,
  toExpandedKey,
  keyToPath,
  loadEntries,
  tryLoadEntries,
  clearDragExpandTimer,
  clearDragAutoScroll,
  setEntriesMap,
  setLoadingPaths,
  setExpandedPaths,
  setSelectedPath,
  setSelectedFile,
  setActionMessage,
  setActionError,
  setActionLoading,
  setContextMenu,
  setMoveConflict,
  setDraggingEntryPath,
  setDropTargetDirPath,
  setExpandedReady,
}: UseProjectExplorerProjectLifecycleOptions) => {
  const activeTreeKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (!projectRootPath) {
      activeTreeKeyRef.current = null;
      clearDragExpandTimer();
      clearDragAutoScroll();
      setEntriesMap({});
      setLoadingPaths(new Set());
      setExpandedPaths(new Set());
      setSelectedPath(null);
      setSelectedFile(null);
      setActionMessage(null);
      setActionError(null);
      setActionLoading(false);
      setContextMenu(null);
      setMoveConflict(null);
      setDraggingEntryPath(null);
      setDropTargetDirPath(null);
      setExpandedReady(false);
      return;
    }

    if (!filesTabActive) {
      setLoadingPaths(new Set());
      return;
    }

    const root = projectRootPath;
    const treeKey = `${projectId || ''}:${root}`;
    const projectChanged = activeTreeKeyRef.current !== treeKey;
    activeTreeKeyRef.current = treeKey;

    clearDragExpandTimer();
    clearDragAutoScroll();
    setLoadingPaths(new Set());
    if (projectChanged) {
      setEntriesMap({});
    }

    const saved = projectId ? localStorage.getItem(`project_explorer_expanded_${projectId}`) : null;
    let nextExpanded = new Set<string>();
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed)) {
          nextExpanded = new Set(
            parsed
              .filter((p) => typeof p === 'string')
              .map((p) => toExpandedKey(p))
          );
        }
      } catch {
        nextExpanded = new Set();
      }
    }

    setExpandedPaths(nextExpanded);
    setExpandedReady(true);
    setSelectedPath(root);
    setSelectedFile(null);
    setActionMessage(null);
    setActionError(null);
    setActionLoading(false);
    setContextMenu(null);
    setMoveConflict(null);
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);

    void loadEntries(root, { silent: !projectChanged });
    void (async () => {
      const expandedQueue = Array.from(nextExpanded)
        .filter(Boolean)
        .map((expandedPath) => ({
          expandedPath,
          full: keyToPath(expandedPath),
        }))
        .filter(({ full }) => full !== root)
        .slice(0, 10);

      if (expandedQueue.length === 0) {
        return;
      }

      const validExpanded = new Set<string>();
      const concurrency = 2;
      let cursor = 0;
      const workers = Array.from({ length: Math.min(concurrency, expandedQueue.length) }, async () => {
        while (cursor < expandedQueue.length) {
          const currentIndex = cursor;
          cursor += 1;
          const current = expandedQueue[currentIndex];
          if (!current) {
            return;
          }
          const ok = await tryLoadEntries(current.full, { silent: true });
          if (ok) {
            validExpanded.add(current.expandedPath);
          }
        }
      });

      await Promise.all(workers);

      if (validExpanded.size > 0 && validExpanded.size !== nextExpanded.size) {
        setExpandedPaths(validExpanded);
      }
    })();
  }, [
    clearDragAutoScroll,
    clearDragExpandTimer,
    filesTabActive,
    keyToPath,
    loadEntries,
    projectId,
    projectRootPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setContextMenu,
    setDropTargetDirPath,
    setDraggingEntryPath,
    setEntriesMap,
    setExpandedPaths,
    setExpandedReady,
    setLoadingPaths,
    setMoveConflict,
    setSelectedFile,
    setSelectedPath,
    toExpandedKey,
    tryLoadEntries,
  ]);
};
