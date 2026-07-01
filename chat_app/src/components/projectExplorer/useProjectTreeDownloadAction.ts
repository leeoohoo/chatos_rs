// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { FsEntry } from '../../types';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeDownloadActionOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'selectedEntry'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeDownloadAction = ({
  client,
  selectedEntry,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeDownloadActionOptions) => {
  const { t } = useI18n();

  const handleDownloadSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError(t('projectExplorer.action.downloadSelectFirst'));
      return;
    }
    if (typeof document === 'undefined') {
      setActionError(t('projectExplorer.action.downloadUnsupported'));
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const { blob, filename } = await client.downloadFsEntry(targetEntry.path);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = filename || targetEntry.name || 'download';
      anchor.style.display = 'none';
      document.body.appendChild(anchor);
      anchor.click();
      document.body.removeChild(anchor);
      URL.revokeObjectURL(url);
      setActionMessage(t('projectExplorer.action.downloadStarted', { name: anchor.download }));
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.downloadFailed')));
    } finally {
      setActionLoading(false);
    }
  }, [client, selectedEntry, setActionError, setActionLoading, setActionMessage, t]);

  return {
    handleDownloadSelected,
  };
};
