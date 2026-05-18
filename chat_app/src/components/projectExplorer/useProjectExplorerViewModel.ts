import { useCallback } from 'react';

import { apiClient as globalApiClient } from '../../lib/api/client';
import { useChatApiClientFromContext } from '../../lib/store/ChatStoreContext';
import type { Project } from '../../types';
import { useProjectExplorerCodeNav } from './useProjectExplorerCodeNav';
import { useProjectExplorerDataLoading } from './useProjectExplorerDataLoading';
import { useProjectExplorerPathHelpers } from './useProjectExplorerPathHelpers';
import { useProjectExplorerRunState } from './useProjectExplorerRunState';
import { useProjectExplorerSearch } from './useProjectExplorerSearch';
import { useProjectExplorerSelection } from './useProjectExplorerSelection';
import { useProjectExplorerState } from './useProjectExplorerState';
import { useProjectExplorerTreeStateOps } from './useProjectExplorerTreeStateOps';
import { useProjectExplorerWorkspaceModel } from './useProjectExplorerWorkspaceModel';

interface UseProjectExplorerViewModelParams {
  project: Project | null;
}

export const useProjectExplorerViewModel = ({
  project,
}: UseProjectExplorerViewModelParams) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = apiClientFromContext || globalApiClient;

  const state = useProjectExplorerState(project?.id);

  const pathHelpers = useProjectExplorerPathHelpers(project?.rootPath);

  const search = useProjectExplorerSearch({
    client,
    projectRootPath: project?.rootPath,
  });

  const resolveParentPath = useCallback(
    (path: string | null | undefined) => pathHelpers.getParentPath(path || '') || '',
    [pathHelpers.getParentPath],
  );
  const resolveNormalizedPath = useCallback(
    (path: string | null | undefined) => pathHelpers.normalizePath(path || ''),
    [pathHelpers.normalizePath],
  );

  const dataLoading = useProjectExplorerDataLoading({
    client,
    projectId: project?.id,
    projectRootPath: project?.rootPath,
    summaryLoadingRef: state.summaryLoadingRef,
    setLoadingPaths: state.setLoadingPaths,
    setError: state.setError,
    setEntriesMap: state.setEntriesMap,
    setChangeSummary: state.setChangeSummary,
    setSummaryError: state.setSummaryError,
    setLoadingSummary: state.setLoadingSummary,
  });

  const selection = useProjectExplorerSelection({
    client,
    project,
    entriesMap: state.entriesMap,
    selectedPath: state.selectedPath,
    clearSearchNavigation: search.clearSearchNavigation,
    normalizePath: resolveNormalizedPath,
    getParentPath: resolveParentPath,
    loadEntries: dataLoading.loadEntries,
    setActionError: state.setActionError,
    setSelectedPath: state.setSelectedPath,
    setSelectedFile: state.setSelectedFile,
    setLoadingFile: state.setLoadingFile,
    setError: state.setError,
    setPreviewTargetLine: search.setPreviewTargetLine,
  });

  const runState = useProjectExplorerRunState({
    client,
    project,
    selectedEntry: selection.selectedEntry,
    selectedPath: state.selectedPath,
    getParentPath: resolveParentPath,
    setActionError: state.setActionError,
    setActionLoading: state.setActionLoading,
    setActionMessage: state.setActionMessage,
  });

  const codeNav = useProjectExplorerCodeNav({
    client,
    projectRootPath: project?.rootPath,
    selectedFilePath: state.selectedFile?.path || null,
    targetLine: search.previewTargetLine,
    openLocation: selection.openCodeNavLocation,
  });

  const treeStateOps = useProjectExplorerTreeStateOps({
    projectRootPath: project?.rootPath,
    entriesMap: state.entriesMap,
    expandedPaths: state.expandedPaths,
    keyToPath: pathHelpers.keyToPath,
    normalizePath: pathHelpers.normalizePath,
    toExpandedKey: pathHelpers.toExpandedKey,
    loadEntries: dataLoading.loadEntries,
    loadChangeSummary: dataLoading.loadChangeSummary,
    clearSearch: search.clearSearch,
    clearSearchNavigation: search.clearSearchNavigation,
    clearTokenSelection: codeNav.clearTokenSelection,
    setEntriesMap: state.setEntriesMap,
    setExpandedPaths: state.setExpandedPaths,
    setSelectedPath: state.setSelectedPath,
    setSelectedFile: state.setSelectedFile,
    setActionError: state.setActionError,
    setError: state.setError,
  });

  const {
    treePaneProps,
    previewPaneProps,
    projectSettingsProps,
    contextMenuStyle,
    isContextRootEntry,
    handleCreateDirectory: workspaceHandleCreateDirectory,
    handleCreateFile: workspaceHandleCreateFile,
    handleDownloadSelected: workspaceHandleDownloadSelected,
    handleDeleteSelected: workspaceHandleDeleteSelected,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  } = useProjectExplorerWorkspaceModel({
    project,
    client,
    state,
    pathHelpers,
    search,
    dataLoading,
    selection,
    runState,
    codeNav,
    treeStateOps,
  });

  return {
    client,
    containerRef: state.containerRef,
    workspaceTab: state.workspaceTab,
    setWorkspaceTab: state.setWorkspaceTab,
    handleGitRepositoryChanged: treeStateOps.handleGitRepositoryChanged,
    treePaneProps,
    treeWidth: state.treeWidth,
    isResizing: state.isResizing,
    resizeStartX: state.resizeStartX,
    resizeStartWidth: state.resizeStartWidth,
    setIsResizing: state.setIsResizing,
    previewPaneProps,
    projectSettingsProps,
    moveConflict: state.moveConflict,
    actionLoading: state.actionLoading,
    setMoveConflict: state.setMoveConflict,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
    contextMenu: state.contextMenu,
    contextMenuStyle,
    isContextRootEntry,
    setContextMenu: state.setContextMenu,
    workspaceHandleCreateDirectory,
    workspaceHandleCreateFile,
    workspaceHandleDownloadSelected,
    workspaceHandleDeleteSelected,
  };
};
