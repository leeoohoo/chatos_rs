import { useCallback } from 'react';

import type { FsMoveOptions, FsMoveResponse } from '../../lib/api/client/types';
import { isValidEntryName } from './utils';
import {
  readProjectTreeErrorMessage,
  readProjectTreeMovedPath,
} from './projectTreeActionHelpers';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';
import type { MoveConflictState } from './Overlays';

type UseProjectTreeMoveActionsOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'canDropToDirectory'
  | 'findEntryByPath'
  | 'clearDragExpandTimer'
  | 'clearDragAutoScroll'
  | 'replaceExpandedPathPrefix'
  | 'reloadTreeWithExpanded'
  | 'toExpandedKey'
  | 'setExpandedPaths'
  | 'setSelectedPath'
  | 'setSelectedFile'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
  | 'setMoveConflict'
>;

export const useProjectTreeMoveActions = ({
  client,
  canDropToDirectory,
  findEntryByPath,
  clearDragExpandTimer,
  clearDragAutoScroll,
  replaceExpandedPathPrefix,
  reloadTreeWithExpanded,
  toExpandedKey,
  setExpandedPaths,
  setSelectedPath,
  setSelectedFile,
  setActionLoading,
  setActionError,
  setActionMessage,
  setMoveConflict,
}: UseProjectTreeMoveActionsOptions) => {
  const applyMoveResult = useCallback(async (
    sourcePath: string,
    targetDirPath: string,
    result: FsMoveResponse,
    movedLabel: string,
  ) => {
    const movedPath = readProjectTreeMovedPath(result);
    if (!movedPath) {
      throw new Error('移动成功，但返回路径为空');
    }

    const nextExpanded = replaceExpandedPathPrefix(sourcePath, movedPath);
    nextExpanded.add(toExpandedKey(targetDirPath));
    setExpandedPaths(nextExpanded);
    setSelectedPath(movedPath);
    setSelectedFile(null);
    await reloadTreeWithExpanded(nextExpanded);
    setActionMessage(`已移动：${movedLabel}`);
  }, [
    reloadTreeWithExpanded,
    replaceExpandedPathPrefix,
    setActionMessage,
    setExpandedPaths,
    setSelectedFile,
    setSelectedPath,
    toExpandedKey,
  ]);

  const executeMoveEntry = useCallback(async (
    sourcePath: string,
    targetDirPath: string,
    movedLabel: string,
    options?: FsMoveOptions,
  ) => {
    const result = await client.moveFsEntry(sourcePath, targetDirPath, options);
    await applyMoveResult(sourcePath, targetDirPath, result, movedLabel);
    return result;
  }, [applyMoveResult, client]);

  const handleMoveEntryByDrop = useCallback(async (sourcePath: string, targetDirPath: string) => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    if (!canDropToDirectory(sourcePath, targetDirPath)) return;

    const sourceEntry = findEntryByPath(sourcePath);
    if (!sourceEntry) {
      setActionError('拖拽源文件不存在');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      try {
        await executeMoveEntry(sourcePath, targetDirPath, sourceEntry.name);
      } catch (err) {
        const message = readProjectTreeErrorMessage(err, '');
        if (!message.includes('已存在同名')) {
          throw err;
        }
        setMoveConflict({
          sourcePath,
          targetDirPath,
          sourceName: sourceEntry.name,
          renameTo: `${sourceEntry.name}_copy`,
        });
        setActionMessage('目标已存在同名项，请选择处理方式');
      }
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '移动失败'));
    } finally {
      setActionLoading(false);
    }
  }, [
    canDropToDirectory,
    clearDragAutoScroll,
    clearDragExpandTimer,
    executeMoveEntry,
    findEntryByPath,
    setActionError,
    setActionLoading,
    setActionMessage,
    setMoveConflict,
  ]);

  const handleMoveConflictCancel = useCallback(() => {
    setMoveConflict(null);
    setActionMessage('已取消移动');
  }, [setActionMessage, setMoveConflict]);

  const handleMoveConflictOverwrite = useCallback(async (
    moveConflict: MoveConflictState | null,
  ) => {
    if (!moveConflict) return;

    setActionLoading(true);
    setActionError(null);
    try {
      await executeMoveEntry(
        moveConflict.sourcePath,
        moveConflict.targetDirPath,
        moveConflict.sourceName,
        { replaceExisting: true },
      );
      setActionMessage(`已覆盖并移动：${moveConflict.sourceName}`);
      setMoveConflict(null);
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '覆盖移动失败'));
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, setActionError, setActionLoading, setActionMessage, setMoveConflict]);

  const handleMoveConflictRename = useCallback(async (moveConflict: MoveConflictState | null) => {
    if (!moveConflict) return;

    const renamed = moveConflict.renameTo.trim();
    if (!renamed || !isValidEntryName(renamed)) {
      setActionError('新名称不合法');
      return;
    }

    setActionLoading(true);
    setActionError(null);
    try {
      await executeMoveEntry(
        moveConflict.sourcePath,
        moveConflict.targetDirPath,
        renamed,
        { targetName: renamed },
      );
      setMoveConflict(null);
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, '重命名移动失败'));
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, setActionError, setActionLoading, setMoveConflict]);

  return {
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  };
};
