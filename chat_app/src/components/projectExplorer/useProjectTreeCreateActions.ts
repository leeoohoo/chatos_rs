import { useCallback } from 'react';

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

  const handleCreateDirectory = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError('请先选择一个目录');
      return;
    }

    const rawName = await prompt({
      title: '新建目录',
      message: '请输入新目录名称',
      inputLabel: '目录名称',
      placeholder: '例如 src',
      confirmText: '创建',
      cancelText: '取消',
      type: 'info',
    });
    if (rawName === null) return;

    const name = rawName.trim();
    if (!name) {
      setActionError('目录名称不能为空');
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError('目录名称不合法');
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
      setActionMessage(`已创建目录：${name}`);
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '创建目录失败'));
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
    toExpandedKey,
  ]);

  const handleCreateFile = useCallback(async (dirPathOverride?: string) => {
    const targetDirPath = dirPathOverride || selectedDirPath;
    if (!targetDirPath) {
      setActionError('请先选择一个目录');
      return;
    }

    const rawName = await prompt({
      title: '新建文件',
      message: '请输入新文件名称',
      inputLabel: '文件名称',
      placeholder: '例如 index.ts',
      confirmText: '创建',
      cancelText: '取消',
      type: 'info',
    });
    if (rawName === null) return;

    const name = rawName.trim();
    if (!name) {
      setActionError('文件名称不能为空');
      return;
    }
    if (!isValidEntryName(name)) {
      setActionError('文件名称不合法');
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
      setActionMessage(`已创建文件：${name}`);
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
      setActionError(readProjectTreeErrorMessage(err, '创建文件失败'));
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
    toExpandedKey,
  ]);

  return {
    handleCreateDirectory,
    handleCreateFile,
  };
};
