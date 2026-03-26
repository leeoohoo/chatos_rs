import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import type { FsEntry, FsReadResult, ProjectChangeSummary } from '../../types';

interface UseProjectExplorerProjectLifecycleOptions {
  projectId?: string | null;
  projectRootPath?: string | null;
  toExpandedKey: (path: string) => string;
  keyToPath: (key: string) => string;
  loadEntries: (path: string) => Promise<void>;
  loadChangeSummary: (options?: { silent?: boolean }) => Promise<void>;
  clearDragExpandTimer: () => void;
  clearDragAutoScroll: () => void;
  resetLogsState: () => void;
  summaryLoadingRef: MutableRefObject<boolean>;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
  setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
  setSelectedPath: Dispatch<SetStateAction<string | null>>;
  setSelectedFile: Dispatch<SetStateAction<FsReadResult | null>>;
  setActionMessage: Dispatch<SetStateAction<string | null>>;
  setActionError: Dispatch<SetStateAction<string | null>>;
  setActionLoading: Dispatch<SetStateAction<boolean>>;
  setContextMenu: Dispatch<SetStateAction<any>>;
  setMoveConflict: Dispatch<SetStateAction<any>>;
  setDraggingEntryPath: Dispatch<SetStateAction<string | null>>;
  setDropTargetDirPath: Dispatch<SetStateAction<string | null>>;
  setChangeSummary: Dispatch<SetStateAction<ProjectChangeSummary>>;
  setSummaryError: Dispatch<SetStateAction<string | null>>;
  setLoadingSummary: Dispatch<SetStateAction<boolean>>;
  setExpandedReady: Dispatch<SetStateAction<boolean>>;
  emptyChangeSummary: ProjectChangeSummary;
}

export const useProjectExplorerProjectLifecycle = ({
  projectId,
  projectRootPath,
  toExpandedKey,
  keyToPath,
  loadEntries,
  loadChangeSummary,
  clearDragExpandTimer,
  clearDragAutoScroll,
  resetLogsState,
  summaryLoadingRef,
  setEntriesMap,
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
  setChangeSummary,
  setSummaryError,
  setLoadingSummary,
  setExpandedReady,
  emptyChangeSummary,
}: UseProjectExplorerProjectLifecycleOptions) => {
  useEffect(() => {
    if (!projectRootPath) {
      clearDragExpandTimer();
      clearDragAutoScroll();
      setEntriesMap({});
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
      resetLogsState();
      setChangeSummary(emptyChangeSummary);
      setSummaryError(null);
      setLoadingSummary(false);
      summaryLoadingRef.current = false;
      setExpandedReady(false);
      return;
    }

    const root = projectRootPath;
    clearDragExpandTimer();
    clearDragAutoScroll();
    setEntriesMap({});

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
    resetLogsState();
    setChangeSummary(emptyChangeSummary);
    setSummaryError(null);

    void loadEntries(root);
    void loadChangeSummary();
    nextExpanded.forEach((expandedPath) => {
      if (!expandedPath) return;
      const full = keyToPath(expandedPath);
      if (full !== root) {
        void loadEntries(full);
      }
    });
  }, [
    clearDragAutoScroll,
    clearDragExpandTimer,
    emptyChangeSummary,
    keyToPath,
    loadChangeSummary,
    loadEntries,
    projectId,
    projectRootPath,
    resetLogsState,
    setActionError,
    setActionLoading,
    setActionMessage,
    setChangeSummary,
    setContextMenu,
    setDropTargetDirPath,
    setDraggingEntryPath,
    setEntriesMap,
    setExpandedPaths,
    setExpandedReady,
    setLoadingSummary,
    setMoveConflict,
    setSelectedFile,
    setSelectedPath,
    setSummaryError,
    summaryLoadingRef,
    toExpandedKey,
  ]);
};

interface UseProjectExplorerSummaryPollingOptions {
  projectId?: string | null;
  loadChangeSummary: (options?: { silent?: boolean }) => Promise<void>;
}

export const useProjectExplorerSummaryPolling = ({
  projectId,
  loadChangeSummary,
}: UseProjectExplorerSummaryPollingOptions) => {
  useEffect(() => {
    if (!projectId) return undefined;
    const timer = window.setInterval(() => {
      void loadChangeSummary({ silent: true });
    }, 6000);
    return () => {
      window.clearInterval(timer);
    };
  }, [loadChangeSummary, projectId]);
};
