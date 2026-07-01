// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';
import { useProjectTreeMoveActions } from './useProjectTreeMoveActions';
import { useProjectTreeMutationActions } from './useProjectTreeMutationActions';

export const useProjectTreeActions = ({
  client,
  selectedDirPath,
  selectedEntry,
  selectedFilePath,
  projectRootPath,
  actionReloadPath,
  normalizePath,
  getParentPath,
  toExpandedKey,
  loadEntries,
  pruneDeletedPath,
  replaceExpandedPathPrefix,
  reloadTreeWithExpanded,
  canDropToDirectory,
  findEntryByPath,
  clearDragExpandTimer,
  clearDragAutoScroll,
  setExpandedPaths,
  setSelectedPath,
  setSelectedFile,
  setActionLoading,
  setActionError,
  setActionMessage,
  setSavingFile,
  setSaveError,
  setMoveConflict,
  openFile,
}: UseProjectTreeActionsOptions) => {
  const mutationActions = useProjectTreeMutationActions({
    client,
    selectedDirPath,
    selectedEntry,
    selectedFilePath,
    projectRootPath,
    actionReloadPath,
    normalizePath,
    getParentPath,
    toExpandedKey,
    loadEntries,
    pruneDeletedPath,
    setExpandedPaths,
    setSelectedPath,
    setSelectedFile,
    setActionError,
    setActionLoading,
    setActionMessage,
    setSavingFile,
    setSaveError,
    setMoveConflict,
    openFile,
  });

  const moveActions = useProjectTreeMoveActions({
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
  });

  return {
    ...mutationActions,
    ...moveActions,
  };
};
