import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useDialogService } from '../ui/DialogProvider';
import { isValidEntryName } from './utils';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeCreateActionsOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'selectedDirPath'
  | 'toExpandedKey'
  | 'loadEntries'
  | 'setExpandedPaths'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
  | 'setMoveConflict'
  | 'openFile'
>;

export const useProjectTreeCreateActions = ({
  client,
  selectedDirPath,
  toExpandedKey,
  loadEntries,
  setExpandedPaths,
  setActionLoading,
  setActionError,
  setActionMessage,
  setMoveConflict,
  openFile,
}: UseProjectTreeCreateActionsOptions) => {
  const { prompt } = useDialogService();
  const { t } = useI18n();

  const handleCreateDirectory = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError(t('projectExplorer.action.selectDirectoryFirst'));
      return;
    }

    const rawName = await prompt({
      title: t('projectExplorer.action.createDirectoryTitle'),
      message: t('projectExplorer.action.createDirectoryMessage'),
      inputLabel: t('projectExplorer.action.directoryName'),
      placeholder: t('projectExplorer.action.directoryPlaceholder'),
      confirmText: t('projectExplorer.action.create'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (rawName === null) return;

    const name = rawName.trim();
    if (!name) {
      setActionError(t('projectExplorer.action.directoryNameRequired'));
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError(t('projectExplorer.action.directoryNameInvalid'));
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    setMoveConflict(null);
    try {
      await client.createFsDirectory(targetDirPath, name);
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(toExpandedKey(targetDirPath));
        return next;
      });
      await loadEntries(targetDirPath);
      setActionMessage(t('projectExplorer.action.directoryCreated', { name }));
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.directoryCreateFailed')));
    } finally {
      setActionLoading(false);
    }
  }, [
    client,
    loadEntries,
    prompt,
    selectedDirPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setExpandedPaths,
    setMoveConflict,
    t,
    toExpandedKey,
  ]);

  const handleCreateFile = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError(t('projectExplorer.action.selectDirectoryFirst'));
      return;
    }

    const rawName = await prompt({
      title: t('projectExplorer.action.createFileTitle'),
      message: t('projectExplorer.action.createFileMessage'),
      inputLabel: t('projectExplorer.action.fileName'),
      placeholder: t('projectExplorer.action.filePlaceholder'),
      confirmText: t('projectExplorer.action.create'),
      cancelText: t('common.cancel'),
      type: 'info',
    });
    if (rawName === null) return;

    const name = rawName.trim();
    if (!name) {
      setActionError(t('projectExplorer.action.fileNameRequired'));
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError(t('projectExplorer.action.fileNameInvalid'));
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const data = await client.createFsFile(targetDirPath, name, '');
      const createdPath = typeof data?.path === 'string' ? data.path.trim() : '';
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        next.add(toExpandedKey(targetDirPath));
        return next;
      });
      await loadEntries(targetDirPath);
      setActionMessage(t('projectExplorer.action.fileCreated', { name }));
      if (createdPath) {
        await openFile({
          name,
          path: createdPath,
          isDir: false,
          size: 0,
          modifiedAt: null,
        });
      }
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.fileCreateFailed')));
    } finally {
      setActionLoading(false);
    }
  }, [
    client,
    loadEntries,
    openFile,
    prompt,
    selectedDirPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setExpandedPaths,
    t,
    toExpandedKey,
  ]);

  return {
    handleCreateDirectory,
    handleCreateFile,
  };
};
