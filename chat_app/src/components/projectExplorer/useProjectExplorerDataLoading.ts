import { useCallback } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';

import type {
  FsEntriesResponse,
  GitStatusResponse,
  GitSummaryResponse,
  ProjectChangeSummaryResponse,
} from '../../lib/api/client/types';
import type { FsEntry, ProjectChangeSummary } from '../../types';
import {
  buildProjectChangeSummaryFromGitStatus,
  EMPTY_CHANGE_SUMMARY,
  isProjectChangeSummaryEqual,
  normalizeEntry,
  normalizeProjectChangeSummary,
} from './utils';
import { normalizeGitSummary } from '../../lib/domain/git';

interface ProjectExplorerApiClient {
  listFsEntries(path: string): Promise<FsEntriesResponse>;
  getGitSummary(root: string): Promise<GitSummaryResponse>;
  getGitStatus(root: string): Promise<GitStatusResponse>;
  getProjectChangeSummary(projectId: string): Promise<ProjectChangeSummaryResponse>;
}

interface UseProjectExplorerDataLoadingParams {
  client: ProjectExplorerApiClient;
  projectId?: string;
  projectRootPath?: string;
  summaryLoadingRef: MutableRefObject<boolean>;
  setLoadingPaths: Dispatch<SetStateAction<Set<string>>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setEntriesMap: Dispatch<SetStateAction<Record<string, FsEntry[]>>>;
  setChangeSummary: Dispatch<SetStateAction<ProjectChangeSummary>>;
  setSummaryError: Dispatch<SetStateAction<string | null>>;
  setLoadingSummary: Dispatch<SetStateAction<boolean>>;
}

const readErrorMessage = (error: unknown, fallback: string): string => (
  error instanceof Error ? error.message : fallback
);

interface LoadEntriesOptions {
  silent?: boolean;
}

export const useProjectExplorerDataLoading = ({
  client,
  projectId,
  projectRootPath,
  summaryLoadingRef,
  setLoadingPaths,
  setError,
  setEntriesMap,
  setChangeSummary,
  setSummaryError,
  setLoadingSummary,
}: UseProjectExplorerDataLoadingParams) => {
  const tryLoadEntries = useCallback(async (path: string, options?: LoadEntriesOptions) => {
    const silent = options?.silent === true;
    setLoadingPaths((prev) => new Set(prev).add(path));
    if (!silent) {
      setError(null);
    }
    try {
      const data = await client.listFsEntries(path);
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

  const loadEntries = useCallback(async (path: string) => {
    await tryLoadEntries(path);
  }, [tryLoadEntries]);

  const loadChangeSummary = useCallback(async (options?: { silent?: boolean }) => {
    const silent = options?.silent ?? false;
    if (!projectId && !projectRootPath) {
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
      let nextSummary: ProjectChangeSummary | null = null;
      if (projectRootPath) {
        try {
          const gitSummaryRaw = await client.getGitSummary(projectRootPath);
          const gitSummary = normalizeGitSummary(gitSummaryRaw);
          if (gitSummary.isRepo) {
            const gitStatusRaw = await client.getGitStatus(projectRootPath);
            nextSummary = buildProjectChangeSummaryFromGitStatus(
              gitSummaryRaw,
              gitStatusRaw,
              projectRootPath,
            );
          }
        } catch {
          nextSummary = null;
        }
      }
      if (!nextSummary) {
        if (!projectId) {
          nextSummary = EMPTY_CHANGE_SUMMARY;
        } else {
          const data = await client.getProjectChangeSummary(projectId);
          nextSummary = normalizeProjectChangeSummary(data);
        }
      }
      setChangeSummary((prev) => (
        isProjectChangeSummaryEqual(prev, nextSummary) ? prev : nextSummary
      ));
      if (!silent) {
        setSummaryError(null);
      }
    } catch (err) {
      if (!silent) {
        setSummaryError(readErrorMessage(err, '加载变更失败'));
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
    projectRootPath,
    setChangeSummary,
    setLoadingSummary,
    setSummaryError,
    summaryLoadingRef,
  ]);

  return {
    loadEntries,
    tryLoadEntries,
    loadChangeSummary,
  };
};
