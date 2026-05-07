import { useCallback, useState } from 'react';

import { normalizeFolderPath } from './utils';

export const useNotepadFolderExpansion = () => {
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['']));

  const ensureFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      next.add('');
      if (!normalized) {
        return next;
      }
      let current = '';
      const parts = normalized.split('/').filter((item) => item.trim().length > 0);
      for (const part of parts) {
        current = current ? `${current}/${part}` : part;
        next.add(current);
      }
      return next;
    });
  }, []);

  const toggleFolderExpanded = useCallback((folderPath: string) => {
    const normalized = normalizeFolderPath(folderPath);
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(normalized)) {
        next.delete(normalized);
      } else {
        next.add(normalized);
      }
      return next;
    });
  }, []);

  return {
    expandedFolders,
    ensureFolderExpanded,
    toggleFolderExpanded,
  };
};
