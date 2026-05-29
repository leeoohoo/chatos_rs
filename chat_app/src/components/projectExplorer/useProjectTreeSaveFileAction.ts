import { useCallback } from 'react';

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
  const handleSaveFile = useCallback(async (path: string, content: string) => {
    const trimmedPath = path.trim();
    if (!trimmedPath) {
      setSaveError('文件路径不能为空');
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
      setActionMessage(`已保存文件：${trimmedPath.split(/[\\/]/).pop() || trimmedPath}`);
      return true;
    } catch (error) {
      const message = readProjectTreeErrorMessage(error, '保存文件失败');
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
  ]);

  return {
    handleSaveFile,
  };
};
