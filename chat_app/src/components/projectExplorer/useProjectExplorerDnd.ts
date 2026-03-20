import { useCallback, useEffect, useRef } from 'react';
import type { FsEntry } from '../../types';

interface Params {
  treeScrollRef: React.RefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  loadingPaths: Set<string>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  getParentPath: (value: string) => string | null;
  findEntryByPath: (path: string) => FsEntry | null;
  loadEntries: (path: string) => Promise<void>;
  setExpandedPaths: React.Dispatch<React.SetStateAction<Set<string>>>;
}

export const useProjectExplorerDnd = ({
  treeScrollRef,
  entriesMap,
  loadingPaths,
  normalizePath,
  toExpandedKey,
  getParentPath,
  findEntryByPath,
  loadEntries,
  setExpandedPaths,
}: Params) => {
  const dragExpandTimerRef = useRef<number | null>(null);
  const dragExpandPathRef = useRef<string | null>(null);
  const dragAutoScrollTimerRef = useRef<number | null>(null);
  const dragAutoScrollVelocityRef = useRef(0);

  const canDropToDirectory = useCallback((sourcePath: string, targetDirPath: string): boolean => {
    const normalizedSource = normalizePath(sourcePath);
    const normalizedTarget = normalizePath(targetDirPath);
    if (!normalizedSource || !normalizedTarget) return false;
    if (normalizedSource === normalizedTarget) return false;

    const targetEntry = findEntryByPath(targetDirPath);
    if (!targetEntry?.isDir) return false;

    const sourceEntry = findEntryByPath(sourcePath);
    if (!sourceEntry) return false;

    const sourceParent = getParentPath(sourcePath);
    if (sourceParent && normalizePath(sourceParent) === normalizedTarget) {
      return false;
    }

    if (sourceEntry.isDir && normalizedTarget.startsWith(`${normalizedSource}/`)) {
      return false;
    }

    return true;
  }, [findEntryByPath, getParentPath, normalizePath]);

  const clearDragExpandTimer = useCallback(() => {
    if (dragExpandTimerRef.current !== null) {
      window.clearTimeout(dragExpandTimerRef.current);
      dragExpandTimerRef.current = null;
    }
    dragExpandPathRef.current = null;
  }, []);

  const cancelDragExpandIfMatches = useCallback((path: string) => {
    const pendingPath = dragExpandPathRef.current;
    if (!pendingPath) return;
    if (normalizePath(pendingPath) !== normalizePath(path)) return;
    clearDragExpandTimer();
  }, [clearDragExpandTimer, normalizePath]);

  const scheduleDragExpand = useCallback((path: string) => {
    const normalizedPath = normalizePath(path);
    const pendingPath = dragExpandPathRef.current;
    if (pendingPath && normalizePath(pendingPath) === normalizedPath) {
      return;
    }
    clearDragExpandTimer();
    dragExpandPathRef.current = path;
    dragExpandTimerRef.current = window.setTimeout(() => {
      const key = toExpandedKey(path);
      setExpandedPaths((prev) => {
        if (prev.has(key)) return prev;
        const next = new Set(prev);
        next.add(key);
        return next;
      });
      if (!entriesMap[path] && !loadingPaths.has(path)) {
        void loadEntries(path);
      }
      dragExpandTimerRef.current = null;
      dragExpandPathRef.current = null;
    }, 500);
  }, [clearDragExpandTimer, entriesMap, loadingPaths, loadEntries, normalizePath, setExpandedPaths, toExpandedKey]);

  const clearDragAutoScroll = useCallback(() => {
    if (dragAutoScrollTimerRef.current !== null) {
      window.clearInterval(dragAutoScrollTimerRef.current);
      dragAutoScrollTimerRef.current = null;
    }
    dragAutoScrollVelocityRef.current = 0;
  }, []);

  const startDragAutoScroll = useCallback((velocity: number) => {
    if (!Number.isFinite(velocity) || velocity === 0) {
      clearDragAutoScroll();
      return;
    }
    dragAutoScrollVelocityRef.current = velocity;
    if (dragAutoScrollTimerRef.current !== null) {
      return;
    }
    dragAutoScrollTimerRef.current = window.setInterval(() => {
      const container = treeScrollRef.current;
      if (!container) return;
      const nextTop = container.scrollTop + dragAutoScrollVelocityRef.current;
      const maxTop = Math.max(0, container.scrollHeight - container.clientHeight);
      container.scrollTop = Math.max(0, Math.min(maxTop, nextTop));
    }, 16);
  }, [clearDragAutoScroll, treeScrollRef]);

  useEffect(() => (() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
  }), [clearDragAutoScroll, clearDragExpandTimer]);

  return {
    canDropToDirectory,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
  };
};
