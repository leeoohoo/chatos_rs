import { useCallback, useMemo } from 'react';

export const useProjectExplorerPathHelpers = (rootPath: string | null | undefined) => {
  const normalizePath = useCallback((value: string) => (
    value.replace(/\\/g, '/').replace(/\/+$/, '')
  ), []);

  const rootPathNormalized = useMemo(
    () => (rootPath ? normalizePath(rootPath) : ''),
    [rootPath, normalizePath]
  );

  const toExpandedKey = useCallback((path: string) => {
    const full = normalizePath(path);
    if (!rootPathNormalized) return full;
    if (full === rootPathNormalized) return '';
    const prefix = `${rootPathNormalized}/`;
    if (full.startsWith(prefix)) {
      return full.slice(prefix.length);
    }
    return full;
  }, [rootPathNormalized, normalizePath]);

  const keyToPath = useCallback((key: string) => {
    if (!rootPathNormalized) return normalizePath(key);
    if (!key) return rootPathNormalized;
    return `${rootPathNormalized}/${key}`;
  }, [rootPathNormalized, normalizePath]);

  const getParentPath = useCallback((value: string): string | null => {
    const normalized = normalizePath(value);
    if (!normalized) return null;
    const idx = normalized.lastIndexOf('/');
    if (idx < 0) return null;
    if (idx === 0) return '/';
    const parent = normalized.slice(0, idx);
    if (/^[A-Za-z]:$/.test(parent)) {
      return `${parent}/`;
    }
    return parent;
  }, [normalizePath]);

  return {
    normalizePath,
    rootPathNormalized,
    toExpandedKey,
    keyToPath,
    getParentPath,
  };
};
