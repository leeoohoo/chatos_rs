// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
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
  const { t } = useI18n();

  const applyMoveResult = useCallback(async (
    sourcePath: string,
    targetDirPath: string,
    result: FsMoveResponse,
    movedLabel: string,
  ) => {
    const movedPath = readProjectTreeMovedPath(result);
    if (!movedPath) {
      throw new Error(t('projectExplorer.action.moveReturnedEmptyPath'));
    }

    const nextExpanded = replaceExpandedPathPrefix(sourcePath, movedPath);
    nextExpanded.add(toExpandedKey(targetDirPath));
    setExpandedPaths(nextExpanded);
    setSelectedPath(movedPath);
    setSelectedFile(null);
    await reloadTreeWithExpanded(nextExpanded);
    setActionMessage(t('projectExplorer.action.moved', { name: movedLabel }));
  }, [
    reloadTreeWithExpanded,
    replaceExpandedPathPrefix,
    setActionMessage,
    setExpandedPaths,
    setSelectedFile,
    setSelectedPath,
    t,
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
      setActionError(t('projectExplorer.action.dragSourceMissing'));
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
        if (!message.includes('\u5df2\u5b58\u5728\u540c\u540d')) {
          throw err;
        }
        setMoveConflict({
          sourcePath,
          targetDirPath,
          sourceName: sourceEntry.name,
          renameTo: `${sourceEntry.name}_copy`,
        });
        setActionMessage(t('projectExplorer.action.moveConflictMessage'));
      }
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.moveFailed')));
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
    t,
  ]);

  const handleMoveConflictCancel = useCallback(() => {
    setMoveConflict(null);
    setActionMessage(t('projectExplorer.action.moveCancelled'));
  }, [setActionMessage, setMoveConflict, t]);

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
      setActionMessage(t('projectExplorer.action.overwriteMoved', { name: moveConflict.sourceName }));
      setMoveConflict(null);
    } catch (err) {
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.overwriteMoveFailed')));
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, setActionError, setActionLoading, setActionMessage, setMoveConflict, t]);

  const handleMoveConflictRename = useCallback(async (moveConflict: MoveConflictState | null) => {
    if (!moveConflict) return;

    const renamed = moveConflict.renameTo.trim();
    if (!renamed || !isValidEntryName(renamed)) {
      setActionError(t('projectExplorer.action.newNameInvalid'));
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
      setActionError(readProjectTreeErrorMessage(err, t('projectExplorer.action.renameMoveFailed')));
    } finally {
      setActionLoading(false);
    }
  }, [executeMoveEntry, setActionError, setActionLoading, setMoveConflict, t]);

  return {
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  };
};
