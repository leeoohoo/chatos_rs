// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useMemo } from 'react';
import type React from 'react';

import type { FsEntry, FsReadResult } from '../../types';
import type { MoveConflictState } from './Overlays';
import type { ExplorerContextMenuState } from './useProjectExplorerState';

interface UseProjectExplorerWorkspaceInteractionsParams {
  projectRootPath: string;
  contextMenu: ExplorerContextMenuState | null;
  normalizePath: (value: string) => string;
  clearDragExpandTimer: () => void;
  clearDragAutoScroll: () => void;
  setSelectedPath: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedFile: React.Dispatch<React.SetStateAction<FsReadResult | null>>;
  setContextMenu: React.Dispatch<React.SetStateAction<ExplorerContextMenuState | null>>;
  setDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  setDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  setMoveConflict: React.Dispatch<React.SetStateAction<MoveConflictState | null>>;
}

export const useProjectExplorerWorkspaceInteractions = ({
  projectRootPath,
  contextMenu,
  normalizePath,
  clearDragExpandTimer,
  clearDragAutoScroll,
  setSelectedPath,
  setSelectedFile,
  setContextMenu,
  setDraggingEntryPath,
  setDropTargetDirPath,
  setMoveConflict,
}: UseProjectExplorerWorkspaceInteractionsParams) => {
  const openEntryContextMenu = useCallback((event: React.MouseEvent, entry: FsEntry) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedPath(entry.path);
    if (entry.isDir) {
      setSelectedFile(null);
    }
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      entry,
    });
  }, [setContextMenu, setSelectedFile, setSelectedPath]);

  const handleDragStart = useCallback((event: React.DragEvent, entry: FsEntry) => {
    if (!entry.path) {
      return;
    }
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(entry.path);
    setDropTargetDirPath(null);
    setMoveConflict(null);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', entry.path);
  }, [
    clearDragAutoScroll,
    clearDragExpandTimer,
    setDraggingEntryPath,
    setDropTargetDirPath,
    setMoveConflict,
  ]);

  const handleDragEnd = useCallback(() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);
  }, [clearDragAutoScroll, clearDragExpandTimer, setDraggingEntryPath, setDropTargetDirPath]);

  const isContextRootEntry = useMemo(() => {
    if (!contextMenu?.entry.path || !projectRootPath) {
      return false;
    }
    return normalizePath(contextMenu.entry.path) === normalizePath(projectRootPath);
  }, [contextMenu, normalizePath, projectRootPath]);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) {
      return undefined;
    }
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 240 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    } satisfies React.CSSProperties;
  }, [contextMenu]);

  return {
    openEntryContextMenu,
    handleDragStart,
    handleDragEnd,
    isContextRootEntry,
    contextMenuStyle,
  };
};
