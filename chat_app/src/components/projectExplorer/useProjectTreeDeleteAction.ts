import { useCallback } from 'react';

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
  const { confirm } = useDialogService();

  const handleDeleteSelected = useCallback(async (entryOverride?: FsEntry) => {
    const targetEntry = entryOverride || selectedEntry;
    if (!targetEntry) {
      setActionError('请先选择要删除的文件或目录');
      return;
    }

    const targetIsRoot = !!projectRootPath
      && normalizePath(targetEntry.path) === normalizePath(projectRootPath);
    if (targetIsRoot) {
      setActionError('不支持删除项目根目录');
      return;
    }

    const confirmed = await confirm({
      title: targetEntry.isDir ? '删除目录' : '删除文件',
      message: targetEntry.isDir
        ? `确认删除目录 "${targetEntry.name}" 吗？将递归删除其全部内容。`
        : `确认删除文件 "${targetEntry.name}" 吗？`,
      confirmText: '删除',
      cancelText: '取消',
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
      setActionMessage(`已删除：${targetEntry.name}`);
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '删除失败'));
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
  ]);

  return {
    handleDeleteSelected,
  };
};
