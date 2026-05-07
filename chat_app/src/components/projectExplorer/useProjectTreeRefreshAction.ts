import { useCallback } from 'react';

import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeRefreshActionOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'actionReloadPath'
  | 'projectRootPath'
  | 'normalizePath'
  | 'loadEntries'
  | 'loadChangeSummary'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeRefreshAction = ({
  actionReloadPath,
  projectRootPath,
  normalizePath,
  loadEntries,
  loadChangeSummary,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeRefreshActionOptions) => {
  const handleRefresh = useCallback(async () => {
    if (!actionReloadPath) return;

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      await loadEntries(actionReloadPath);
      if (
        projectRootPath
        && normalizePath(actionReloadPath) !== normalizePath(projectRootPath)
      ) {
        await loadEntries(projectRootPath);
      }
      await loadChangeSummary();
      setActionMessage('目录已刷新');
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '刷新失败'));
    } finally {
      setActionLoading(false);
    }
  }, [
    actionReloadPath,
    loadChangeSummary,
    loadEntries,
    normalizePath,
    projectRootPath,
    setActionError,
    setActionLoading,
    setActionMessage,
  ]);

  return {
    handleRefresh,
  };
};
