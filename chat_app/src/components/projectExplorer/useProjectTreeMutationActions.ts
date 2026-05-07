import { useProjectTreeCreateActions } from './useProjectTreeCreateActions';
import { useProjectTreeDeleteAction } from './useProjectTreeDeleteAction';
import { useProjectTreeDownloadAction } from './useProjectTreeDownloadAction';
import { useProjectTreeRefreshAction } from './useProjectTreeRefreshAction';
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
  | 'loadChangeSummary'
  | 'pruneDeletedPath'
  | 'setExpandedPaths'
  | 'setSelectedPath'
  | 'setSelectedFile'
  | 'setActionLoading'
  | 'setActionError'
  | 'setActionMessage'
  | 'setMoveConflict'
  | 'openFile'
>;

export const useProjectTreeMutationActions = (options: UseProjectTreeMutationActionsOptions) => {
  const createActions = useProjectTreeCreateActions(options);
  const deleteAction = useProjectTreeDeleteAction(options);
  const downloadAction = useProjectTreeDownloadAction(options);
  const refreshAction = useProjectTreeRefreshAction(options);

  return {
    ...createActions,
    ...deleteAction,
    ...downloadAction,
    ...refreshAction,
  };
};
