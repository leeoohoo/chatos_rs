import { useEffect } from 'react';

import type { GitBranchesResult, GitStatusResult } from '../../../types';

interface UseProjectGitLifecycleParams {
  clearCompare: () => void;
  setBranches: React.Dispatch<React.SetStateAction<GitBranchesResult | null>>;
  setStatus: React.Dispatch<React.SetStateAction<GitStatusResult | null>>;
  setActionMessage: React.Dispatch<React.SetStateAction<string | null>>;
  refreshClientInfo: () => Promise<void>;
  refreshSummary: () => Promise<void>;
}

export const useProjectGitLifecycle = ({
  clearCompare,
  setBranches,
  setStatus,
  setActionMessage,
  refreshClientInfo,
  refreshSummary,
}: UseProjectGitLifecycleParams) => {
  useEffect(() => {
    setBranches(null);
    setStatus(null);
    clearCompare();
    setActionMessage(null);
    void refreshClientInfo();
    void refreshSummary();
  }, [clearCompare, refreshClientInfo, refreshSummary, setActionMessage, setBranches, setStatus]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshSummary();
    }, 15000);
    const handleFocus = () => {
      void refreshSummary();
    };
    window.addEventListener('focus', handleFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', handleFocus);
    };
  }, [refreshSummary]);
};
