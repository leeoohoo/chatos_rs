import { useEffect } from 'react';

import type { GitBranchesResult, GitStatusResult } from '../../../types';

interface UseProjectGitLifecycleParams {
  open: boolean;
  clearCompare: () => void;
  setBranches: React.Dispatch<React.SetStateAction<GitBranchesResult | null>>;
  setStatus: React.Dispatch<React.SetStateAction<GitStatusResult | null>>;
  setActionMessage: React.Dispatch<React.SetStateAction<string | null>>;
  hydrateCachedState: (projectRoot: string) => void;
  refreshClientInfo: () => Promise<void>;
  refreshSummary: (options?: { force?: boolean }) => Promise<void>;
  markSummaryStale: () => void;
  markDetailsStale: () => void;
  projectRoot: string;
}

export const useProjectGitLifecycle = ({
  open,
  clearCompare,
  setBranches,
  setStatus,
  setActionMessage,
  hydrateCachedState,
  refreshClientInfo,
  refreshSummary,
  markSummaryStale,
  markDetailsStale,
  projectRoot,
}: UseProjectGitLifecycleParams) => {
  useEffect(() => {
    setBranches(null);
    setStatus(null);
    clearCompare();
    setActionMessage(null);
    markSummaryStale();
    markDetailsStale();
    hydrateCachedState(projectRoot);
    void refreshSummary();
  }, [
    clearCompare,
    hydrateCachedState,
    markDetailsStale,
    markSummaryStale,
    projectRoot,
    refreshSummary,
    setActionMessage,
    setBranches,
    setStatus,
  ]);

  useEffect(() => {
    if (!open) {
      return;
    }
    void refreshClientInfo();
  }, [open, refreshClientInfo]);

  useEffect(() => {
    const handleFocus = () => {
      markSummaryStale();
      if (open) {
        void refreshSummary();
      }
    };
    window.addEventListener('focus', handleFocus);
    return () => {
      window.removeEventListener('focus', handleFocus);
    };
  }, [markSummaryStale, open, refreshSummary]);
};
