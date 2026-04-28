import { useCallback } from 'react';

import type { Project } from '../../types';
import { useProjectExplorerDataLoading } from './useProjectExplorerDataLoading';
import { useProjectExplorerLogs } from './useProjectExplorerLogs';
import { useProjectExplorerPathHelpers } from './useProjectExplorerPathHelpers';
import { useProjectExplorerState } from './useProjectExplorerState';
import { useProjectExplorerSearch } from './useProjectExplorerSearch';
import { useProjectExplorerCodeNav } from './useProjectExplorerCodeNav';
import { useProjectExplorerRunState } from './useProjectExplorerRunState';
import { useProjectExplorerSelection } from './useProjectExplorerSelection';
import { useProjectExplorerSessionBridge } from './useProjectExplorerSessionBridge';
import { useProjectExplorerTreeStateOps } from './useProjectExplorerTreeStateOps';
import { useProjectExplorerWorkspaceModel } from './useProjectExplorerWorkspaceModel';

interface UseProjectExplorerViewModelParams {
  project: Project | null;
}

export const useProjectExplorerViewModel = ({
  project,
}: UseProjectExplorerViewModelParams) => {
  const { client, handleGenerateRunnerScriptForContact } = useProjectExplorerSessionBridge({
    project,
  });

  const state = useProjectExplorerState();

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
    summaryLoadingRef: state.summaryLoadingRef,
    setLoadingPaths: state.setLoadingPaths,
    setError: state.setError,
    setEntriesMap: state.setEntriesMap,
    setChangeSummary: state.setChangeSummary,
    setSummaryError: state.setSummaryError,
    setLoadingSummary: state.setLoadingSummary,
  });

  const logs = useProjectExplorerLogs({
    client,
    projectId: project?.id,
    selectedPath: state.selectedPath,
    selectedFilePath: state.selectedFile?.path || null,
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
    contextMenuStyle,
    isContextRootEntry,
    canRunFile: workspaceCanRunFile,
    handleRunFile: workspaceHandleRunFile,
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
    logs,
    selection,
    runState,
    codeNav,
    treeStateOps,
    handleGenerateRunnerScriptForContact,
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
    loadingLogs: logs.loadingLogs,
    logsError: logs.logsError,
    changeLogs: logs.changeLogs,
    selectedLogId: logs.selectedLogId,
    setSelectedLogId: logs.setSelectedLogId,
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
    workspaceCanRunFile,
    workspaceHandleCreateDirectory,
    workspaceHandleCreateFile,
    workspaceHandleRunFile,
    workspaceHandleDownloadSelected,
    workspaceHandleDeleteSelected,
  };
};
