import { useCallback, useMemo } from 'react';
import type React from 'react';

import type {
  ChangeLogItem,
  FsEntry,
  FsReadResult,
  Project,
  ProjectChangeSummary,
} from '../../types';
import type { ChangeKind } from './utils';
import { ProjectPreviewPane } from './PreviewPane';
import { ProjectTreePane } from './TreePane';
import type { MoveConflictState } from './Overlays';
import type { ExplorerContextMenuState } from './useProjectExplorerState';
import type {
  ProjectRunnerActiveTerminal,
  ProjectRunnerMember,
} from './useProjectExplorerRunState';

interface UseProjectExplorerWorkspaceViewParams {
  project: Project;
  treeWidth: number;
  treeScrollRef: React.MutableRefObject<HTMLDivElement | null>;
  entriesMap: Record<string, FsEntry[]>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedPath: string | null;
  selectedEntry: FsEntry | null;
  draggingEntryPath: string | null;
  dropTargetDirPath: string | null;
  actionLoading: boolean;
  actionReloadPath: string | null;
  contextMenu: ExplorerContextMenuState | null;
  canConfirmCurrent: boolean;
  showOnlyChanged: boolean;
  changeSummary: ProjectChangeSummary;
  loadingSummary: boolean;
  summaryError: string | null;
  actionMessage: string | null;
  actionError: string | null;
  aggregatedChangeKindByPath: Map<string, ChangeKind>;
  normalizePath: (value: string) => string;
  toExpandedKey: (path: string) => string;
  canDropToDirectory: (sourcePath: string, targetDirPath: string) => boolean;
  setSelectedPath: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedFile: React.Dispatch<React.SetStateAction<FsReadResult | null>>;
  setShowOnlyChanged: React.Dispatch<React.SetStateAction<boolean>>;
  setDraggingEntryPath: React.Dispatch<React.SetStateAction<string | null>>;
  setDropTargetDirPath: React.Dispatch<React.SetStateAction<string | null>>;
  setMoveConflict: React.Dispatch<React.SetStateAction<MoveConflictState | null>>;
  setContextMenu: React.Dispatch<React.SetStateAction<ExplorerContextMenuState | null>>;
  clearDragExpandTimer: () => void;
  cancelDragExpandIfMatches: (path: string) => void;
  scheduleDragExpand: (targetDirPath: string) => void;
  clearDragAutoScroll: () => void;
  startDragAutoScroll: (velocity: number) => void;
  selectProjectRoot: () => Promise<void>;
  toggleDir: (entry: FsEntry) => Promise<void>;
  openFile: (entry: FsEntry) => Promise<void>;
  handleCreateDirectory: (path: string) => Promise<void>;
  handleCreateFile: (path: string) => Promise<void>;
  handleRefresh: () => Promise<void>;
  handleConfirmCurrentChanges: () => Promise<void>;
  handleConfirmAllChanges: () => Promise<void>;
  handleMoveEntryByDrop: (sourcePath: string, targetDirPath: string) => Promise<void>;
  canRunFile: (entry: FsEntry) => boolean;
  handleRunFile: (entry: FsEntry) => Promise<void>;
  handleDownloadSelected: (entry: FsEntry) => Promise<void>;
  handleDeleteSelected: (entry: FsEntry) => Promise<void>;
  loadingFile: boolean;
  error: string | null;
  selectedFile: FsReadResult | null;
  selectedLog: ChangeLogItem | null;
  runStatus: string;
  runCatalogLoading: boolean;
  runCatalogError: string | null;
  projectMembers: ProjectRunnerMember[];
  projectMembersLoading: boolean;
  projectMembersError: string | null;
  runnerScriptExists: boolean;
  runnerScriptChecking: boolean;
  runnerScriptPath: string;
  runnerStartCommand: string;
  runnerStopCommand: string;
  runnerRestartCommand: string;
  starting: boolean;
  stopping: boolean;
  restarting: boolean;
  runnerMessage: string | null;
  runnerError: string | null;
  activeRun: ProjectRunnerActiveTerminal | null;
  activeTerminalBusy: boolean;
  handleRunnerStart: () => Promise<void>;
  handleRunnerStop: () => Promise<void>;
  handleRunnerRestart: () => Promise<void>;
  refreshRunnerState: () => Promise<void>;
  handleGenerateRunnerScriptForContact: (member: ProjectRunnerMember) => Promise<void>;
}

export const useProjectExplorerWorkspaceView = ({
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
}: UseProjectExplorerWorkspaceViewParams) => {
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
  }, [setContextMenu, setSelectedFile, setSelectedPath]);

  const handleDragStart = useCallback((event: React.DragEvent, entry: FsEntry) => {
    if (!entry.path) {
      return;
    }
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(entry.path);
    setDropTargetDirPath(null);
    setMoveConflict(null);
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', entry.path);
  }, [
    clearDragAutoScroll,
    clearDragExpandTimer,
    setDraggingEntryPath,
    setDropTargetDirPath,
    setMoveConflict,
  ]);

  const handleDragEnd = useCallback(() => {
    clearDragExpandTimer();
    clearDragAutoScroll();
    setDraggingEntryPath(null);
    setDropTargetDirPath(null);
  }, [clearDragAutoScroll, clearDragExpandTimer, setDraggingEntryPath, setDropTargetDirPath]);

  const isContextRootEntry = useMemo(() => {
    if (!contextMenu?.entry.path || !project.rootPath) {
      return false;
    }
    return normalizePath(contextMenu.entry.path) === normalizePath(project.rootPath);
  }, [contextMenu, normalizePath, project.rootPath]);

  const contextMenuStyle = useMemo(() => {
    if (!contextMenu) {
      return undefined;
    }
    const maxX = typeof window !== 'undefined' ? window.innerWidth - 220 : contextMenu.x;
    const maxY = typeof window !== 'undefined' ? window.innerHeight - 240 : contextMenu.y;
    return {
      left: `${Math.max(8, Math.min(contextMenu.x, maxX))}px`,
      top: `${Math.max(8, Math.min(contextMenu.y, maxY))}px`,
    } satisfies React.CSSProperties;
  }, [contextMenu]);

  const treePaneProps: React.ComponentProps<typeof ProjectTreePane> = useMemo(() => ({
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
  }), [
    actionError,
    actionLoading,
    actionMessage,
    actionReloadPath,
    aggregatedChangeKindByPath,
    canConfirmCurrent,
    canDropToDirectory,
    cancelDragExpandIfMatches,
    changeSummary,
    clearDragAutoScroll,
    clearDragExpandTimer,
    draggingEntryPath,
    dropTargetDirPath,
    entriesMap,
    expandedPaths,
    handleConfirmAllChanges,
    handleConfirmCurrentChanges,
    handleCreateDirectory,
    handleCreateFile,
    handleMoveEntryByDrop,
    handleRefresh,
    loadingPaths,
    loadingSummary,
    normalizePath,
    openEntryContextMenu,
    openFile,
    project,
    scheduleDragExpand,
    selectProjectRoot,
    selectedEntry,
    selectedPath,
    setDraggingEntryPath,
    setDropTargetDirPath,
    setSelectedFile,
    setSelectedPath,
    setShowOnlyChanged,
    showOnlyChanged,
    startDragAutoScroll,
    summaryError,
    toExpandedKey,
    toggleDir,
    treeScrollRef,
    treeWidth,
  ]);

  const previewPaneProps: React.ComponentProps<typeof ProjectPreviewPane> = useMemo(() => ({
    selectedFile,
    selectedPath,
    selectedEntry,
    loadingFile,
    error,
    selectedLog,
    projectRootPath: project.rootPath,
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
    onRunnerStart: () => {
      void handleRunnerStart();
    },
    onRunnerStop: () => {
      void handleRunnerStop();
    },
    onRunnerRestart: () => {
      void handleRunnerRestart();
    },
    onRefreshRunnerState: () => {
      void refreshRunnerState();
    },
    onGenerateRunnerScriptForContact: handleGenerateRunnerScriptForContact,
  }), [
    activeRun,
    activeTerminalBusy,
    error,
    handleGenerateRunnerScriptForContact,
    handleRunnerRestart,
    handleRunnerStart,
    handleRunnerStop,
    loadingFile,
    projectMembers,
    projectMembersError,
    projectMembersLoading,
    project.rootPath,
    refreshRunnerState,
    runCatalogError,
    runCatalogLoading,
    runStatus,
    runnerError,
    runnerMessage,
    runnerRestartCommand,
    runnerScriptChecking,
    runnerScriptExists,
    runnerScriptPath,
    runnerStartCommand,
    runnerStopCommand,
    restarting,
    selectedEntry,
    selectedFile,
    selectedLog,
    selectedPath,
    starting,
    stopping,
  ]);

  return {
    openEntryContextMenu,
    handleDragStart,
    handleDragEnd,
    contextMenuStyle,
    isContextRootEntry,
    treePaneProps,
    previewPaneProps,
    canRunFile,
    handleRunFile,
    handleCreateDirectory,
    handleCreateFile,
    handleDownloadSelected,
    handleDeleteSelected,
  };
};
