// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { normalizeFile } from './utils';
import { readProjectTreeErrorMessage } from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeSaveFileActionOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'projectRootPath'
  | 'getParentPath'
  | 'normalizePath'
  | 'loadEntries'
  | 'setSelectedFile'
  | 'setActionError'
  | 'setActionMessage'
> & {
  setSavingFile: (value: boolean) => void;
  setSaveError: (value: string | null) => void;
};

export const useProjectTreeSaveFileAction = ({
  client,
  projectRootPath,
  getParentPath,
  normalizePath,
  loadEntries,
  setSelectedFile,
  setActionError,
  setActionMessage,
  setSavingFile,
  setSaveError,
}: UseProjectTreeSaveFileActionOptions) => {
  const { t } = useI18n();

  const handleSaveFile = useCallback(async (path: string, content: string) => {
    const trimmedPath = path.trim();
    if (!trimmedPath) {
      setSaveError(t('projectExplorer.action.filePathRequired'));
      return false;
    }

    setSavingFile(true);
    setSaveError(null);
    setActionError(null);
    setActionMessage(null);
    try {
      await client.writeFsFile(trimmedPath, content);
      const refreshed = await client.readFsFile(trimmedPath);
      setSelectedFile(normalizeFile(refreshed));

      const parentPath = getParentPath(trimmedPath) || projectRootPath || null;
      if (parentPath) {
        await loadEntries(parentPath, { silent: true, forceRefresh: true });
      }
      if (
        projectRootPath
        && parentPath
        && normalizePath(parentPath) !== normalizePath(projectRootPath)
      ) {
        await loadEntries(projectRootPath, { silent: true, forceRefresh: true });
      }
      setActionMessage(t('projectExplorer.action.fileSaved', { name: trimmedPath.split(/[\\/]/).pop() || trimmedPath }));
      return true;
    } catch (error) {
      const message = readProjectTreeErrorMessage(error, t('projectExplorer.action.fileSaveFailed'));
      setSaveError(message);
      return false;
    } finally {
      setSavingFile(false);
    }
  }, [
    client,
    getParentPath,
    loadEntries,
    normalizePath,
    projectRootPath,
    setActionError,
    setActionMessage,
    setSaveError,
    setSavingFile,
    setSelectedFile,
    t,
  ]);

  return {
    handleSaveFile,
  };
};
