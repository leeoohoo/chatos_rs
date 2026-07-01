// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type React from 'react';

interface UseProjectTreeRootDropHandlersOptions {
  draggingEntryPath: string | null;
  projectRootPath: string;
  normalizePath: (value: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  onSetDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  onSetDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  onMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => void;
  onClearDragExpandTimer: () => void;
  onClearDragAutoScroll: () => void;
}

export const useProjectTreeRootDropHandlers = ({
  draggingEntryPath,
  projectRootPath,
  normalizePath,
  canDropToDirectory,
  onSetDropTargetDirPath,
  onSetDraggingEntryPath,
  onMoveEntryByDrop,
  onClearDragExpandTimer,
  onClearDragAutoScroll,
}: UseProjectTreeRootDropHandlersOptions) => {
  const getSourcePath = (event: React.DragEvent): string => (
    draggingEntryPath || event.dataTransfer.getData('text/plain')
  );

  const handleRootDragOver = (event: React.DragEvent<HTMLDivElement>) => {
    const sourcePath = getSourcePath(event);
    if (!sourcePath) return;
    if (!canDropToDirectory(sourcePath, projectRootPath)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  };

  const handleRootDragEnter = (event: React.DragEvent<HTMLDivElement>) => {
    const sourcePath = getSourcePath(event);
    if (!sourcePath) return;
    if (!canDropToDirectory(sourcePath, projectRootPath)) return;
    event.preventDefault();
    onClearDragExpandTimer();
    onClearDragAutoScroll();
    onSetDropTargetDirPath(projectRootPath);
  };

  const handleRootDragLeave = (event: React.DragEvent<HTMLDivElement>) => {
    const nextTarget = event.relatedTarget as Node | null;
    if (nextTarget && (event.currentTarget as HTMLElement).contains(nextTarget)) {
      return;
    }
    const normalizedRoot = normalizePath(projectRootPath);
    onSetDropTargetDirPath((prev) => (
      prev && normalizePath(prev) === normalizedRoot ? null : prev
    ));
  };

  const handleRootDrop = (event: React.DragEvent<HTMLDivElement>) => {
    const sourcePath = getSourcePath(event);
    if (!sourcePath) return;
    if (!canDropToDirectory(sourcePath, projectRootPath)) return;
    event.preventDefault();
    event.stopPropagation();
    onClearDragExpandTimer();
    onClearDragAutoScroll();
    onSetDropTargetDirPath(null);
    onSetDraggingEntryPath(null);
    onMoveEntryByDrop(sourcePath, projectRootPath);
  };

  return {
    handleRootDragEnter,
    handleRootDragLeave,
    handleRootDragOver,
    handleRootDrop,
  };
};
