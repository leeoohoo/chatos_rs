import { useCallback, useMemo } from 'react';

import type { FsEntry, ProjectChangeSummary } from '../../../types';
import type { ChangeKind } from '../utils';

interface UseProjectTreeEntriesDerivedStateOptions {
  projectRootPath: string;
  entriesMap: Record<string, FsEntry[]>;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
}

export const useProjectTreeEntriesDerivedState = ({
  projectRootPath,
  entriesMap,
  showOnlyChanged,
  changeSummary,
  aggregatedChangeKindByPath,
  normalizePath,
}: UseProjectTreeEntriesDerivedStateOptions) => {
  const isEntryVisible = useCallback((entryPath: string): boolean => {
    if (!showOnlyChanged) return true;
    return aggregatedChangeKindByPath.has(normalizePath(entryPath));
  }, [aggregatedChangeKindByPath, normalizePath, showOnlyChanged]);

  const visibleRootEntryCount = useMemo(() => {
    const rootEntries = entriesMap[projectRootPath] || [];
    return rootEntries.filter((entry) => isEntryVisible(entry.path)).length;
  }, [entriesMap, isEntryVisible, projectRootPath]);

  const loadedEntryPathSet = useMemo(() => {
    const out = new Set<string>();
    Object.values(entriesMap).forEach((entries) => {
      entries.forEach((entry) => {
        const normalized = normalizePath(entry.path);
        if (normalized) {
          out.add(normalized);
        }
      });
    });
    return out;
  }, [entriesMap, normalizePath]);

  const hiddenFileMarks = useMemo(
    () => changeSummary.fileMarks.filter((mark) => {
      const normalizedMarkPath = normalizePath(mark.path);
      if (!normalizedMarkPath) {
        return false;
      }
      return !loadedEntryPathSet.has(normalizedMarkPath);
    }),
    [changeSummary.fileMarks, loadedEntryPathSet, normalizePath],
  );

  return {
    hiddenFileMarks,
    isEntryVisible,
    visibleRootEntryCount,
  };
};
