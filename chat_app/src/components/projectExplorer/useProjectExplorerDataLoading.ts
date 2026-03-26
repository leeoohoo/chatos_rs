import { useCallback } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';

import type { FsEntry, ProjectChangeSummary } from '../../types';
import {
  EMPTY_CHANGE_SUMMARY,
  isProjectChangeSummaryEqual,
  normalizeEntry,
  normalizeProjectChangeSummary,
} from './utils';

interface ProjectExplorerApiClient {
  listFsEntries(path: string): Promise<any>;
  getProjectChangeSummary(projectId: string): Promise<any>;
}

interface UseProjectExplorerDataLoadingParams {
  client: ProjectExplorerApiClient;
  projectId?: string;
  summaryLoadingRef: MutableRefObject<boolean>;
  setLoadingPaths: Dispatch<SetStateAction<Set<string>>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
  setChangeSummary: Dispatch<SetStateAction<ProjectChangeSummary>>;
  setSummaryError: Dispatch<SetStateAction<string | null>>;
  setLoadingSummary: Dispatch<SetStateAction<boolean>>;
}

export const useProjectExplorerDataLoading = ({
  client,
  projectId,
  summaryLoadingRef,
  setLoadingPaths,
  setError,
  setEntriesMap,
  setChangeSummary,
  setSummaryError,
  setLoadingSummary,
}: UseProjectExplorerDataLoadingParams) => {
  const loadEntries = useCallback(async (path: string) => {
    setLoadingPaths((prev) => new Set(prev).add(path));
    setError(null);
    try {
      const data = await client.listFsEntries(path);
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeEntry) : [];
      setEntriesMap((prev) => ({ ...prev, [path]: entries }));
    } catch (err: any) {
      setError(err?.message || '加载目录失败');
    } finally {
      setLoadingPaths((prev) => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  }, [client, setEntriesMap, setError, setLoadingPaths]);

  const loadChangeSummary = useCallback(async (options?: { silent?: boolean }) => {
    const silent = options?.silent ?? false;
    if (!projectId) {
      if (!silent) {
        setChangeSummary(EMPTY_CHANGE_SUMMARY);
        setSummaryError(null);
      }
      return;
    }
    if (summaryLoadingRef.current) {
      return;
    }
    summaryLoadingRef.current = true;
    if (!silent) {
      setLoadingSummary(true);
      setSummaryError(null);
    }
    try {
      const data = await client.getProjectChangeSummary(projectId);
      const nextSummary = normalizeProjectChangeSummary(data);
      setChangeSummary((prev) => (
        isProjectChangeSummaryEqual(prev, nextSummary) ? prev : nextSummary
      ));
      if (!silent) {
        setSummaryError(null);
      }
    } catch (err: any) {
      if (!silent) {
        setSummaryError(err?.message || '加载变更标记失败');
        setChangeSummary(EMPTY_CHANGE_SUMMARY);
      }
    } finally {
      if (!silent) {
        setLoadingSummary(false);
      }
      summaryLoadingRef.current = false;
    }
  }, [
    client,
    projectId,
    setChangeSummary,
    setLoadingSummary,
    setSummaryError,
    summaryLoadingRef,
  ]);

  return {
    loadEntries,
    loadChangeSummary,
  };
};
