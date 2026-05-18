import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
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
  const { t } = useI18n();
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
      setActionMessage(t('projectExplorer.refresh.success'));
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.refresh.failed')));
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
    t,
  ]);

  return {
    handleRefresh,
  };
};
