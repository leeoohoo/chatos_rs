import { useProjectTreeCreateActions } from './useProjectTreeCreateActions';
import { useProjectTreeContextActions } from './useProjectTreeContextActions';
import { useProjectTreeDeleteAction } from './useProjectTreeDeleteAction';
import { useProjectTreeDownloadAction } from './useProjectTreeDownloadAction';
import { useProjectTreeRefreshAction } from './useProjectTreeRefreshAction';
import { useProjectTreeSaveFileAction } from './useProjectTreeSaveFileAction';
import type { UseProjectTreeActionsOptions } from './projectTreeActionTypes';

type UseProjectTreeMutationActionsOptions = Pick<
  UseProjectTreeActionsOptions,
  | 'client'
  | 'selectedDirPath'
  | 'selectedEntry'
  | 'selectedFilePath'
  | 'projectRootPath'
  | 'actionReloadPath'
  | 'normalizePath'
  | 'getParentPath'
  | 'toExpandedKey'
  | 'loadEntries'
  | 'pruneDeletedPath'
  | 'setExpandedPaths'
  | 'setSelectedPath'
  | 'setSelectedFile'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
  | 'setSavingFile'
  | 'setSaveError'
  | 'setMoveConflict'
  | 'openFile'
>;

export const useProjectTreeMutationActions = (options: UseProjectTreeMutationActionsOptions) => {
  const createActions = useProjectTreeCreateActions(options);
  const contextActions = useProjectTreeContextActions(options);
  const deleteAction = useProjectTreeDeleteAction(options);
  const downloadAction = useProjectTreeDownloadAction(options);
  const refreshAction = useProjectTreeRefreshAction(options);
  const saveFileAction = useProjectTreeSaveFileAction(options);

  return {
    ...createActions,
    ...contextActions,
    ...deleteAction,
    ...downloadAction,
    ...refreshAction,
    ...saveFileAction,
  };
};
