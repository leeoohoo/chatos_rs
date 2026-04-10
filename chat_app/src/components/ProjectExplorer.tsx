import React, { useCallback, useMemo } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type {
  Project,
  FsEntry,
} from '../types';
import { cn } from '../lib/utils';
import {
  EMPTY_CHANGE_SUMMARY,
  normalizeFile,
} from './projectExplorer/utils';
import { ProjectExplorerFilesWorkspace } from './projectExplorer/ProjectExplorerFilesWorkspace';
import TeamMembersPane from './projectExplorer/TeamMembersPane';
import WorkspaceTabs from './projectExplorer/WorkspaceTabs';
import { useProjectTreeActions } from './projectExplorer/useProjectTreeActions';
import { useProjectExplorerChangeTracking } from './projectExplorer/useProjectExplorerChangeTracking';
import { useProjectExplorerDnd } from './projectExplorer/useProjectExplorerDnd';
import { useProjectExplorerDataLoading } from './projectExplorer/useProjectExplorerDataLoading';
import { useProjectExplorerLogs } from './projectExplorer/useProjectExplorerLogs';
import { useProjectExplorerPathHelpers } from './projectExplorer/useProjectExplorerPathHelpers';
import {
  useProjectExplorerProjectLifecycle,
  useProjectExplorerSummaryPolling,
} from './projectExplorer/useProjectExplorerProjectLifecycle';
import {
  useProjectExplorerState,
} from './projectExplorer/useProjectExplorerState';
import { useProjectExplorerRunState } from './projectExplorer/useProjectExplorerRunState';
import { useProjectExplorerUiPersistence } from './projectExplorer/useProjectExplorerUiPersistence';
import { useProjectExplorerWorkspaceView } from './projectExplorer/useProjectExplorerWorkspaceView';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const {
    containerRef,
    treeScrollRef,
    resizeStartX,
    resizeStartWidth,
    summaryLoadingRef,
    entriesMap,
    setEntriesMap,
    expandedPaths,
    setExpandedPaths,
    loadingPaths,
    setLoadingPaths,
    selectedPath,
    setSelectedPath,
    selectedFile,
    setSelectedFile,
    loadingFile,
    setLoadingFile,
    error,
    setError,
    actionMessage,
    setActionMessage,
    actionError,
    setActionError,
    actionLoading,
    setActionLoading,
    contextMenu,
    setContextMenu,
    moveConflict,
    setMoveConflict,
    draggingEntryPath,
    setDraggingEntryPath,
    dropTargetDirPath,
    setDropTargetDirPath,
    changeSummary,
    setChangeSummary,
    loadingSummary,
    setLoadingSummary,
    summaryError,
    setSummaryError,
    expandedReady,
    setExpandedReady,
    showOnlyChanged,
    setShowOnlyChanged,
    workspaceTab,
    setWorkspaceTab,
    treeWidth,
    setTreeWidth,
    isResizing,
    setIsResizing,
  } = useProjectExplorerState();

  const {
    normalizePath,
    rootPathNormalized,
    toExpandedKey,
    keyToPath,
    getParentPath,
  } = useProjectExplorerPathHelpers(project?.rootPath);
  const resolveParentPath = useCallback(
    (path: string | null | undefined) => getParentPath(path || '') || '',
    [getParentPath],
  );

  const { loadEntries, loadChangeSummary } = useProjectExplorerDataLoading({
    client,
    projectId: project?.id,
    summaryLoadingRef,
    setLoadingPaths,
    setError,
    setEntriesMap,
    setChangeSummary,
    setSummaryError,
    setLoadingSummary,
  });

  const {
    changeLogs,
    loadingLogs,
    logsError,
    selectedLogId,
    setSelectedLogId,
    selectedLog,
    resetLogsState,
  } = useProjectExplorerLogs({
    client,
    projectId: project?.id,
    selectedPath,
    selectedFilePath: selectedFile?.path || null,
  });

  const toggleDir = useCallback(async (entry: FsEntry) => {
    if (!entry.isDir) return;
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    const key = toExpandedKey(entry.path);
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
    if (!entriesMap[entry.path]) {
      await loadEntries(entry.path);
    }
  }, [entriesMap, loadEntries, toExpandedKey]);

  const openFile = useCallback(async (entry: FsEntry) => {
    setActionError(null);
    setSelectedPath(entry.path);
    setSelectedFile(null);
    setLoadingFile(true);
    setError(null);
    try {
      const data = await client.readFsFile(entry.path);
      setSelectedFile(normalizeFile(data));
    } catch (err: any) {
      setError(err?.message || '读取文件失败');
    } finally {
      setLoadingFile(false);
    }
  }, [client]);

  const projectRootEntry = useMemo<FsEntry | null>(() => {
    if (!project?.rootPath) return null;
    return {
      name: project.name || project.rootPath,
      path: project.rootPath,
      isDir: true,
      size: null,
      modifiedAt: null,
    };
  }, [project?.name, project?.rootPath]);

  const findEntryByPath = useCallback((path: string): FsEntry | null => {
    const normalizedTarget = normalizePath(path);
    const root = project?.rootPath ? normalizePath(project.rootPath) : '';
    if (root && normalizedTarget === root) {
      return projectRootEntry;
    }
    for (const entries of Object.values(entriesMap)) {
      const found = entries.find((entry) => normalizePath(entry.path) === normalizedTarget);
      if (found) return found;
    }
    return null;
  }, [entriesMap, normalizePath, project?.rootPath, projectRootEntry]);

  const selectedEntry = useMemo<FsEntry | null>(() => {
    if (!selectedPath) return null;
    return findEntryByPath(selectedPath);
  }, [findEntryByPath, selectedPath]);

  const selectedDirPath = useMemo(
    () => (selectedEntry?.isDir ? selectedEntry.path : null),
    [selectedEntry]
  );
  const {
    runCwd,
    runStatus,
    runTargets,
    runCatalogLoading,
    runCatalogError,
    selectedRunTargetId,
    setSelectedRunTargetId,
    handleDispatchTerminalCommand,
    handleInterruptTerminal,
    handleGetTerminal,
    handleListTerminalLogs,
    handleListTerminals,
    handleAnalyzeRunTargets,
    canRunFile,
    handleRunFile,
  } = useProjectExplorerRunState({
    client,
    project,
    selectedEntry,
    selectedPath,
    getParentPath: resolveParentPath,
    setActionError,
    setActionLoading,
    setActionMessage,
  });

  const actionReloadPath = useMemo(() => {
    if (!selectedEntry) return project?.rootPath || null;
    if (selectedEntry.isDir) return selectedEntry.path;
    return getParentPath(selectedEntry.path) || project?.rootPath || null;
  }, [getParentPath, project?.rootPath, selectedEntry]);

  const selectProjectRoot = useCallback(async () => {
    const root = project?.rootPath;
    if (!root) return;
    setSelectedPath(root);
    setSelectedFile(null);
    if (!entriesMap[root]) {
      await loadEntries(root);
    }
  }, [entriesMap, loadEntries, project?.rootPath]);

  const {
    hasPendingChangesForPath,
    canConfirmCurrent,
    aggregatedChangeKindByPath,
  } = useProjectExplorerChangeTracking({
    changeSummary,
    selectedPath,
    normalizePath,
    getParentPath,
    rootPathNormalized,
  });

  const {
    canDropToDirectory,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
  } = useProjectExplorerDnd({
    treeScrollRef,
    entriesMap,
    loadingPaths,
    normalizePath,
    toExpandedKey,
    getParentPath,
    findEntryByPath,
    loadEntries,
    setExpandedPaths,
  });

  const replaceExpandedPathPrefix = useCallback((sourcePath: string, movedPath: string) => {
    const normalizedSource = normalizePath(sourcePath);
    const normalizedMoved = normalizePath(movedPath);
    const sourcePrefix = `${normalizedSource}/`;
    const next = new Set<string>();
    expandedPaths.forEach((key) => {
      const full = normalizePath(keyToPath(key));
      if (full === normalizedSource || full.startsWith(sourcePrefix)) {
        const suffix = full.slice(normalizedSource.length);
        const nextPath = normalizePath(`${normalizedMoved}${suffix}`);
        next.add(toExpandedKey(nextPath));
      } else {
        next.add(key);
      }
    });
    return next;
  }, [expandedPaths, keyToPath, normalizePath, toExpandedKey]);

  const reloadTreeWithExpanded = useCallback(async (nextExpanded: Set<string>) => {
    if (!project?.rootPath) return;
    setEntriesMap({});
    await loadEntries(project.rootPath);
    const tasks = Array.from(nextExpanded)
      .filter((key) => key.length > 0)
      .map((key) => loadEntries(keyToPath(key)));
    if (tasks.length > 0) {
      await Promise.all(tasks);
    }
  }, [keyToPath, loadEntries, project?.rootPath]);

  const pruneDeletedPath = useCallback((deletedPath: string) => {
    const normalizedDeleted = normalizePath(deletedPath);
    const deletedPrefix = `${normalizedDeleted}/`;

    setEntriesMap((prev) => {
      const next: Record<string, FsEntry[]> = {};
      Object.entries(prev).forEach(([key, entries]) => {
        const normalizedKey = normalizePath(key);
        if (normalizedKey === normalizedDeleted || normalizedKey.startsWith(deletedPrefix)) {
          return;
        }
        next[key] = entries.filter((entry) => {
          const normalizedEntryPath = normalizePath(entry.path);
          return normalizedEntryPath !== normalizedDeleted && !normalizedEntryPath.startsWith(deletedPrefix);
        });
      });
      return next;
    });

    setExpandedPaths((prev) => {
      const next = new Set<string>();
      prev.forEach((key) => {
        const full = normalizePath(keyToPath(key));
        if (full !== normalizedDeleted && !full.startsWith(deletedPrefix)) {
          next.add(key);
        }
      });
      return next;
    });
  }, [keyToPath, normalizePath]);

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
    client,
    selectedDirPath,
    selectedEntry,
    selectedFilePath: selectedFile?.path || null,
    selectedPath,
    projectRootPath: project?.rootPath,
    projectId: project?.id,
    actionReloadPath,
    normalizePath,
    getParentPath,
    toExpandedKey,
    loadEntries,
    loadChangeSummary,
    hasPendingChangesForPath,
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
  });

  useProjectExplorerProjectLifecycle({
    projectId: project?.id,
    projectRootPath: project?.rootPath,
    toExpandedKey,
    keyToPath,
    loadEntries,
    loadChangeSummary,
    clearDragExpandTimer,
    clearDragAutoScroll,
    resetLogsState,
    summaryLoadingRef,
    setEntriesMap,
    setExpandedPaths,
    setSelectedPath,
    setSelectedFile,
    setActionMessage,
    setActionError,
    setActionLoading,
    setContextMenu,
    setMoveConflict,
    setDraggingEntryPath,
    setDropTargetDirPath,
    setChangeSummary,
    setSummaryError,
    setLoadingSummary,
    setExpandedReady,
    emptyChangeSummary: EMPTY_CHANGE_SUMMARY,
  });

  useProjectExplorerUiPersistence({
    projectId: project?.id,
    projectRootPath: project?.rootPath,
    expandedReady,
    expandedPaths,
    showOnlyChanged,
    setShowOnlyChanged,
    workspaceTab,
    setWorkspaceTab,
    contextMenu,
    setContextMenu,
    isResizing,
    resizeStartX,
    resizeStartWidth,
    setTreeWidth,
    treeWidth,
    setIsResizing,
  });

  useProjectExplorerSummaryPolling({
    projectId: project?.id,
    loadChangeSummary,
  });

  const effectiveProject: Project = project ?? {
    id: '__placeholder__',
    name: '',
    rootPath: '',
    createdAt: new Date(0),
    updatedAt: new Date(0),
  };

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
  } = useProjectExplorerWorkspaceView({
    project: effectiveProject,
    treeWidth,
    treeScrollRef,
    entriesMap,
    expandedPaths,
    loadingPaths,
    selectedPath,
    selectedEntry,
    draggingEntryPath,
    dropTargetDirPath,
    actionLoading,
    actionReloadPath,
    contextMenu,
    canConfirmCurrent,
    showOnlyChanged,
    changeSummary,
    loadingSummary,
    summaryError,
    actionMessage,
    actionError,
    aggregatedChangeKindByPath,
    normalizePath,
    toExpandedKey,
    canDropToDirectory,
    setSelectedPath,
    setSelectedFile,
    setShowOnlyChanged,
    setDraggingEntryPath,
    setDropTargetDirPath,
    setMoveConflict,
    setContextMenu,
    clearDragExpandTimer,
    cancelDragExpandIfMatches,
    scheduleDragExpand,
    clearDragAutoScroll,
    startDragAutoScroll,
    selectProjectRoot,
    toggleDir,
    openFile,
    handleCreateDirectory,
    handleCreateFile,
    handleRefresh,
    handleConfirmCurrentChanges,
    handleConfirmAllChanges,
    handleMoveEntryByDrop,
    handleDispatchTerminalCommand,
    handleInterruptTerminal,
    handleGetTerminal,
    handleListTerminalLogs,
    handleListTerminals,
    canRunFile,
    handleRunFile,
    handleDownloadSelected,
    handleDeleteSelected,
    loadingFile,
    error,
    selectedFile,
    selectedLog,
    runCwd,
    runTargets,
    runStatus,
    runCatalogLoading,
    runCatalogError,
    selectedRunTargetId,
    setSelectedRunTargetId,
    handleAnalyzeRunTargets,
  });

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  return (
    <div ref={containerRef} className={cn('flex h-full flex-col overflow-hidden', className)}>
      <WorkspaceTabs
        activeTab={workspaceTab}
        onChange={setWorkspaceTab}
      />

      <div className="flex-1 min-h-0 overflow-hidden">
        {workspaceTab === 'team' ? (
          <TeamMembersPane
            project={project}
            className="h-full"
          />
        ) : (
          <ProjectExplorerFilesWorkspace
            treePaneProps={treePaneProps}
            treeWidth={treeWidth}
            isResizing={isResizing}
            resizeStartX={resizeStartX}
            resizeStartWidth={resizeStartWidth}
            setIsResizing={setIsResizing}
            previewPaneProps={previewPaneProps}
            loadingLogs={loadingLogs}
            logsError={logsError}
            changeLogs={changeLogs}
            selectedLogId={selectedLogId}
            setSelectedLogId={setSelectedLogId}
            moveConflict={moveConflict}
            actionLoading={actionLoading}
            setMoveConflict={setMoveConflict}
            onMoveConflictCancel={handleMoveConflictCancel}
            onMoveConflictOverwrite={handleMoveConflictOverwrite}
            onMoveConflictRename={handleMoveConflictRename}
            contextMenu={contextMenu}
            contextMenuStyle={contextMenuStyle}
            isContextRootEntry={isContextRootEntry}
            setContextMenu={setContextMenu}
            canRunFile={workspaceCanRunFile}
            onCreateDirectory={workspaceHandleCreateDirectory}
            onCreateFile={workspaceHandleCreateFile}
            onRunFile={workspaceHandleRunFile}
            onDownloadSelected={workspaceHandleDownloadSelected}
            onDeleteSelected={workspaceHandleDeleteSelected}
          />
        )}
      </div>
    </div>
  );
};

export default ProjectExplorer;
