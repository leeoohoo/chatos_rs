// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { FsEntry } from '../../types';
import { useDialogService } from '../ui/DialogProvider';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeDeleteActionOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'selectedEntry'
  | 'selectedFilePath'
  | 'projectRootPath'
  | 'normalizePath'
  | 'getParentPath'
  | 'loadEntries'
  | 'pruneDeletedPath'
  | 'setSelectedPath'
  | 'setSelectedFile'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
>;

export const useProjectTreeDeleteAction = ({
  client,
  selectedEntry,
  selectedFilePath,
  projectRootPath,
  normalizePath,
  getParentPath,
  loadEntries,
  pruneDeletedPath,
  setSelectedPath,
  setSelectedFile,
  setActionLoading,
  setActionError,
  setActionMessage,
}: UseProjectTreeDeleteActionOptions) => {
  const { t } = useI18n();
  const { confirm } = useDialogService();

  const handleDeleteSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError(t('projectExplorer.delete.selectFirst'));
      return;
    }

    const targetIsRoot = !!projectRootPath
      && normalizePath(targetEntry.path) === normalizePath(projectRootPath);
    if (targetIsRoot) {
      setActionError(t('projectExplorer.delete.rootNotAllowed'));
      return;
    }

    const confirmed = await confirm({
      title: targetEntry.isDir ? t('projectExplorer.delete.titleDir') : t('projectExplorer.delete.titleFile'),
      message: targetEntry.isDir
        ? t('projectExplorer.delete.messageDir', { name: targetEntry.name })
        : t('projectExplorer.delete.messageFile', { name: targetEntry.name }),
      confirmText: t('aiModelManager.action.delete'),
      cancelText: t('common.cancel'),
      type: 'danger',
    });
    if (!confirmed) return;

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      await client.deleteFsEntry(targetEntry.path, targetEntry.isDir);
      pruneDeletedPath(targetEntry.path);
      if (selectedFilePath && normalizePath(selectedFilePath) === normalizePath(targetEntry.path)) {
        setSelectedFile(null);
      }

      const fallbackPath = getParentPath(targetEntry.path) || projectRootPath || null;
      setSelectedPath(fallbackPath);
      if (fallbackPath) {
        await loadEntries(fallbackPath);
      }
      setActionMessage(t('projectExplorer.delete.success', { name: targetEntry.name }));
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.delete.failed')));
    } finally {
      setActionLoading(false);
    }
  }, [
    client,
    confirm,
    getParentPath,
    loadEntries,
    normalizePath,
    projectRootPath,
    pruneDeletedPath,
    selectedEntry,
    selectedFilePath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setSelectedFile,
    setSelectedPath,
    t,
  ]);

  return {
    handleDeleteSelected,
  };
};
