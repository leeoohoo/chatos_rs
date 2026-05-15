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
  loadChangeSummary,
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
    loadChangeSummary,
    pruneDeletedPath,
    setExpandedPaths,
    setSelectedPath,
    setSelectedFile,
    setActionError,
    setActionLoading,
    setActionMessage,
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
