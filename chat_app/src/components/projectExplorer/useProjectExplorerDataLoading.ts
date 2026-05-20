import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type {
  FsEntriesResponse,
} from '../../lib/api/client/types';
import type { FsEntry } from '../../types';
import { normalizeEntry } from './utils';

interface ProjectExplorerApiClient {
  listFsEntries(path: string, options?: { forceRefresh?: boolean }): Promise<FsEntriesResponse>;
}

interface UseProjectExplorerDataLoadingParams {
  client: ProjectExplorerApiClient;
  setLoadingPaths: Dispatch<SetStateAction<Set<string>>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
}

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

interface LoadEntriesOptions {
  silent?: boolean;
  forceRefresh?: boolean;
}

export const useProjectExplorerDataLoading = ({
  client,
  setLoadingPaths,
  setError,
  setEntriesMap,
}: UseProjectExplorerDataLoadingParams) => {
  const tryLoadEntries = useCallback(async (path: string, options?: LoadEntriesOptions) => {
    const silent = options?.silent === true;
    setLoadingPaths((prev) => new Set(prev).add(path));
    if (!silent) {
      setError(null);
    }
    try {
      const data = await client.listFsEntries(path, { forceRefresh: options?.forceRefresh });
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeEntry) : [];
      setEntriesMap((prev) => ({ ...prev, [path]: entries }));
      return true;
    } catch (err) {
      if (!silent) {
        setError(readErrorMessage(err, '加载目录失败'));
      }
      return false;
    } finally {
      setLoadingPaths((prev) => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  }, [client, setEntriesMap, setError, setLoadingPaths]);

  const loadEntries = useCallback(async (path: string, options?: LoadEntriesOptions) => {
    await tryLoadEntries(path, options);
  }, [tryLoadEntries]);

  return {
    loadEntries,
    tryLoadEntries,
  };
};
