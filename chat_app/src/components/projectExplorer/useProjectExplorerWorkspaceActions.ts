import { useProjectExplorerDnd } from './useProjectExplorerDnd';
import { useProjectTreeActions } from './useProjectTreeActions';

interface UseProjectExplorerWorkspaceActionsParams {
  dnd: Parameters<typeof useProjectExplorerDnd>[0];
  treeActions: Omit<
    Parameters<typeof useProjectTreeActions>[0],
    'canDropToDirectory'
    | 'clearDragExpandTimer'
    | 'clearDragAutoScroll'
  >;
}

export const useProjectExplorerWorkspaceActions = ({
  dnd,
  treeActions,
}: UseProjectExplorerWorkspaceActionsParams) => {
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
    handleCopyFilePath,
    handleCopyRelativeFilePath,
    handleAppendGitignore,
    handleOpenExternally,
    handleDeleteSelected,
    handleDownloadSelected,
    handleRefresh,
    handleSaveFile,
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  } = useProjectTreeActions({
    ...treeActions,
    canDropToDirectory,
    clearDragExpandTimer,
    clearDragAutoScroll,
  });

  return {
    canDropToDirectory,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
    handleCreateDirectory,
    handleCreateFile,
    handleCopyFilePath,
    handleCopyRelativeFilePath,
    handleAppendGitignore,
    handleOpenExternally,
    handleDeleteSelected,
    handleDownloadSelected,
    handleRefresh,
    handleSaveFile,
    handleMoveEntryByDrop,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  };
};
