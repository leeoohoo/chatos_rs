import { useProjectExplorerChangeTracking } from './useProjectExplorerChangeTracking';
import { useProjectExplorerDnd } from './useProjectExplorerDnd';
import { useProjectTreeActions } from './useProjectTreeActions';

interface UseProjectExplorerWorkspaceActionsParams {
  changeTracking: Parameters<typeof useProjectExplorerChangeTracking>[0];
  dnd: Parameters<typeof useProjectExplorerDnd>[0];
  treeActions: Omit<
    Parameters<typeof useProjectTreeActions>[0],
    'hasPendingChangesForPath'
    | 'canDropToDirectory'
    | 'clearDragExpandTimer'
    | 'clearDragAutoScroll'
  >;
}

export const useProjectExplorerWorkspaceActions = ({
  changeTracking,
  dnd,
  treeActions,
}: UseProjectExplorerWorkspaceActionsParams) => {
  const {
    hasPendingChangesForPath,
    canConfirmCurrent,
    aggregatedChangeKindByPath,
  } = useProjectExplorerChangeTracking(changeTracking);

  const {
    canDropToDirectory,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
  } = useProjectExplorerDnd(dnd);

  const {
    handleCreateDirectory,
    handleCreateFile,
    handleDeleteSelected,
    handleDownloadSelected,
    handleRefresh,
    handleConfirmCurrentChanges,
    handleConfirmAllChanges,
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  } = useProjectTreeActions({
    ...treeActions,
    hasPendingChangesForPath,
    canDropToDirectory,
    clearDragExpandTimer,
    clearDragAutoScroll,
  });

  return {
    hasPendingChangesForPath,
    canConfirmCurrent,
    aggregatedChangeKindByPath,
    canDropToDirectory,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
    handleCreateDirectory,
    handleCreateFile,
    handleDeleteSelected,
    handleDownloadSelected,
    handleRefresh,
    handleConfirmCurrentChanges,
    handleConfirmAllChanges,
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  };
};
