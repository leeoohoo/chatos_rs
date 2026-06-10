import { useCallback, useRef, useState } from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { normalizeGitCompare, normalizeGitFileDiff } from '../../../lib/domain/git';
import type { GitBranchInfo, GitCompareResult, GitFileDiff } from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';

interface UseProjectGitCompareParams {
  client: ProjectGitApiClient;
  projectRoot: string | null;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
}

export const useProjectGitCompare = ({
  client,
  projectRoot,
  setError,
}: UseProjectGitCompareParams) => {
  const { t } = useI18n();
  const [compareResult, setCompareResult] = useState<GitCompareResult | null>(null);
  const [fileDiff, setFileDiff] = useState<GitFileDiff | null>(null);
  const [loadingCompare, setLoadingCompare] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const compareRequestIdRef = useRef(0);
  const diffRequestIdRef = useRef(0);

  const clearCompare = useCallback(() => {
    compareRequestIdRef.current += 1;
    diffRequestIdRef.current += 1;
    setCompareResult(null);
    setFileDiff(null);
    setLoadingCompare(false);
    setLoadingDiff(false);
  }, []);

  const clearFileDiff = useCallback(() => {
    diffRequestIdRef.current += 1;
    setFileDiff(null);
    setLoadingDiff(false);
  }, []);

  const compareBranch = useCallback(async (branch: GitBranchInfo) => {
    if (!projectRoot) return;
    const target = branch.name;
    if (!target) return;
    const requestId = compareRequestIdRef.current + 1;
    compareRequestIdRef.current = requestId;
    setLoadingCompare(true);
    setError(null);
    setFileDiff(null);
    try {
      const raw = await client.compareGitBranch(projectRoot, target);
      if (compareRequestIdRef.current !== requestId) return;
      setCompareResult(normalizeGitCompare(raw));
    } catch (err) {
      if (compareRequestIdRef.current !== requestId) return;
      setCompareResult(null);
      setError(err instanceof Error ? err.message : t('git.error.compareFailed'));
    } finally {
      if (compareRequestIdRef.current === requestId) {
        setLoadingCompare(false);
      }
    }
  }, [client, projectRoot, setError, t]);

  const loadFileDiff = useCallback(async (path: string, target?: string, staged?: boolean) => {
    if (!projectRoot) return;
    if (!path) return;
    const requestId = diffRequestIdRef.current + 1;
    diffRequestIdRef.current = requestId;
    setLoadingDiff(true);
    setError(null);
    try {
      const raw = await client.getGitDiff({ root: projectRoot, path, target, staged });
      if (diffRequestIdRef.current !== requestId) return;
      setFileDiff(normalizeGitFileDiff(raw));
    } catch (err) {
      if (diffRequestIdRef.current !== requestId) return;
      setFileDiff(null);
      setError(err instanceof Error ? err.message : t('git.error.diffLoadFailed'));
    } finally {
      if (diffRequestIdRef.current === requestId) {
        setLoadingDiff(false);
      }
    }
  }, [client, projectRoot, setError, t]);

  return {
    compareResult,
    fileDiff,
    loadingCompare,
    loadingDiff,
    clearCompare,
    clearFileDiff,
    compareBranch,
    loadFileDiff,
  };
};
