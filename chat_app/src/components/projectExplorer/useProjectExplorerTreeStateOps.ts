import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type { FsEntry, FsReadResult } from '../../types';

interface UseProjectExplorerTreeStateOpsParams {
  projectRootPath: string | null | undefined;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  keyToPath: (key: string) => string;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  loadEntries: (path: string) => Promise<void>;
  loadChangeSummary: (options?: { silent?: boolean }) => Promise<void>;
  clearSearch: () => void;
  clearSearchNavigation: () => void;
  clearTokenSelection: () => void;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
  setExpandedPaths: Dispatch<SetStateAction<Set<string>>>;
  setSelectedPath: Dispatch<SetStateAction<string | null>>;
  setSelectedFile: Dispatch<SetStateAction<FsReadResult | null>>;
  setActionError: Dispatch<SetStateAction<string | null>>;
  setError: Dispatch<SetStateAction<string | null>>;
}

export const useProjectExplorerTreeStateOps = ({
  projectRootPath,
  entriesMap,
  expandedPaths,
  keyToPath,
  normalizePath,
  toExpandedKey,
  loadEntries,
  loadChangeSummary,
  clearSearch,
  clearSearchNavigation,
  clearTokenSelection,
  setEntriesMap,
  setExpandedPaths,
  setSelectedPath,
  setSelectedFile,
  setActionError,
  setError,
}: UseProjectExplorerTreeStateOpsParams) => {
  const toggleDir = useCallback(async (entry: FsEntry) => {
    if (!entry.isDir) return;
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    const key = toExpandedKey(entry.path);
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
    if (!entriesMap[entry.path]) {
      await loadEntries(entry.path);
    }
  }, [
    entriesMap,
    loadEntries,
    setActionError,
    setExpandedPaths,
    setSelectedFile,
    setSelectedPath,
    toExpandedKey,
  ]);

  const replaceExpandedPathPrefix = useCallback((sourcePath: string, movedPath: string) => {
    const normalizedSource = normalizePath(sourcePath);
    const normalizedMoved = normalizePath(movedPath);
    const sourcePrefix = `${normalizedSource}/`;
    const next = new Set<string>();
    expandedPaths.forEach((key) => {
      const full = normalizePath(keyToPath(key));
      if (full === normalizedSource || full.startsWith(sourcePrefix)) {
        const suffix = full.slice(normalizedSource.length);
        const nextPath = normalizePath(`${normalizedMoved}${suffix}`);
        next.add(toExpandedKey(nextPath));
      } else {
        next.add(key);
      }
    });
    return next;
  }, [expandedPaths, keyToPath, normalizePath, toExpandedKey]);

  const reloadTreeWithExpanded = useCallback(async (nextExpanded: Set<string>) => {
    if (!projectRootPath) return;
    setEntriesMap({});
    await loadEntries(projectRootPath);
    const tasks = Array.from(nextExpanded)
      .filter((key) => key.length > 0)
      .map((key) => loadEntries(keyToPath(key)));
    if (tasks.length > 0) {
      await Promise.all(tasks);
    }
  }, [keyToPath, loadEntries, projectRootPath, setEntriesMap]);

  const pruneDeletedPath = useCallback((deletedPath: string) => {
    const normalizedDeleted = normalizePath(deletedPath);
    const deletedPrefix = `${normalizedDeleted}/`;

    setEntriesMap((prev) => {
      const next: Record<string, FsEntry[]> = {};
      Object.entries(prev).forEach(([key, entries]) => {
        const normalizedKey = normalizePath(key);
        if (normalizedKey === normalizedDeleted || normalizedKey.startsWith(deletedPrefix)) {
          return;
        }
        next[key] = entries.filter((entry) => {
          const normalizedEntryPath = normalizePath(entry.path);
          return normalizedEntryPath !== normalizedDeleted && !normalizedEntryPath.startsWith(deletedPrefix);
        });
      });
      return next;
    });

    setExpandedPaths((prev) => {
      const next = new Set<string>();
      prev.forEach((key) => {
        const full = normalizePath(keyToPath(key));
        if (full !== normalizedDeleted && !full.startsWith(deletedPrefix)) {
          next.add(key);
        }
      });
      return next;
    });
  }, [keyToPath, normalizePath, setEntriesMap, setExpandedPaths]);

  const handleGitRepositoryChanged = useCallback(async () => {
    if (!projectRootPath) return;
    clearSearch();
    clearSearchNavigation();
    clearTokenSelection();
    setSelectedPath(projectRootPath);
    setSelectedFile(null);
    setError(null);
    setEntriesMap({});
    await loadEntries(projectRootPath);
    await loadChangeSummary();
  }, [
    clearSearch,
    clearSearchNavigation,
    clearTokenSelection,
    loadChangeSummary,
    loadEntries,
    projectRootPath,
    setEntriesMap,
    setError,
    setSelectedFile,
    setSelectedPath,
  ]);

  return {
    toggleDir,
    replaceExpandedPathPrefix,
    reloadTreeWithExpanded,
    pruneDeletedPath,
    handleGitRepositoryChanged,
  };
};
