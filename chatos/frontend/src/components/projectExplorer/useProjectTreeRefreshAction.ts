// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeRefreshAction = ({
  actionReloadPath,
  projectRootPath,
  normalizePath,
  loadEntries,
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
      await loadEntries(actionReloadPath, { forceRefresh: true });
      if (
        projectRootPath
        && normalizePath(actionReloadPath) !== normalizePath(projectRootPath)
      ) {
        await loadEntries(projectRootPath, { forceRefresh: true });
      }
      setActionMessage(t('projectExplorer.refresh.success'));
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.refresh.failed')));
    } finally {
      setActionLoading(false);
    }
  }, [
    actionReloadPath,
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
