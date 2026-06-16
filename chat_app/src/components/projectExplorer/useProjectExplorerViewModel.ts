import { useCallback } from 'react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import type { Project } from '../../types';
import { useTerminalUiSetting } from '../../hooks/useTerminalUiSetting';
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
  const client = useApiClient();

  const state = useProjectExplorerState(project?.id);
  const filesTabActive = state.workspaceTab === 'files';
  const settingsTabActive = state.workspaceTab === 'settings';
  const { terminalUiEnabled } = useTerminalUiSetting();

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
    setLoadingPaths: state.setLoadingPaths,
    setError: state.setError,
    setEntriesMap: state.setEntriesMap,
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
    enabled: settingsTabActive,
    terminalUiEnabled,
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
    handleCopyFilePath: workspaceHandleCopyFilePath,
    handleCopyRelativeFilePath: workspaceHandleCopyRelativeFilePath,
    handleIgnoreFile: workspaceHandleIgnoreFile,
    handleIgnoreFolder: workspaceHandleIgnoreFolder,
    handleIgnoreByExtension: workspaceHandleIgnoreByExtension,
    handleOpenPathInDefaultProgram: workspaceHandleOpenPathInDefaultProgram,
    handleRevealInFinder: workspaceHandleRevealInFinder,
    handleOpenInCode: workspaceHandleOpenInCode,
    handleMoveConflictCancel,
    handleMoveConflictOverwrite,
    handleMoveConflictRename,
  } = useProjectExplorerWorkspaceModel({
    project,
    filesTabActive,
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
    workspaceHandleCopyFilePath,
    workspaceHandleCopyRelativeFilePath,
    workspaceHandleIgnoreFile,
    workspaceHandleIgnoreFolder,
    workspaceHandleIgnoreByExtension,
    workspaceHandleOpenPathInDefaultProgram,
    workspaceHandleRevealInFinder,
    workspaceHandleOpenInCode,
  };
};
