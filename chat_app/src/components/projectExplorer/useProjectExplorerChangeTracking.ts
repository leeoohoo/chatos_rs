import { useMemo } from 'react';
import type { ProjectChangeSummary } from '../../types';
import { CHANGE_KIND_PRIORITY, normalizeChangeKind, type ChangeKind } from './utils';

interface Params {
  changeSummary: ProjectChangeSummary;
  normalizePath: (value: string) => string;
  getParentPath: (value: string) => string | null;
  rootPathNormalized: string;
}

export const useProjectExplorerChangeTracking = ({
  changeSummary,
  normalizePath,
  getParentPath,
  rootPathNormalized,
}: Params) => {
  const pendingMarks = useMemo(
    () => [...changeSummary.fileMarks, ...changeSummary.deletedMarks],
    [changeSummary.deletedMarks, changeSummary.fileMarks]
  );

  const aggregatedChangeKindByPath = useMemo(() => {
    const map = new Map<string, ChangeKind>();
    const applyKind = (path: string, kind: ChangeKind) => {
      const prev = map.get(path);
      if (!prev || CHANGE_KIND_PRIORITY[kind] >= CHANGE_KIND_PRIORITY[prev]) {
        map.set(path, kind);
      }
    };

    for (const mark of pendingMarks) {
      const normalizedMarkPath = normalizePath(mark.path);
      if (!normalizedMarkPath) continue;
      const kind = normalizeChangeKind(mark.kind);
      applyKind(normalizedMarkPath, kind);

      let parentPath = getParentPath(normalizedMarkPath);
      while (parentPath) {
        const normalizedParent = normalizePath(parentPath);
        if (!normalizedParent) break;
        applyKind(normalizedParent, kind);
        if (rootPathNormalized && normalizedParent === rootPathNormalized) {
          break;
        }
        parentPath = getParentPath(normalizedParent);
      }
    }

    return map;
  }, [getParentPath, normalizePath, pendingMarks, rootPathNormalized]);

  return {
    pendingMarks,
    aggregatedChangeKindByPath,
  };
};
