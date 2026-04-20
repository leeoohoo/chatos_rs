import React, { useCallback, useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import { apiClient as globalApiClient } from '../lib/api/client';
import {
  useChatApiClientFromContext,
  useChatStoreSelector,
} from '../lib/store/ChatStoreContext';
import type {
  CodeNavLocation,
  Project,
  FsEntry,
} from '../types';
import { cn } from '../lib/utils';
import { useContactSessionResolver } from '../features/contactSession/useContactSessionResolver';
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
import { useProjectExplorerSearch } from './projectExplorer/useProjectExplorerSearch';
import { useProjectExplorerCodeNav } from './projectExplorer/useProjectExplorerCodeNav';
import {
  useProjectExplorerRunState,
  type ProjectRunnerMember,
} from './projectExplorer/useProjectExplorerRunState';
import { useProjectExplorerUiPersistence } from './projectExplorer/useProjectExplorerUiPersistence';
import { useProjectExplorerWorkspaceView } from './projectExplorer/useProjectExplorerWorkspaceView';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

const RUNNER_SCRIPT_REL_PATH = '.chatos/project_runner.sh';
const RUNNER_LOG_DIR_REL_PATH = 'project_runner/logs';
const RUNNER_GENERATION_MCP_IDS = [
  'builtin_code_maintainer_read',
  'builtin_code_maintainer_write',
  'builtin_terminal_controller',
];

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const {
    currentSession,
    sessions,
    createSession,
    selectSession,
    sendMessage,
    selectedModelId,
  } = useChatStoreSelector((state) => ({
    currentSession: state.currentSession,
    sessions: state.sessions,
    createSession: state.createSession,
    selectSession: state.selectSession,
    sendMessage: state.sendMessage,
    selectedModelId: state.selectedModelId,
  }), shallow);
  const { ensureContactSession } = useContactSessionResolver({
    sessions: sessions || [],
    currentSession,
    createSession,
    apiClient: client,
    defaultProjectId: project?.id || null,
  });
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
  const {
    searchQuery,
    setSearchQuery,
    searchCaseSensitive,
    setSearchCaseSensitive,
    searchWholeWord,
    setSearchWholeWord,
    searchResults,
    searchLoading,
    searchError,
    searchTruncated,
    activeSearchHitId,
    activeSearchHitIndex,
    totalSearchHits,
    previewTargetLine,
    previewTargetLineRevision,
    setPreviewTargetLine,
    canOpenPreviousSearchHit,
    canOpenNextSearchHit,
    runSearchQuery,
    clearSearch,
    clearSearchNavigation,
    activateSearchHit,
    handleOpenSearchHit,
    openPreviousSearchHit,
    openNextSearchHit,
  } = useProjectExplorerSearch({
    client,
    projectRootPath: project?.rootPath,
  });
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
    clearSearchNavigation();
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
  }, [clearSearchNavigation, client]);

  const openCodeNavLocation = useCallback(async (location: CodeNavLocation) => {
    await openFile({
      name: location.relativePath.split('/').filter(Boolean).pop() || location.path.split(/[\\/]/).pop() || location.path,
      path: location.path,
      isDir: false,
      size: null,
      modifiedAt: null,
    });
    setPreviewTargetLine(location.line);
  }, [openFile, setPreviewTargetLine]);

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
    runStatus,
    runCatalogLoading,
    runCatalogError,
    canRunFile,
    handleRunFile,
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    runnerScriptExists,
    runnerScriptChecking,
    runnerScriptPath,
    runnerStartCommand,
    runnerStopCommand,
    runnerRestartCommand,
    starting,
    stopping,
    restarting,
    runnerMessage,
    runnerError,
    activeRun,
    activeTerminalBusy,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
    refreshRunnerState,
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
  const {
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    selectedToken,
    selectedTokenLine,
    selectedTokenColumn,
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
    handleTokenSelection,
    clearTokenSelection,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
  } = useProjectExplorerCodeNav({
    client,
    projectRootPath: project?.rootPath,
    selectedFilePath: selectedFile?.path || null,
    openLocation: openCodeNavLocation,
  });
  const handlePreviewTokenSelection = useCallback((selection: {
    token: string;
    line: number;
    column: number;
  } | null) => {
    handleTokenSelection(selection);
    if (selection?.line) {
      setPreviewTargetLine(selection.line);
    }
  }, [handleTokenSelection, setPreviewTargetLine]);
  const handleOpenDocumentSymbol = useCallback((line: number) => {
    setPreviewTargetLine(line);
  }, [setPreviewTargetLine]);

  const handleGenerateRunnerScriptForContact = useCallback(async (member: ProjectRunnerMember) => {
    if (!project?.id || !project?.rootPath) {
      throw new Error('当前项目不存在或根目录为空');
    }
    const contactId = typeof member?.contactId === 'string' ? member.contactId.trim() : '';
    const contactAgentId = typeof member?.agentId === 'string' ? member.agentId.trim() : '';
    if (!contactId || !contactAgentId) {
      throw new Error('联系人信息不完整，无法生成启动脚本');
    }
    const sessionId = await ensureContactSession({
      id: contactId,
      agentId: contactAgentId,
      name: member.name,
    }, {
      projectId: project.id,
      title: member.name || '项目运行助手',
      selectedModelId: selectedModelId ?? null,
      projectRoot: project.rootPath,
      mcpEnabled: true,
      enabledMcpIds: RUNNER_GENERATION_MCP_IDS,
      createSessionOptions: { keepActivePanel: true },
    });
    if (!sessionId) {
      throw new Error('未能创建或定位联系人会话');
    }
    if (currentSession?.id !== sessionId) {
      await selectSession(sessionId, { keepActivePanel: true });
    }

    const prompt = [
      `你是项目运行脚本生成助手。请在项目根目录 ${project.rootPath} 下创建文件 ${RUNNER_SCRIPT_REL_PATH}。`,
      '',
      '目标：',
      '1) 生成一个 bash 脚本，支持参数 start / stop / restart。',
      '2) start: 启动当前项目下所有可启动服务（前端、后端、worker 等都包含，能启动的都要启动）。',
      '3) stop: 停止 start 启动的全部进程（优先使用 pid 文件，避免误杀非本脚本启动进程）。',
      '4) restart: 等价于 stop + start。',
      `5) 所有服务日志必须写入 ${project.rootPath}/${RUNNER_LOG_DIR_REL_PATH}/。`,
      '',
      '强制要求：',
      '1) 先读取项目关键文件（如 package.json / pyproject.toml / Cargo.toml / go.mod / pom.xml 等）再决策。',
      '2) 可使用终端工具做必要探测（如命令是否存在）。',
      '3) 脚本必须可执行（#!/usr/bin/env bash，set -euo pipefail）。',
      `4) 必须创建日志目录 ${project.rootPath}/${RUNNER_LOG_DIR_REL_PATH}/，并按服务拆分日志文件（例如 frontend.log、backend.log）。`,
      '5) 若无法确定某服务启动命令，要在注释与日志里明确标记该服务待人工补充，但其他可启动服务仍需正常启动。',
      '6) 禁止把后端端口写死为 3997 或其它固定值；每个服务启动前必须检测端口是否可用，不可用时自动选择可用端口。',
      '7) 必须把实际使用端口写入 project_runner/runtime/ports.env，重启时优先复用该文件中的端口配置。',
      '8) stop 只能按本脚本维护的 pid 文件停止，不允许按端口全局 kill，避免误伤其他项目服务。',
      `9) 完成后请回复：脚本已生成: ${RUNNER_SCRIPT_REL_PATH}`,
    ].join('\n');

    await sendMessage(prompt, [], {
      mcpEnabled: true,
      enabledMcpIds: RUNNER_GENERATION_MCP_IDS,
      contactAgentId,
      contactId,
      projectId: project.id,
      projectRoot: project.rootPath,
      workspaceRoot: null,
    });
  }, [
    currentSession?.id,
    ensureContactSession,
    project?.id,
    project?.rootPath,
    selectSession,
    selectedModelId,
    sendMessage,
  ]);

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
    searchQuery,
    searchCaseSensitive,
    searchWholeWord,
    searchResults,
    searchLoading,
    searchError,
    searchTruncated,
    activeSearchHitId,
    activeSearchHitIndex,
    totalSearchHits,
    previewTargetLine,
    previewTargetLineRevision,
    navCapabilities,
    navCapabilitiesLoading,
    navCapabilitiesError,
    selectedToken,
    selectedTokenLine,
    selectedTokenColumn,
    navResult,
    navRequestKind,
    navLoading,
    navError,
    activeNavLocationId,
    documentSymbols,
    documentSymbolsLoading,
    documentSymbolsError,
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
    handleSearchQueryChange: setSearchQuery,
    handleSearchCaseSensitiveChange: setSearchCaseSensitive,
    handleSearchWholeWordChange: setSearchWholeWord,
    handleSearchInProject: runSearchQuery,
    canOpenPreviousSearchHit,
    canOpenNextSearchHit,
    handleClearSearch: clearSearch,
    handleActivateSearchHit: activateSearchHit,
    handleOpenSearchHit: (hit) => handleOpenSearchHit(hit, openFile),
    handleOpenPreviousSearchHit: () => openPreviousSearchHit(openFile),
    handleOpenNextSearchHit: () => openNextSearchHit(openFile),
    handleTokenSelection: handlePreviewTokenSelection,
    clearTokenSelection,
    requestDefinition,
    requestReferences,
    handleOpenNavLocation,
    handleOpenDocumentSymbol,
    handleMoveEntryByDrop,
    canRunFile,
    handleRunFile,
    handleDownloadSelected,
    handleDeleteSelected,
    loadingFile,
    error,
    selectedFile,
    selectedLog,
    runStatus,
    runCatalogLoading,
    runCatalogError,
    projectMembers,
    projectMembersLoading,
    projectMembersError,
    runnerScriptExists,
    runnerScriptChecking,
    runnerScriptPath,
    runnerStartCommand,
    runnerStopCommand,
    runnerRestartCommand,
    starting,
    stopping,
    restarting,
    runnerMessage,
    runnerError,
    activeRun,
    activeTerminalBusy,
    handleRunnerStart,
    handleRunnerStop,
    handleRunnerRestart,
    refreshRunnerState,
    handleGenerateRunnerScriptForContact,
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
