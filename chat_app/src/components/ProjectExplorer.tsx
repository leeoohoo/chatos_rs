import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type {
  Project,
  FsEntry,
  ProjectRunTarget,
} from '../types';
import { cn } from '../lib/utils';
import {
  EMPTY_CHANGE_SUMMARY,
  normalizeFile,
  normalizeProjectRunCatalog,
} from './projectExplorer/utils';
import { buildSingleFileRunProfile } from './projectExplorer/runProfiles';
import { ProjectExplorerFilesWorkspace } from './projectExplorer/ProjectExplorerFilesWorkspace';
import { ProjectPreviewPane } from './projectExplorer/PreviewPane';
import { ProjectTreePane } from './projectExplorer/TreePane';
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
import { useProjectExplorerUiPersistence } from './projectExplorer/useProjectExplorerUiPersistence';

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

  const [runStatus, setRunStatus] = useState<string>('analyzing');
  const [runTargets, setRunTargets] = useState<ProjectRunTarget[]>([]);
  const [runCatalogLoading, setRunCatalogLoading] = useState(false);
  const [runCatalogError, setRunCatalogError] = useState<string | null>(null);
  const [selectedRunTargetId, setSelectedRunTargetId] = useState<string | null>(null);

  const runCwd = useMemo(() => {
    if (!project?.rootPath) {
      return '';
    }
    if (selectedEntry?.isDir) {
      return selectedEntry.path;
    }
    if (selectedEntry && !selectedEntry.isDir) {
      return getParentPath(selectedEntry.path) || project.rootPath;
    }
    if (selectedPath) {
      return getParentPath(selectedPath) || project.rootPath;
    }
    return project.rootPath;
  }, [getParentPath, project?.rootPath, selectedEntry, selectedPath]);

  const handleDispatchTerminalCommand = useCallback(async (payload: { cwd: string; command: string }) => {
    return client.dispatchTerminalCommand({
      cwd: payload.cwd,
      command: payload.command,
      project_id: project?.id,
      create_if_missing: true,
    });
  }, [client, project?.id]);

  const loadRunCatalog = useCallback(async (analyze = false) => {
    if (!project?.id) return;
    setRunCatalogLoading(true);
    setRunCatalogError(null);
    try {
      const raw = analyze
        ? await client.analyzeProjectRun(project.id)
        : await client.getProjectRunCatalog(project.id);
      const catalog = normalizeProjectRunCatalog(raw);
      setRunStatus(catalog.status || (catalog.targets.length > 0 ? 'ready' : 'empty'));
      setRunTargets(catalog.targets || []);
      const nextDefault = catalog.defaultTargetId
        ? String(catalog.defaultTargetId)
        : (catalog.targets.find((item) => item.isDefault)?.id || null);
      setSelectedRunTargetId(nextDefault);
    } catch (err: any) {
      setRunStatus('error');
      setRunTargets([]);
      setSelectedRunTargetId(null);
      setRunCatalogError(err?.message || '运行目标分析失败');
    } finally {
      setRunCatalogLoading(false);
    }
  }, [client, project?.id]);

  useEffect(() => {
    setRunStatus('analyzing');
    setRunTargets([]);
    setSelectedRunTargetId(null);
    setRunCatalogError(null);
    if (!project?.id) {
      return;
    }
    void loadRunCatalog(true);
  }, [loadRunCatalog, project?.id]);

  const handleAnalyzeRunTargets = useCallback(() => {
    void loadRunCatalog(true);
  }, [loadRunCatalog]);

  const canRunFile = useCallback((entry: FsEntry) => {
    if (entry.isDir) return false;
    return Boolean(buildSingleFileRunProfile(entry.path));
  }, []);

  const handleRunFile = useCallback(async (entry: FsEntry) => {
    const profile = buildSingleFileRunProfile(entry.path);
    if (!profile) {
      setActionError('该文件类型暂不支持直接运行');
      return;
    }
    setActionLoading(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await client.dispatchTerminalCommand({
        cwd: profile.cwd,
        command: profile.command,
        project_id: project?.id,
        create_if_missing: true,
      });
      const terminalName = String(result?.terminal_name || result?.terminal_id || '');
      setActionMessage(terminalName ? `已在终端 ${terminalName} 运行文件` : '已派发运行命令');
    } catch (err: any) {
      setActionError(err?.message || '运行文件失败');
    } finally {
      setActionLoading(false);
    }
  }, [client, project?.id, setActionError, setActionLoading, setActionMessage]);

  const isContextRootEntry = useMemo(() => {
    if (!contextMenu?.entry.path || !project?.rootPath) return false;
    return normalizePath(contextMenu.entry.path) === normalizePath(project.rootPath);
  }, [contextMenu?.entry.path, normalizePath, project?.rootPath]);

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

  const handleDragStart = useCallback((event: React.DragEvent, entry: FsEntry) => {
    if (!entry.path) return;
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(entry.path);
    setDropTargetDirPath(null);
    setMoveConflict(null);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', entry.path);
  }, [clearDragAutoScroll, clearDragExpandTimer]);

  const handleDragEnd = useCallback(() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);
  }, [clearDragAutoScroll, clearDragExpandTimer]);

  const openEntryContextMenu = useCallback((event: React.MouseEvent, entry: FsEntry) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedPath(entry.path);
    if (entry.isDir) {
      setSelectedFile(null);
    }
    setContextMenu({
      x: event.clientX,
      y: event.clientY,
      entry,
    });
  }, []);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) return undefined;
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 240 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    };
  }, [contextMenu]);

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

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  const treePaneProps: React.ComponentProps<typeof ProjectTreePane> = {
    project,
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
    onSelectProjectRoot: () => {
      void selectProjectRoot();
    },
    onToggleShowOnlyChanged: () => {
      setShowOnlyChanged((prev) => !prev);
    },
    onCreateDirectoryAtRoot: () => {
      void handleCreateDirectory(project.rootPath);
    },
    onCreateFileAtRoot: () => {
      void handleCreateFile(project.rootPath);
    },
    onRefresh: () => {
      void handleRefresh();
    },
    onConfirmCurrent: () => {
      void handleConfirmCurrentChanges();
    },
    onConfirmAll: () => {
      void handleConfirmAllChanges();
    },
    onOpenContextMenu: openEntryContextMenu,
    onSelectDeletedPath: (path) => {
      setSelectedPath(path);
      setSelectedFile(null);
    },
    onSelectMarkedPath: (path) => {
      setSelectedPath(path);
      setSelectedFile(null);
    },
    onToggleDir: (entry) => {
      void toggleDir(entry);
    },
    onOpenFile: (entry) => {
      void openFile(entry);
    },
    onDragStart: handleDragStart,
    onDragEnd: handleDragEnd,
    onSetDropTargetDirPath: setDropTargetDirPath,
    onSetDraggingEntryPath: setDraggingEntryPath,
    onMoveEntryByDrop: (sourcePath, targetDirPath) => {
      void handleMoveEntryByDrop(sourcePath, targetDirPath);
    },
    onScheduleDragExpand: scheduleDragExpand,
    onCancelDragExpandIfMatches: cancelDragExpandIfMatches,
    onClearDragExpandTimer: clearDragExpandTimer,
    onStartDragAutoScroll: startDragAutoScroll,
    onClearDragAutoScroll: clearDragAutoScroll,
  };

  const previewPaneProps: React.ComponentProps<typeof ProjectPreviewPane> = {
    selectedFile,
    selectedPath,
    selectedEntry,
    loadingFile,
    error,
    selectedLog,
    runCwd,
    projectRootPath: project.rootPath,
    onRunCommand: handleDispatchTerminalCommand,
    runTargets,
    runStatus,
    runCatalogLoading,
    runCatalogError,
    selectedRunTargetId,
    onSelectRunTarget: setSelectedRunTargetId,
    onAnalyzeRunTargets: handleAnalyzeRunTargets,
  };

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
            canRunFile={canRunFile}
            onCreateDirectory={handleCreateDirectory}
            onCreateFile={handleCreateFile}
            onRunFile={handleRunFile}
            onDownloadSelected={handleDownloadSelected}
            onDeleteSelected={handleDeleteSelected}
          />
        )}
      </div>
    </div>
  );
};

export default ProjectExplorer;
